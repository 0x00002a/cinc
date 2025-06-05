use chrono::{DateTime, Utc};
use popout::{Color32, LogicalSize, RichText, WindowAttributes, egui::TextStyle};

use crate::{curr_crate_ver, platform::IncomaptibleCincVersionError};

pub struct SyncIssueInfo {
    pub local_time: DateTime<Utc>,
    pub remote_time: DateTime<Utc>,
    pub remote_name: String,
    pub remote_last_writer: String,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncChoices {
    /// User chose to continue (download changes)
    Download = 0,
    /// User chose to upload local changes to remote
    Continue = 1,
    /// User chose to abort completely
    Exit = 2,
}

/// Spawn a dialog warning the user of sync issues and asking them whether to
/// continue. Returns whether the user elected to continue
pub fn spawn_sync_confirm(info: SyncIssueInfo) -> anyhow::Result<SyncChoices> {
    let min_sz = popout::PhysicalSize::new(500.0, 200.0);
    let r = popout::create_window(
        |ui| {
            let local_time = info
                .local_time
                .with_timezone(&chrono::Local)
                .format("%c")
                .to_string();
            let remote_time = info
                .remote_time
                .with_timezone(&chrono::Local)
                .format("%c")
                .to_string();
            ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Cloud conflict detected")
                            .size(20.0)
                            .heading()
                            .color(Color32::YELLOW),
                    );
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Local changes are from");
                        ui.label(RichText::new(local_time).color(Color32::CYAN));
                    });

                    ui.horizontal(|ui| {
                        let remote_name = &info.remote_name;
                        ui.label("Remote changes are from");
                        ui.label(RichText::new(remote_time).color(Color32::CYAN));
                        ui.label(RichText::new(format!("({remote_name})")));
                    });

                    ui.label(
                        r"
If you continue, your local changes will be overwrite the remote changes when you close the game.
If you download the remote changes your local files will be overwritten with the remote changes, if
you have made any progress since the time displayed above for the remote changes, THIS WILL ERASE IT!!
                ".replace('\n', " "),
                    );
                    ui.label(
                        RichText::new("CONTINUE OR DOWNLOAD MAY RESULT IN DATA LOSS")
                            .color(Color32::RED)
                            .strong()
                            .size(18.0),
                    )
                });

            ui.horizontal(|ui| {
                if ui.button("Continue").clicked() {
                    return Some(SyncChoices::Continue);
                }
                if ui.button("Download").clicked() {
                    return Some(SyncChoices::Download);
                }
                if ui.button("Exit").clicked() {
                    return Some(SyncChoices::Exit);
                }
                None
            })
            .inner
        },
        popout::WindowAttributes::default()
            .with_title("Cloud conflict")
            .with_inner_size(popout::LogicalSize::new(min_sz.width, min_sz.height))
            .with_min_inner_size(min_sz),
    )?;
    Ok(r.unwrap_or(SyncChoices::Exit))
}

pub fn version_mismatch(err: &IncomaptibleCincVersionError) -> anyhow::Result<()> {
    let title = "Incompatible cinc version detected";
    popout::create_window(
        |ui| {
            ui.label(RichText::new(title).heading().color(Color32::YELLOW));
            ui.separator();
            ui.label(RichText::new("To avoid data loss cinc will not continue").strong());
            let msg = format!(
                "The version of cinc used to write the files on the server ({}) is incompatible with the current version ({}).",
                err.server_version,
                curr_crate_ver()
            );
            ui.label(RichText::new(msg).text_style(TextStyle::Body));

            if err.read {
                ui.label("You can solve this by specifying --upload-only after the launch argument. This will overwrite the server version with your local files");
                ui.label(
                    RichText::new(
                        "PLEASE ENSURE YOUR LOCAL FILES ARE THE LATEST VERSION BEFORE DOING THIS",
                    )
                    .color(Color32::RED)
                    .strong(),
                );
            } else {
                ui.label("You can solve this by upgrading your version of cinc to match the version on the server");
            }

            if ui.button("Close").clicked() {
                Some(())
            } else {
                None
            }
        },
        WindowAttributes::default()
            .with_title("Incompatible cinc version detected")
            .with_inner_size(LogicalSize::new(500.0, 200.0)),
    )?;
    Ok(())
}

pub fn show_no_download_confirmation() -> anyhow::Result<bool> {
    let mut txt_entry = String::new();
    let title = "Potentially destructive action";
    let confirmation = "trust me";
    let mut mismatch = false;
    let r = popout::create_window(
        |ui| {
            if mismatch && !txt_entry.is_empty() {
                mismatch = false;
            }
            ui.label(RichText::new(title).heading().color(Color32::YELLOW));
            ui.label("You have passed --upload-only. This may result in data loss");
            ui.label(
                RichText::new(
                    r#"
if you have made progress on another computer and not successfully run the game at least once on
this one you will LOSE YOUR PROGRESS FROM THE OTHER COMPUTER
        "#
                    .replace('\n', ""),
                )
                .color(Color32::RED)
                .strong(),
            );
            ui.label(format!(
                "To ensure you mean to continue please enter '{confirmation}' in the text box below"
            ));
            ui.text_edit_singleline(&mut txt_entry);
            if mismatch {
                ui.label(RichText::new("That doesn't match").color(Color32::YELLOW));
            }
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    return Some(false);
                }
                if ui.button("Confirm").clicked() {
                    if txt_entry != confirmation {
                        mismatch = true;
                        txt_entry.clear();
                    } else {
                        return Some(true);
                    }
                }
                None
            })
            .inner
        },
        WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(500.0, 200.0)),
    )?;
    Ok(r == Some(true))
}

pub fn show_error_dialog(err: &impl std::fmt::Debug) -> anyhow::Result<()> {
    popout::dialog::Dialog::new()
        .with_line(
            RichText::new("error encountered")
                .heading()
                .color(Color32::RED),
        )
        .with_line(format!("{err:?}"))
        .with_button("Exit")
        .with_title("Error")
        .with_size(LogicalSize::new(300, 100))
        .show()?;
    Ok(())
}

pub fn show_panic_dialog(
    msg: impl Into<String>,
    loc: Option<&std::panic::Location>,
) -> anyhow::Result<()> {
    let mut dialog = popout::dialog::Dialog::new()
        .with_line(RichText::new("panic!").heading().color(Color32::RED))
        .with_line(msg.into())
        .with_button("Exit")
        .with_title("Panic")
        .with_size(LogicalSize::new(200, 100));

    if let Some(loc) = loc {
        dialog = dialog.with_line(format!("at {}:{}:{}", loc.file(), loc.line(), loc.column()));
    }
    dialog.show()?;
    Ok(())
}
