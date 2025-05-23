use egui::ViewportCommand;

pub struct SyncIssueInfo {}

pub enum CincUi<'s> {
    Error(anyhow::Error),
    Panic(String),
    SyncIssue {
        info: SyncIssueInfo,
        on_continue: Box<dyn FnOnce() + 's>,
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
            CincUi::SyncIssue { info, on_continue } => todo!(),
        });
    }
}
