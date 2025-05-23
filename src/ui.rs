use anyhow::Result;
use egui::{Context, Ui, ViewportCommand, Window};

pub enum CincUi {
    Error(anyhow::Error),
    Panic(String),
}

impl eframe::App for CincUi {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| match self {
            CincUi::Error(err) => {
                ui.label("error encountered");
                ui.label(err.to_string());
                if ui.button("close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
            Self::Panic(msg) => {
                ui.label("panic!");
                ui.label(&*msg);
                if ui.button("close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        });
    }
}
