use crate::ui::theme;
use egui::{TextEdit, Ui};

pub const SEARCH_ID: &str = "cherry_search";

/// Renders the search bar. Returns true if the query changed.
/// `focused`: when true the bar holds keyboard focus; when false it surrenders it so
/// arrow-key navigation is not intercepted by the TextEdit.
pub fn search_bar(ui: &mut Ui, query: &mut String, placeholder: &str, focused: bool) -> bool {
    let mut changed = false;

    let desired_height = 48.0;
    ui.allocate_ui_with_layout(
        egui::Vec2::new(ui.available_width(), desired_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                    .size(18.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);
            let response = ui.add(
                TextEdit::singleline(query)
                    .id(egui::Id::new(SEARCH_ID))
                    .hint_text(egui::RichText::new(placeholder).color(theme::TEXT_MUTED))
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .font(egui::FontId::proportional(theme::FONT_TITLE))
                    .text_color(theme::TEXT_PRIMARY),
            );
            if focused {
                response.request_focus();
            } else {
                response.surrender_focus();
            }
            changed = response.changed();
        },
    );

    changed
}
