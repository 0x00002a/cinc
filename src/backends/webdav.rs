use std::path::Path;

use crate::config::WebDavInfo;
use reqwest::{
    Method, StatusCode,
    blocking::{Client, RequestBuilder},
};

use super::StorageBackend;

pub struct WebDavStore {
    client: Client,
    cfg: WebDavInfo,
}

impl WebDavStore {
    pub fn new(cfg: WebDavInfo) -> Self {
        Self {
            client: Client::new(),
            cfg,
        }
    }

    fn mk_req(&self, method: Method, path: &Path) -> RequestBuilder {
        let url = self.cfg.url.to_owned()
            + self
                .cfg
                .root
                .join(path)
                .to_str()
                .expect("failed to concat path");
        self.client
            .request(method, url)
            .basic_auth(&self.cfg.username, Some(&self.cfg.psk))
    }
}

impl StorageBackend for WebDavStore {
    fn write_file(&mut self, at: &Path, bytes: &[u8]) -> super::Result<()> {
        self.mk_req(Method::PUT, at).body(bytes.to_owned()).send()?;
        Ok(())
    }

    fn read_file(&self, at: &Path) -> super::Result<Vec<u8>> {
        let data = self.mk_req(Method::GET, at).send()?;
        let d = data.bytes()?;
        Ok(d.to_vec())
    }

    fn exists(&self, f: &Path) -> super::Result<bool> {
        let req = self.mk_req(Method::GET, f).send()?;
        Ok(req.status() == StatusCode::NOT_FOUND)
    }
}
