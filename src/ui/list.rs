use crate::ui::theme;
use egui::{Color32, Response, Ui, Vec2};

fn row_background(ui: &mut Ui, height: f32, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::click(),
    );
    let bg = if selected {
        theme::ROW_SELECTED
    } else if response.hovered() {
        theme::ROW_HOVER
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 0.0, bg);
    }
    response
}

/// Renders one wallpaper row. Returns true if clicked.
pub fn wallpaper_row(
    ui: &mut Ui,
    name: &str,
    subtitle: &str,
    selected: bool,
    scroll_to: bool,
) -> bool {
    let response = row_background(ui, theme::ROW_H, selected);
    if scroll_to {
        // Keep one row of context visible above the focused row by scrolling to a
        // rect that starts one row height higher than the row itself.
        let mut target = response.rect;
        target.min.y -= theme::ROW_H;
        ui.scroll_to_rect(target, Some(egui::Align::Min));
    }

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(response.rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.add_space(16.0);
    child.label(
        egui::RichText::new(egui_phosphor::regular::IMAGE)
            .size(theme::ICON_SIZE)
            .color(if selected { theme::ACCENT } else { theme::TEXT_MUTED }),
    );
    child.add_space(12.0);

    child.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.style_mut().spacing.item_spacing.y = 1.0;
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(name)
                .size(theme::FONT_TITLE)
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        if !subtitle.is_empty() {
            ui.label(
                egui::RichText::new(subtitle)
                    .size(theme::FONT_SUBTITLE)
                    .color(theme::TEXT_MUTED),
            );
        }
    });

    response.clicked()
}
