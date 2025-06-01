use std::path::{Path, PathBuf};

use super::Result;

use crate::{config::WebDavInfo, paths::PathExt};
use reqwest::{
    Method, StatusCode,
    blocking::{Client, RequestBuilder},
};
use tracing::debug;

pub struct WebDavStore {
    client: Client,
    cfg: WebDavInfo,
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

impl WebDavStore {
    pub fn new(cfg: WebDavInfo) -> Self {
        Self {
            client: Client::new(),
            cfg,
        }
    }

    fn mk_req_abs(&self, method: Method, url: &str) -> RequestBuilder {
        debug!("dispatching {method:?} request to {url}");
        self.client
            .request(method, url)
            .basic_auth(&self.cfg.username, self.cfg.psk.as_deref())
    }

    fn mk_req(&self, method: Method, path: &Path) -> RequestBuilder {
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
        self.mk_req_abs(method, &url)
    }

    /// creates a single directory, requires parents to be created
    ///
    /// Requires that dir is already parented to root
    fn mkdir_abs(&self, dir: &Path) -> Result<()> {
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
            .send()?;
        resp.error_for_status()?;
        Ok(())
    }

    fn mkdir_all(&self, dir: &Path) -> Result<()> {
        let dir = self.cfg.root.join_good(dir);
        debug!("mkdir all for {dir:?}");
        assert!(dir.is_absolute());
        let parts = calc_mkdir_all_paths(&dir);
        for p in parts.into_iter().skip(1) {
            self.mkdir_abs(&p)?;
        }
        Ok(())
    }
}

impl WebDavStore {
    pub fn write_file(&mut self, at: &Path, bytes: &[u8]) -> super::Result<()> {
        debug!("writing to {at:?}");
        if !self.exists(at.parent().expect("no parent path for file"))? {
            debug!("creating parent directories for {at:?}");
            self.mkdir_all(at.parent().unwrap())?;
        }
        let resp = self.mk_req(Method::PUT, at).body(bytes.to_owned()).send()?;
        if resp.status() == StatusCode::CONFLICT {
            panic!("invalidly scoped but we should've checked for that?");
        } else {
            resp.error_for_status()?;
        }
        Ok(())
    }

    pub fn read_file(&self, at: &Path) -> super::Result<Vec<u8>> {
        debug!("read {at:?}");
        let data = self.mk_req(Method::GET, at).send()?.error_for_status()?;
        let d = data.bytes()?;
        Ok(d.to_vec())
    }

    pub fn exists(&self, f: &Path) -> super::Result<bool> {
        debug!("check exists for {f:?}");
        let req = self.mk_req(Method::GET, f).send()?;
        Ok(req.status() != StatusCode::NOT_FOUND)
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
