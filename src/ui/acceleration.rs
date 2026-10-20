use super::*;
use crate::acceleration::{Encoder, ThreeD};
impl FrameApp {
    pub(super) fn acceleration_controls(&mut self, ui: &mut egui::Ui, id: &str) {
        ui.push_id(id, |ui| {
            ui.label(RichText::new("Hardware acceleration").strong());
            let running = self.exports.as_ref().is_some_and(|(_, h)| !h.is_finished());
            let mut prefs = self.acceleration;
            ui.add_enabled_ui(!running, |ui| {
                egui::ComboBox::from_id_salt("encoder").selected_text(prefs.encoder.label()).show_ui(ui, |ui| {
                    for e in [Encoder::Auto, Encoder::Nvidia, Encoder::Cpu] { ui.selectable_value(&mut prefs.encoder, e, e.label()); }
                });
                egui::ComboBox::from_id_salt("3d_backend").selected_text(match prefs.scene3d { ThreeD::Auto => "3D · Automatic GPU", ThreeD::Cpu => "3D · CPU" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut prefs.scene3d, ThreeD::Auto, "3D · Automatic GPU (prefer NVIDIA)");
                    ui.selectable_value(&mut prefs.scene3d, ThreeD::Cpu, "3D · CPU");
                });
            });
            if prefs != self.acceleration {
                match prefs.save(&self.storage.root) {
                    Ok(()) => { self.acceleration = prefs; prefs.apply(); self.preview_dirty = Some(Instant::now()); self.acceleration_result.clear(); }
                    Err(e) => self.notify(format!("Could not save acceleration settings: {e}")),
                }
            }
            ui.label(RichText::new(crate::gpu3d::status()).small().color(palette().muted));
            ui.label(RichText::new("NVENC accelerates final video encoding. 3D uses DX12/Vulkan. Compositing, masks, vector drawing and temporal effects still use the CPU.").small().color(palette().muted));
            let checking = self.acceleration_task.as_ref().is_some_and(|(_, h)| !h.is_finished());
            if ui.add_enabled(!checking && !running, egui::Button::new("Check GPU & encoder")).clicked() {
                if let Some((_, handle)) = self.acceleration_task.take() { let _ = handle.join(); }
                let group = ProcessGroup::default();
                let token = group.clone();
                let tx = self.send.clone();
                self.acceleration_result = "Checking GPU and encoding two test frames…".into();
                let handle = thread::spawn(move || {
                    let result = crate::acceleration::diagnostics(&token).unwrap_or_else(|e| e.to_string());
                    let _ = tx.send(Event::Acceleration(result));
                });
                self.acceleration_task = Some((group, handle));
            }
            if !self.acceleration_result.is_empty() {
                egui::ScrollArea::vertical().id_salt("acceleration_result").max_height(120.).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&self.acceleration_result).small()).wrap());
                });
            }
        });
    }
}
