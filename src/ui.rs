use chrono::{DateTime, Utc};
use egui::{Color32, RichText, ViewportCommand};

pub struct SyncIssueInfo {
    pub local_time: DateTime<Utc>,
    pub remote_time: DateTime<Utc>,
    pub remote_name: String,
    pub remote_last_writer: String,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncChoices {
    /// User chose to continue (download changes)
    Continue,
    /// User chose to upload local changes to remote
    Upload,
    /// User chose to abort completely
    Exit,
}

pub enum CincUi<'s> {
    Error(anyhow::Error),
    Panic(String),
    SyncIssue {
        info: SyncIssueInfo,
        on_continue: Box<dyn FnMut(&mut SyncChoices)>,
        on_upload: Box<dyn FnMut(&mut SyncChoices)>,
        choice_store: &'s mut SyncChoices,
    },
}

impl<'s> eframe::App for CincUi<'s> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| match self {
            CincUi::Error(err) => {
                ui.label("error encountered");
                ui.label(err.to_string());
                if ui.button("close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
            CincUi::Panic(msg) => {
                ui.label("panic!");
                ui.label(&*msg);
                if ui.button("close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
            CincUi::SyncIssue {
                info,
                on_continue,
                on_upload,
                choice_store,
            } => {
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
If you continue, your local changes will be overwritten with the remote changes.

You may also chose to upload your local changes now, overwriting the remote changes.
                ",
                    );
                    ui.label(
                        RichText::new("UPLOAD OR CONTINUE MAY RESULT IN DATA LOSS")
                            .color(Color32::RED)
                            .strong()
                            .size(18.0),
                    )
                });

                ui.horizontal(|ui| {
                    if ui.button("Upload").clicked() {
                        on_upload(choice_store);
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    if ui.button("Continue").clicked() {
                        on_continue(choice_store);
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
            }
        });
    }
}
