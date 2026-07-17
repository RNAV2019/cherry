use crate::ui::theme;
use egui::{Rect, TextureHandle, Ui, Vec2};

/// Renders the preview pane: the highlighted wallpaper aspect-fit inside a
/// rounded panel, with a filename + resolution caption underneath.
pub fn preview_pane(
    ui: &mut Ui,
    rect: Rect,
    texture: Option<&TextureHandle>,
    name: Option<&str>,
    original: Option<(u32, u32)>,
) {
    let padding = 16.0;
    let caption_h = 44.0;
    let panel = rect.shrink(padding);
    let image_area = Rect::from_min_max(
        panel.min,
        egui::pos2(panel.max.x, (panel.max.y - caption_h).max(panel.min.y)),
    );

    ui.painter().rect_filled(image_area, 12.0, theme::BG);

    match texture {
        Some(tex) => {
            let tex_size = tex.size_vec2();
            let avail = image_area.size() - Vec2::splat(2.0 * 8.0);
            let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y);
            let size = tex_size * scale;
            let image_rect = Rect::from_center_size(image_area.center(), size);
            ui.painter().image(
                tex.id(),
                image_rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            let (icon, label) = if name.is_some() {
                (egui_phosphor::regular::HOURGLASS, "Loading...")
            } else {
                (egui_phosphor::regular::IMAGE, "No wallpapers found")
            };
            ui.painter().text(
                image_area.center() - Vec2::new(0.0, 14.0),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::FontId::proportional(32.0),
                theme::TEXT_MUTED,
            );
            ui.painter().text(
                image_area.center() + Vec2::new(0.0, 14.0),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(theme::FONT_SUBTITLE),
                theme::TEXT_MUTED,
            );
        }
    }

    if let Some(name) = name {
        let caption_center_y = image_area.max.y + caption_h / 2.0;
        ui.painter().text(
            egui::pos2(panel.center().x, caption_center_y - 8.0),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(theme::FONT_TITLE),
            theme::TEXT_PRIMARY,
        );
        if let Some((w, h)) = original {
            ui.painter().text(
                egui::pos2(panel.center().x, caption_center_y + 10.0),
                egui::Align2::CENTER_CENTER,
                format!("{w} × {h}"),
                egui::FontId::proportional(theme::FONT_SUBTITLE),
                theme::TEXT_MUTED,
            );
        }
    }
}
