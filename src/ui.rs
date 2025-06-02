use chrono::{DateTime, Utc};
use popout::{Color32, LogicalSize, RichText};

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
