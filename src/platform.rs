use std::{
    env,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    args::{LaunchArgs, PlatformOpt},
    backends::StorageBackend,
    config::{Config, SteamId},
    manifest::{self, GameManifest, GameManifests},
    secrets::SecretsApi,
    sync::SyncMgr,
    time,
    ui::{self, SyncChoices},
};
use anyhow::Result;
use anyhow::{anyhow, bail};
use itertools::Itertools;
use tracing::{debug, error, warn};

pub enum PlatformInfo {
    Steam { app_id: SteamId },
    Umu { exe_path: PathBuf },
}
impl PlatformInfo {
    fn find_game_in_manifest<'a>(
        &self,
        manifests: &'a GameManifests,
    ) -> Option<(&'a str, &'a GameManifest)> {
        match self {
            PlatformInfo::Steam { app_id } => find_in_manifest_by_steam_id(manifests, *app_id),
            PlatformInfo::Umu { exe_path } => {
                let r = find_game_from_env_vars(manifests);
                let reason = if let Err(e) = r.as_ref() {
                    e.to_string()
                } else {
                    "no match".to_owned()
                };
                match r.ok().flatten() {
                    Some(v) => Some(v),
                    None => {
                        debug!(
                            "failed to discover game from env vars (reason: {reason}), falling back to executable name"
                        );
                        find_likelist_umu_match(manifests, exe_path)
                    }
                }
            }
        }
    }
}
fn find_in_manifest_by_steam_id(
    manifest: &GameManifests,
    app_id: SteamId,
) -> Option<(&str, &GameManifest)> {
    manifest
        .iter()
        .find(|(_, m)| m.steam.as_ref().map(|i| i.id == app_id).unwrap_or(false))
        .map(|(s, g)| (s.as_str(), g))
}

fn find_likelist_umu_match<'a>(
    manifest: &'a GameManifests,
    exe_path: &Path,
) -> Option<(&'a str, &'a GameManifest)> {
    let platform = manifest::PlatformInfo {
        store: None,
        wine: true,
    };
    let exe_comps = exe_path.components().rev().collect_vec();
    let mut max_len = 0;
    let mut max = None;
    for (k, m) in manifest {
        for (p, _) in m
            .launch
            .iter()
            .filter(|l| l.1.iter().all(|p| p.sat(platform)))
        {
            let len = p
                .as_raw_path()
                .components()
                .rev()
                .zip(exe_comps.iter())
                .take_while(|(a, b)| a == *b)
                .count();
            if max_len < len {
                max = Some((k.as_str(), m));
                max_len = len;
            }
        }
    }
    max
}
/// Set to the store the game came from, gog, epic, amazon
pub const HEROIC_APP_SOURCE: &str = "HEROIC_APP_SOURCE";
/// Set to the app name for that store. For gog this seems to be the app id
pub const HEROIC_APP_NAME: &str = "HEROIC_APP_NAME";

/// Try and find the game match based on environment variables set by some launchers (e.g. heroic)
fn find_game_from_env_vars(manifest: &GameManifests) -> Result<Option<(&str, &GameManifest)>> {
    // Heroic sets 2 environment variables that are of interest to us (https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/blob/a7feb36ad98c72be8fc58cd2976276a03910f9ee/src/backend/launcher.ts#L840)
    let source = env::var(HEROIC_APP_SOURCE)?;
    let name = env::var(HEROIC_APP_NAME)?;

    if source == "gog" {
        debug!("found gog source, attempting to match on id {name}");
        let gog_id: u32 = name.parse()?;

        return Ok(manifest
            .iter()
            .find(|(_, m)| m.gog.as_ref().map(|g| g.id == gog_id).unwrap_or(false))
            .map(|(s, m)| (s.as_str(), m)));
    }
    Ok(None)
}

pub struct LaunchInfo<'s, 'm> {
    platform: PlatformInfo,
    b: StorageBackend<'s>,
    bname: String,
    game: &'m GameManifest,
    game_name: &'m str,
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
        let manifest_steam_id = largs.app_id;

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

