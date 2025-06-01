use std::collections::HashMap;

use anyhow::Result;
use secret_service::{Collection, EncryptionType, SecretService};
use tracing::debug;

const ATTR_ID: &str = "id";
const ATTR_SERVICE: &str = "service";

struct Inner<'s> {
    hdl: SecretService<'s>,
}

impl<'s> Inner<'s> {
    #[allow(unused)]
    async fn list_collections(&self) -> Result<()> {
        let cs = self.hdl.get_all_collections().await?;
        for c in cs {
            println!("{}: {}", c.get_label().await?, c.is_locked().await?);
            for i in c.get_all_items().await? {
                println!("  {}", i.get_label().await?);
            }
        }
        Ok(())
    }
    async fn collection(&self) -> Result<Collection<'_>, secret_service::Error> {
        self.hdl.get_default_collection().await
    }
}

/// Wrapper for system secrets API
///
/// Note that the methods on this object panic if there is no available secrets API
pub struct SecretsApi<'s> {
    i: Option<Inner<'s>>,
}
fn mk_cinc_attrs(id: &str) -> HashMap<&str, &str> {
    let mut attrs = HashMap::new();
    attrs.insert(ATTR_ID, id);
    attrs.insert(ATTR_SERVICE, "cinc");
    attrs
}

impl<'s> SecretsApi<'s> {
    pub async fn new() -> Result<Self> {
        let i = match SecretService::connect(EncryptionType::Dh).await {
            Ok(s) => Some(Inner { hdl: s }),
            Err(e) => match e {
                secret_service::Error::Unavailable => None,
                _ => unreachable!("secrets api returned an error we didn't expect {e:?}"),
            },
        };
        let _ = i.as_ref().unwrap().collection().await;
        Ok(Self { i })
    }
    /// Whether the secrets API is available on this system
    pub fn available(&self) -> bool {
        self.i.is_some()
    }
    /// Remove IDs that are unused
    pub async fn garbage_collect(&self, used_ids: &[&str]) -> Result<()> {
        debug!("gc ids, used: {used_ids:?}");
        let hdl = self.i.as_ref().expect("no available secrets API");
        let c = hdl.collection().await?;
        let mut q = HashMap::new();
        q.insert(ATTR_SERVICE, "cinc");
        for item in c.search_items(q).await? {
            let attrs = item.get_attributes().await?;
            if !used_ids.contains(&&*attrs[ATTR_ID]) {
                item.delete().await?;
            }
        }
        Ok(())
    }

    pub async fn add_item(&self, label: &str, secret: &str) -> Result<()> {
        debug!("storing secret '{label}'");
        let hdl = self.i.as_ref().expect("no available secrets API");
        hdl.collection()
            .await?
            .create_item(
                &format!("cinc secret {label}"),
                mk_cinc_attrs(label),
                secret.as_bytes(),
                true,
                "text/plain",
            )
            .await?;
        Ok(())
    }

    pub async fn get_item(&self, label: &str) -> Result<Option<Vec<u8>>, secret_service::Error> {
        debug!("getting secret '{label}'");
        let hdl = self.i.as_ref().expect("no available secrets API");
        let items = hdl.collection().await?;
        let items = items.search_items(mk_cinc_attrs(label)).await?;
        let s = items.first().map(|i| i.get_secret());
        if let Some(s) = s {
            Ok(Some(s.await?))
        } else {
            Ok(None)
        }
    }
}
