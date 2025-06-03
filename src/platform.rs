use std::time::SystemTime;

use crate::{
    args::{LaunchArgs, PlatformOpt},
    backends::StorageBackend,
    config::{Config, SteamId},
    manifest::{GameManifest, GameManifests},
    secrets::SecretsApi,
    sync::SyncMgr,
    time,
    ui::{self, SyncChoices},
};
use anyhow::Result;
use anyhow::{anyhow, bail};
use tracing::{debug, error, info, warn};

pub enum PlatformInfo {
    Steam {
        app_id: SteamId,
        manifest_id: SteamId,
    },
    Umu {},
}
impl PlatformInfo {
    fn find_game_in_manifest<'a>(
        &self,
        manifests: &'a GameManifests,
    ) -> Option<(&'a str, &'a GameManifest)> {
        match self {
            PlatformInfo::Steam { manifest_id, .. } => manifests
                .iter()
                .find(|(_, m)| {
                    m.steam
                        .as_ref()
                        .map(|i| &i.id == manifest_id)
                        .unwrap_or(false)
                })
                .map(|(s, g)| (s.as_str(), g)),
            PlatformInfo::Umu {} => todo!(),
        }
    }
}

pub struct LaunchInfo<'s, 'm> {
    platform: PlatformInfo,
    b: StorageBackend<'s>,
    bname: String,
    game: &'m GameManifest,
}

impl<'s, 'm> LaunchInfo<'s, 'm> {
    pub fn new(
        cfg: &Config,
        manifests: &'m GameManifests,
        secrets: &'s SecretsApi<'_>,
        largs @ LaunchArgs { command, .. }: &LaunchArgs,
    ) -> Result<Self> {
        let Some(platform) = largs.resolve_platform() else {
            bail!(
                "failed to resolve platform we are running on, try specifying it explicitly with --platform"
            );
        };

        let platform = match platform {
            PlatformOpt::Steam => {
                let app_id = command
                    .iter()
                    .find(|e| e.starts_with("AppId="))
                    .map(|s| {
                        s.split_once("=")
                            .expect("invalid AppId field, has the steam arg format changed?")
                            .1
                            .parse::<u32>()
                            .map(SteamId::new)
                            .expect("failed to parse app id")
                    })
                    .expect("couldn't find steam id");

                let manifest_id = largs.app_id.unwrap_or(app_id);
                PlatformInfo::Steam {
                    app_id,
                    manifest_id,
                }
            }
            PlatformOpt::Umu => {
                todo!()
            }
            PlatformOpt::Auto => unreachable!(),
        };
        time! {
        "finding the game manifest":
        {
        let (game_name, game) = platform.find_game_in_manifest(manifests).ok_or_else(|| anyhow!("failed to find game in manifest"))?;
        }}

        debug!("found game manifest for {game_name}\n{game:#?}");

        let (bname, b) = cfg
            .backends
            .iter()
            .find(|b| b.name == cfg.default_backend)
            .map(|b| {
                b.to_backend(game_name, secrets)
                    .map(|bk| (b.name.clone(), bk))
            })
            .ok_or_else(|| anyhow!("no backends or default backend is invalid"))??;
        Ok(Self {
            platform,
            b,
            bname,
            game,
        })
    }

    fn mk_sync_mgr(&self) -> Result<SyncMgr> {
        let r = match &self.platform {
            PlatformInfo::Steam { app_id, .. } => {
                SyncMgr::from_steam_game(self.game, *app_id, &self.bname)
            }
            PlatformInfo::Umu {} => todo!(),
        };
        if let Err(e) = r.as_ref() {
            error!("failed to get information about game: {e}");
        }
        r
    }

    pub async fn sync_down(&self) -> Result<()> {
        let info = self.mk_sync_mgr()?;

        time! {
            "cloud sync down": {
            cloud_sync_down(&self.b, info).await?;
            }
        }
        Ok(())
    }

    pub async fn sync_up(&self) -> Result<()> {
        let info = self.mk_sync_mgr()?;

        time! {
            "cloud sync up": {
                info.upload(&self.b).await?;
            }
        }
        Ok(())
    }
}

async fn cloud_sync_down(b: &StorageBackend<'_>, info: SyncMgr<'_>) -> Result<()> {
    let Some(metadata) = b.read_sync_time().await? else {
        debug!("server has no metadata, we don't have to do anything");
        return Ok(());
    };
    if let Some(sync_info) = info.are_local_files_newer(&metadata).await? {
        warn!("found local files newer than local, showing confirmation box to the user...");

        match ui::spawn_sync_confirm(sync_info)? {
            SyncChoices::Download => {
                info.download(b, true, &metadata).await?;
            }
            SyncChoices::Continue => {}
            SyncChoices::Exit => {
                return Ok(());
            }
        }
    } else {
        info.download(b, false, &metadata).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_fs_sync() {
        //let dir = testdir::testdir!();
    }
}
