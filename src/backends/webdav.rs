use std::path::{Path, PathBuf};

use super::Result;

use crate::{config::WebDavInfo, paths::PathExt, secrets::SecretsApi};
use reqwest::{
    Method, StatusCode, {Client, RequestBuilder},
};
use tracing::debug;

pub struct WebDavStore<'s> {
    client: Client,
    cfg: WebDavInfo,
    secrets: &'s SecretsApi<'s>,
}

fn calc_mkdir_all_paths(dir: &Path) -> Vec<PathBuf> {
    let mut r = dir.components().fold(vec![], |mut bs, v| {
        if bs.is_empty() {
            bs.push(PathBuf::from(v.as_os_str()));
        } else {
            bs.push(bs[bs.len() - 1].join(Path::new(v.as_os_str())))
        }
        bs
    });
    if dir.is_absolute() {
        r.remove(0);
    }
    r
}

impl<'s> WebDavStore<'s> {
    pub fn new(cfg: WebDavInfo, secrets: &'s SecretsApi) -> Self {
        Self {
            client: Client::new(),
            cfg,
            secrets,
        }
    }

    async fn mk_req_abs(&self, method: Method, url: &str) -> Result<RequestBuilder> {
        debug!("dispatching {method:?} request to {url}");
        let psk = self.cfg.psk.as_ref().map(|s| async move {
            match s {
                crate::config::Secret::SystemSecret(name) => {
                    assert!(
                        self.secrets.available(),
                        "system secrets must be available to use them in a config"
                    );
                    let s = self.secrets.get_item(name).await?;
                    let sv = s.ok_or_else(|| {
                        super::BackendError::CouldNotLocateSecret(name.to_owned())
                    })?;
                    let s = String::from_utf8(sv).expect("failed to convert from secret to utf8");
                    Ok::<_, super::BackendError>(s)
                }
                crate::config::Secret::Plain(p) => Ok(p.to_owned()),
            }
        });
        let psk = if let Some(p) = psk {
            Some(p.await?)
        } else {
            None
        };
        Ok(self
            .client
            .request(method, url)
            .basic_auth(&self.cfg.username, psk.as_deref()))
    }

    async fn mk_req(&self, method: Method, path: &Path) -> Result<RequestBuilder> {
        let url = Path::new(&self.cfg.url)
            .join_good(
                self.cfg
                    .root
                    .join(path)
                    .to_str()
                    .expect("failed to concat path"),
            )
            .to_str()
            .unwrap()
            .to_owned();
        self.mk_req_abs(method, &url).await
    }

    /// creates a single directory, requires parents to be created
    ///
    /// Requires that dir is already parented to root
    async fn mkdir_abs(&self, dir: &Path) -> Result<()> {
        debug!("mkdir for {dir:?}");
        let url = Path::new(&self.cfg.url)
            .join_good(dir)
            .to_str()
            .unwrap()
            .to_owned();
        let resp = self
            .mk_req_abs(
                Method::from_bytes(b"MKCOL").expect("failed to make mkcol method"),
                &url,
            )
            .await?
            .send()
            .await?;
        resp.error_for_status()?;
        Ok(())
    }

    async fn mkdir_all(&self, dir: &Path) -> Result<()> {
        let dir = self.cfg.root.join_good(dir);
        debug!("mkdir all for {dir:?}");
        assert!(dir.is_absolute());
        let parts = calc_mkdir_all_paths(&dir);
        for p in parts.into_iter().skip(1) {
            self.mkdir_abs(&p).await?;
        }
        Ok(())
    }
}

impl WebDavStore<'_> {
    pub async fn write_file(&self, at: &Path, bytes: &[u8]) -> super::Result<()> {
        debug!("writing to {at:?}");
        if !self
            .exists(at.parent().expect("no parent path for file"))
            .await?
        {
            debug!("creating parent directories for {at:?}");
            self.mkdir_all(at.parent().unwrap()).await?;
        }
        let resp = self
            .mk_req(Method::PUT, at)
            .await?
            .body(bytes.to_owned())
            .send()
            .await?;
        if resp.status() == StatusCode::CONFLICT {
            panic!("invalidly scoped but we should've checked for that?");
        } else {
            resp.error_for_status()?;
        }
        Ok(())
    }

    pub async fn read_file(&self, at: &Path) -> super::Result<Vec<u8>> {
        debug!("read {at:?}");
        let data = self
            .mk_req(Method::GET, at)
            .await?
            .send()
            .await?
            .error_for_status()?;
        let d = data.bytes().await?;
        Ok(d.to_vec())
    }

    pub async fn exists(&self, f: &Path) -> super::Result<bool> {
        debug!("check exists for {f:?}");
        let req = self.mk_req(Method::GET, f).await?.send().await?;
        if req.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else {
            req.error_for_status()?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::calc_mkdir_all_paths;

    #[test]
    fn calc_mkdir_all_paths_gives_individual_segments() {
        let paths = calc_mkdir_all_paths(Path::new("/hello/world/hmm"));
        assert_eq!(
            paths.as_slice(),
            &["/hello", "/hello/world", "/hello/world/hmm"].map(PathBuf::from)
        )
    }
}
