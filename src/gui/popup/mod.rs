use eframe::egui;

use crate::Account;

use super::Context;

mod add_node;
mod set_name;

pub use add_node::PopupAddNode;
pub use set_name::PopupSetName;

pub trait Popup {
    fn show(&mut self, ui: &mut egui::Ui, context: &mut Context, account: &mut Account) -> bool;
    fn name(&self) -> String;
}
