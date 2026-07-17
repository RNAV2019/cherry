use std::collections::HashMap;
use std::path::PathBuf;

use egui::TextureHandle;

use crate::loader::Loader;
use crate::ui::theme;
use crate::wallpapers::{self, Wallpaper};

const LIST_WIDTH: f32 = 300.0;

pub struct CherryApp {
    query: String,
    selected_idx: Option<usize>,
    wallpapers: Vec<Wallpaper>,
    loader: Loader,
    textures: HashMap<PathBuf, TextureHandle>,
    dims: HashMap<PathBuf, (u32, u32)>,
    focus_search: bool,
    scroll_to_selected: bool,
}

impl CherryApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let wallpapers = wallpapers::scan_wallpapers(&wallpapers::default_dir());
        let loader = Loader::new();
        loader.request_all(
            wallpapers.iter().map(|w| w.path.clone()).collect(),
            cc.egui_ctx.clone(),
        );

        CherryApp {
            query: String::new(),
            selected_idx: None,
            wallpapers,
            loader,
            textures: HashMap::new(),
            dims: HashMap::new(),
            focus_search: true,
            scroll_to_selected: false,
        }
    }

    fn poll_images(&mut self, ctx: &egui::Context) {
        while let Some(loaded) = self.loader.poll() {
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied(loaded.size, &loaded.rgba);
            let handle = ctx.load_texture(
                loaded.path.to_string_lossy(),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            self.dims.insert(loaded.path.clone(), loaded.original);
            self.textures.insert(loaded.path, handle);
        }
    }

    fn filtered(&self) -> Vec<usize> {
        wallpapers::filtered(&self.wallpapers, &self.query)
    }

    /// The wallpaper shown in the preview pane: the selected row, or the first
    /// match when nothing is selected yet (what Enter would apply).
    fn preview_wall(&self) -> Option<&Wallpaper> {
        let filtered = self.filtered();
        let row = self.selected_idx.unwrap_or(0);
        filtered.get(row).map(|&i| &self.wallpapers[i])
    }

    fn move_selection(&mut self, delta: i32, count: usize) {
        if count == 0 {
            return;
        }
        self.scroll_to_selected = true;
        self.selected_idx = match self.selected_idx {
            None if delta > 0 => Some(0),
            None => None,
            Some(i) => {
                let next = i as i32 + delta;
                if next < 0 {
                    None
                } else {
                    Some((next as usize).min(count - 1))
                }
            }
        };
    }

    fn apply_and_close(&self, path: &std::path::Path, ctx: &egui::Context) {
        match crate::apply::apply(path) {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(err) => {
                eprintln!("cherry: {err}");
                crate::apply::notify(&err);
                std::process::exit(1);
            }
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let count = self.filtered().len();
        let mut apply_path: Option<PathBuf> = None;
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
                    match key {
                        egui::Key::Enter => {
                            apply_path = self.preview_wall().map(|w| w.path.clone());
                        }
                        egui::Key::ArrowDown | egui::Key::Tab => {
                            if modifiers.shift && *key == egui::Key::Tab {
                                self.move_selection(-1, count);
                            } else {
                                self.move_selection(1, count);
                            }
                        }
                        egui::Key::ArrowUp => self.move_selection(-1, count),
                        _ => {}
                    }
                }
            }
        });
        if let Some(path) = apply_path {
            self.apply_and_close(&path, ctx);
        }
    }

    fn render_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rows: Vec<(usize, bool)> = {
            let filtered = self.filtered();
            let preview_row = self.selected_idx.unwrap_or(0);
            filtered
                .iter()
                .enumerate()
                .map(|(row, &wall_idx)| (wall_idx, row == preview_row))
                .collect()
        };

        let mut clicked: Option<PathBuf> = None;
        let scroll_to_selected = std::mem::take(&mut self.scroll_to_selected);
        for &(wall_idx, selected) in &rows {
            let wall = &self.wallpapers[wall_idx];
            let subtitle = self
                .dims
                .get(&wall.path)
                .map(|(w, h)| format!("{w} × {h}"))
                .unwrap_or_default();
            let scroll = selected && scroll_to_selected;
            if crate::ui::list::wallpaper_row(ui, &wall.name, &subtitle, selected, scroll) {
                clicked = Some(wall.path.clone());
            }
        }
        if let Some(path) = clicked {
            self.apply_and_close(&path, ctx);
        }
    }
}

impl eframe::App for CherryApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_images(ctx);
        self.handle_keyboard(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let full_rect = ui.max_rect();
                let rounding = 16.0;
                if full_rect.width() <= 0.0 || full_rect.height() <= 0.0 {
                    return;
                }
                ui.painter().rect_filled(full_rect, rounding, theme::CARD_BG);
                ui.painter().rect_stroke(
                    full_rect.shrink(1.0),
                    rounding - 1.0,
                    egui::Stroke::new(1.0, theme::BORDER),
                );

                let builder = egui::UiBuilder::new()
                    .max_rect(full_rect.shrink(1.0))
                    .layout(egui::Layout::top_down(egui::Align::Min));
                ui.allocate_new_ui(builder, |ui| {
                    ui.style_mut().spacing.item_spacing = egui::Vec2::ZERO;
                    ui.style_mut().visuals.selection.bg_fill = theme::ACCENT;
                    ui.style_mut().visuals.widgets.noninteractive.bg_stroke.color =
                        theme::SEPARATOR;

                    let should_focus = self.focus_search || self.selected_idx.is_none();
                    self.focus_search = false;
                    let changed = crate::ui::search::search_bar(
                        ui,
                        &mut self.query,
                        "Search wallpapers...",
                        should_focus,
                    );
                    if changed {
                        self.selected_idx = None;
                    }

                    ui.add(egui::Separator::default().horizontal().spacing(0.0));

                    let hints_height = 40.0f32;
                    let body_height = (ui.available_height() - hints_height).max(0.0);
                    let body_top = ui.cursor().min.y;

                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(ui.available_width(), body_height),
                        egui::Layout::left_to_right(egui::Align::Min),
                        |ui| {
                            ui.allocate_ui(egui::Vec2::new(LIST_WIDTH, body_height), |ui| {
                                ui.set_min_size(egui::Vec2::new(LIST_WIDTH, body_height));
                                egui::ScrollArea::vertical()
                                    .max_height(body_height)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        self.render_list(ui, ctx);
                                    });
                            });

                            // Vertical separator between list and preview.
                            ui.painter().vline(
                                ui.cursor().min.x,
                                egui::Rangef::new(body_top, body_top + body_height),
                                egui::Stroke::new(1.0, theme::SEPARATOR),
                            );

                            let preview_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::Vec2::new(ui.available_width(), body_height),
                            );
                            let wall = self.preview_wall();
                            let texture = wall.and_then(|w| self.textures.get(&w.path));
                            let name = wall.map(|w| w.name.as_str());
                            let original = wall.and_then(|w| self.dims.get(&w.path)).copied();
                            crate::ui::preview::preview_pane(
                                ui,
                                preview_rect,
                                texture,
                                name,
                                original,
                            );
                        },
                    );

                    crate::ui::hints::hints_bar(ui);
                });
            });
    }
}