                PlatformInfo::Steam { app_id }
            }
            PlatformOpt::Umu => {
                let exe_path = command
                    .get(1)
                    .ok_or_else(|| anyhow!("expected a command to invoke for umu"))?
                    .to_owned();
                PlatformInfo::Umu {
                    exe_path: exe_path.into(),
                }
            }
            PlatformOpt::Auto => unreachable!(),
        };
        time! {
        "finding the game manifest":
        {
        let (game_name, game) = manifest_steam_id.and_then(|id|{
            debug!("using supplied steam id to find game in the manifest");
            find_in_manifest_by_steam_id(manifests, id)
        }).or_else(||  platform.find_game_in_manifest(manifests)).ok_or_else(|| anyhow!("failed to find game in manifest"))?;
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
            game_name,
        })
    }

    fn mk_sync_mgr(&self) -> Result<SyncMgr> {
        let r = match &self.platform {
            PlatformInfo::Steam { app_id, .. } => {
                SyncMgr::from_steam_game(self.game_name, self.game, *app_id, &self.bname)
            }
            PlatformInfo::Umu { .. } => {
                SyncMgr::from_umu_env(self.game_name, self.game, &self.bname)
            }
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
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
    };

    use crate::{
        args::{LaunchArgs, PlatformOpt},
        config::{BackendInfo, BackendTy, Config, SteamId},
        manifest::{
            FileConfig, FileTag, GameManifest, GameManifests, GogInfo, SteamInfo, TemplatePath,
        },
        paths::PathExt,
        platform::{HEROIC_APP_NAME, HEROIC_APP_SOURCE, find_game_from_env_vars},
        secrets::SecretsApi,
        sync::ARCHIVE_NAME,
    };
    use temp_env::async_with_vars;
    use test_log::test;

    use super::LaunchInfo;

    async fn run_sync_test(root: &Path, file_path: &Path, largs: &LaunchArgs, game: GameManifest) {
        let contents = "hello-world";
        std::fs::write(file_path, contents).unwrap();

        let manifest = mk_manifest(game);
        let local_path = root.join("store");
        let archive_p = local_path.join("test").join(ARCHIVE_NAME);

        let cfg = test_cfg(local_path);
        let secrets = SecretsApi::new_unavailable();
        let launch = LaunchInfo::new(&cfg, &manifest, &secrets, largs).unwrap();

        launch.sync_down().await.unwrap();
        assert!(!std::fs::exists(&archive_p).unwrap());
        launch.sync_up().await.unwrap();
        assert!(
            std::fs::exists(&archive_p).unwrap(),
            "didn't write archive to {archive_p:?}"
        );
        std::fs::remove_file(file_path).unwrap();
        launch.sync_down().await.unwrap();
        assert!(
            !std::fs::exists(file_path).unwrap(),
            "sync downloaded even though it didn't have to"
        );

        let info = launch.mk_sync_mgr().unwrap();
        let metadata = launch.b.read_sync_time().await.unwrap().unwrap();
        assert!(
            info.are_local_files_newer(&metadata)
                .await
                .unwrap()
                .is_none()
        );
        info.download(&launch.b, false, &metadata).await.unwrap();
    }

    #[test(tokio::test)]
    async fn local_fs_sync() {
        let root = testdir::testdir!("local_fs_sync");
        let file_path = root.join("file");
        let launch_exe = "run.exe";
        let wine_prefix = root.join("wineprefix");
        std::fs::create_dir_all(&wine_prefix).unwrap();
        async_with_vars(
            [("WINEPREFIX", Some(wine_prefix.to_str().unwrap()))],
            async {
                run_sync_test(
                    &root,
                    &file_path,
                    &LaunchArgs {
                        platform: PlatformOpt::Auto,
                        no_upload: false,
                        app_id: None,
                        command: vec!["/usr/bin/umu-run".to_owned(), launch_exe.to_owned()],
                    },
                    GameManifest {
                        steam: None,
                        gog: None,
                        install_dir: None,
                        files: [(
                            TemplatePath::new(
                                Path::new("<base>").join_good(&file_path).to_str().unwrap(),
                            ),
                            FileConfig {
                                preds: vec![],
                                tags: vec![FileTag::Save],
                            },
                        )]
                        .into_iter()
                        .collect(),
                        launch: [(TemplatePath::new(launch_exe), vec![])]
                            .into_iter()
                            .collect(),
                    },
                )
                .await;
            },
        )
        .await;
    }
    fn test_cfg(root: PathBuf) -> Config {
        Config {
            default_backend: "t".to_owned(),
            manifest_url: None,
            backends: vec![BackendInfo {
                name: "t".to_owned(),
                info: BackendTy::Filesystem { root },
            }],
        }
    }
    fn mk_manifest(game: GameManifest) -> GameManifests {
        let mut manifest = HashMap::new();
        manifest.insert("test".to_owned(), game);
        manifest
    }

    #[test(tokio::test)]
    async fn discovery_via_forced_steam_id_with_heroic() {
        let root = testdir::testdir!("discovery_via_forced_steam_id_with_heroic");
        let launch_exe = "run.exe";
        let id = SteamId::new(0);

        let game =
            // it wouldn't normally find this
            GameManifest {
                steam: Some(SteamInfo { id }),
                ..Default::default()
            };
        let largs = &LaunchArgs {
            platform: PlatformOpt::Auto,
            no_upload: false,
            app_id: Some(id),
            command: vec!["/usr/bin/umu-run".to_owned(), launch_exe.to_owned()],
        };
        let manifest = mk_manifest(game);
        let cfg = test_cfg(root);
        let secrets = SecretsApi::new_unavailable();
        LaunchInfo::new(&cfg, &manifest, &secrets, largs).unwrap();
    }
    #[test]
    fn find_game_from_vars_heroic() {
        let id = 1;
        let mut manifest = HashMap::new();
        unsafe {
            std::env::set_var(HEROIC_APP_SOURCE, "gog");
            std::env::set_var(HEROIC_APP_NAME, "1");
        }
        manifest.insert(
            "test".to_owned(),
            GameManifest {
                steam: None,
                gog: Some(GogInfo { id }),
                files: Default::default(),
                launch: Default::default(),
                install_dir: None,
            },
        );
        assert!(find_game_from_env_vars(&manifest).unwrap().is_some());
    }
}
