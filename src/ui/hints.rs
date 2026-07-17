use crate::ui::theme;
use egui::Ui;

pub fn hints_bar(ui: &mut Ui) {
    ui.add(egui::Separator::default().horizontal().spacing(0.0));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        kbd_icon(ui, egui_phosphor::regular::ARROW_UP);
        kbd_icon(ui, egui_phosphor::regular::ARROW_DOWN);
        hint(ui, "Navigate");
        ui.add_space(16.0);
        kbd_icon(ui, egui_phosphor::regular::KEY_RETURN);
        hint(ui, "Apply");
        ui.add_space(16.0);
        kbd(ui, "Esc");
        hint(ui, "Close");
    });
    ui.add_space(6.0);
}

/// Keyboard badge with text label.
fn kbd(ui: &mut Ui, label: &str) {
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(label.to_string(), egui::FontId::proportional(11.0), theme::KBD_TEXT)
    });
    let padding = egui::Vec2::new(6.0, 3.0);
    let desired = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, theme::KBD_BG);
    ui.painter().galley(rect.min + padding, galley, theme::KBD_TEXT);
    ui.add_space(4.0);
}

/// Keyboard badge with a Phosphor icon.
fn kbd_icon(ui: &mut Ui, icon: &str) {
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(icon.to_string(), egui::FontId::proportional(13.0), theme::KBD_TEXT)
    });
    let padding = egui::Vec2::new(5.0, 2.0);
    let desired = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, theme::KBD_BG);
    ui.painter().galley(rect.min + padding, galley, theme::KBD_TEXT);
    ui.add_space(4.0);
}

fn hint(ui: &mut Ui, label: &str) {
    ui.label(egui::RichText::new(label).size(11.0).color(theme::TEXT_MUTED));
}
