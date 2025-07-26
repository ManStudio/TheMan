use eframe::egui;

pub trait Window: Send + Sync {
    fn builder(&self) -> egui::ViewportBuilder;
    fn show(&mut self, ctx: &egui::Context);
    fn should_close(&self) -> bool;
}
