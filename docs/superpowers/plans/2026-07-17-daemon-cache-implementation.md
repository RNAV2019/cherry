# Cherry Daemon Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert cherry from a fresh-process-per-launch `eframe` app into a resident daemon (Unix socket + `winit`/`wgpu`), so decoded wallpaper textures, filenames, and dimensions persist in memory across toggles instead of being rebuilt on every launch.

**Architecture:** Port the daemon/socket/GPU-lifecycle plumbing from the sibling project `project-picker` (`~/Projects/project-picker/src/daemon.rs`, `~/Projects/project-picker/src/main.rs`) verbatim where possible, dropping mycelium/project-picker's fade animation (cherry has none today). `CherryApp` becomes a plain struct owned by the daemon's persistent `State` instead of an `eframe::App`; a rescan-and-diff step on every show keeps the wallpaper list correct without re-decoding unchanged files. The existing list+preview UI (`src/ui/*.rs`) is untouched.

**Tech Stack:** Rust, `winit` 0.30, `wgpu` 22, `egui`/`egui-wgpu`/`egui-winit` 0.29, `pollster`, `image` (unchanged), replacing `eframe`.

**Reference spec:** `docs/superpowers/specs/2026-07-17-daemon-cache-design.md`

---

### Task 1: Swap `eframe` for the winit/wgpu daemon stack

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Replace the `[dependencies]` block**

Current `Cargo.toml` has:

```toml
[dependencies]
egui = "0.29"
eframe = { version = "0.29", default-features = false, features = ["wgpu", "wayland", "default_fonts"] }
egui-phosphor = { version = "0.7", features = ["regular"] }
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
```

Replace it with:

```toml
[dependencies]
egui = "0.29"
egui-wgpu = "0.29"
wgpu = { version = "22", features = [] }
egui-winit = { version = "0.29", default-features = false, features = ["wayland"] }
winit = { version = "0.30", default-features = false, features = ["wayland", "rwh_06"] }
pollster = "0.3"
egui-phosphor = { version = "0.7", features = ["regular"] }
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
```

Leave `[package]`, `[[bin]]`, and `[dev-dependencies]` (`tempfile = "3"`) unchanged.

- [ ] **Step 2: Regenerate the lockfile**

Run: `cargo generate-lockfile --manifest-path /home/ryan/Projects/cherry/Cargo.toml`
Expected: completes without error (it will fail to fully resolve until Task 5 removes the `eframe`-only code paths that no longer compile — that's expected at this point; this step just seeds `Cargo.lock` with the new dependency graph). If `cargo generate-lockfile` errors because it also tries to typecheck, skip this step and let Task 5's build step regenerate the lockfile instead.

- [ ] **Step 3: Commit**

```bash
cd /home/ryan/Projects/cherry
git add Cargo.toml Cargo.lock
git commit -m "build: swap eframe for winit/wgpu daemon stack"
```

---

### Task 2: Add a pure diff function for rescan-without-redecode

**Files:**
- Modify: `src/wallpapers.rs`
- Test: `src/wallpapers.rs` (inline `#[cfg(test)]` module, same file)

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/wallpapers.rs` (after the existing `filtered_returns_matching_indices` test):

```rust
    #[test]
    fn diff_scan_detects_added_and_removed() {
        let old = vec![
            Wallpaper { path: PathBuf::from("a.png"), name: "a.png".into() },
            Wallpaper { path: PathBuf::from("b.png"), name: "b.png".into() },
        ];
        let fresh = vec![
            Wallpaper { path: PathBuf::from("b.png"), name: "b.png".into() },
            Wallpaper { path: PathBuf::from("c.png"), name: "c.png".into() },
        ];
        let (added, removed) = diff_scan(&old, &fresh);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].path, PathBuf::from("c.png"));
        assert_eq!(removed, vec![PathBuf::from("a.png")]);
    }

    #[test]
    fn diff_scan_no_changes_is_empty() {
        let list = vec![Wallpaper { path: PathBuf::from("a.png"), name: "a.png".into() }];
        let (added, removed) = diff_scan(&list, &list);
        assert!(added.is_empty());
        assert!(removed.is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path /home/ryan/Projects/cherry/Cargo.toml diff_scan`
Expected: FAIL with `cannot find function 'diff_scan' in this scope`

- [ ] **Step 3: Implement `diff_scan`**

Add this function to `src/wallpapers.rs`, directly after `pub fn filtered(...)` (before the `// ── Tests ──` divider):

```rust
/// Diffs `old` against `fresh` by path, without assuming either is sorted.
/// Returns `(added, removed)`: wallpapers present in `fresh` but not `old`,
/// and the paths of wallpapers present in `old` but not `fresh`. Used to
/// rescan a directory without re-decoding images that haven't changed.
pub fn diff_scan(old: &[Wallpaper], fresh: &[Wallpaper]) -> (Vec<Wallpaper>, Vec<PathBuf>) {
    use std::collections::HashSet;

    let old_paths: HashSet<&PathBuf> = old.iter().map(|w| &w.path).collect();
    let fresh_paths: HashSet<&PathBuf> = fresh.iter().map(|w| &w.path).collect();

    let added = fresh
        .iter()
        .filter(|w| !old_paths.contains(&w.path))
        .cloned()
        .collect();
    let removed = old
        .iter()
        .filter(|w| !fresh_paths.contains(&w.path))
        .map(|w| w.path.clone())
        .collect();

    (added, removed)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path /home/ryan/Projects/cherry/Cargo.toml diff_scan`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
cd /home/ryan/Projects/cherry
git add src/wallpapers.rs
git commit -m "feat: add diff_scan to detect added/removed wallpapers without redecoding"
```

---

### Task 3: Rewrite `app.rs` as a daemon-owned struct with action draining

**Files:**
- Modify: `src/app.rs` (full rewrite)

- [ ] **Step 1: Replace the entire contents of `src/app.rs`**

```rust
use std::collections::HashMap;
use std::path::PathBuf;

use egui::TextureHandle;

use crate::loader::Loader;
use crate::ui::theme;
use crate::wallpapers::{self, Wallpaper};

const LIST_WIDTH: f32 = 380.0;

/// Actions produced during a frame that require daemon-level responses
/// (the daemon owns the window/process lifecycle, not `CherryApp`).
#[derive(Debug)]
pub enum AppAction {
    Hide,
    Apply(PathBuf),
}

pub struct CherryApp {
    query: String,
    selected_idx: Option<usize>,
    wallpapers: Vec<Wallpaper>,
    loader: Loader,
    textures: HashMap<PathBuf, TextureHandle>,
    dims: HashMap<PathBuf, (u32, u32)>,
    focus_search: bool,
    scroll_to_selected: bool,
    pending_actions: Vec<AppAction>,
}

impl CherryApp {
    /// Scans the wallpaper directory but does not start decoding — the
    /// daemon calls `request_initial_load` once it has a live `egui::Context`.
    pub fn new() -> Self {
        let wallpapers = wallpapers::scan_wallpapers(&wallpapers::default_dir());
        CherryApp {
            query: String::new(),
            selected_idx: None,
            wallpapers,
            loader: Loader::new(),
            textures: HashMap::new(),
            dims: HashMap::new(),
            focus_search: true,
            scroll_to_selected: false,
            pending_actions: Vec::new(),
        }
    }

    /// Kicks off background decoding for every wallpaper found by `new()`.
    /// Called once, right after daemon startup, so the cache is warm before
    /// the first toggle.
    pub fn request_initial_load(&self, ctx: egui::Context) {
        let paths = self.wallpapers.iter().map(|w| w.path.clone()).collect();
        self.loader.request_all(paths, ctx);
    }

    /// Called on every toggle-open: rescans the wallpaper directory,
    /// decodes only newly-added files, evicts removed ones from the
    /// texture/dimension caches, and resets search/selection state.
    pub fn on_show(&mut self, ctx: &egui::Context) {
        self.rescan(ctx);
        self.query.clear();
        self.selected_idx = None;
        self.focus_search = true;
    }

    /// No-op placeholder for symmetry with `on_show` — cherry has no
    /// hide-time state to reset (no animation).
    pub fn on_hide(&mut self) {}

    fn rescan(&mut self, ctx: &egui::Context) {
        let fresh = wallpapers::scan_wallpapers(&wallpapers::default_dir());
        let (added, removed) = wallpapers::diff_scan(&self.wallpapers, &fresh);

        for path in &removed {
            self.textures.remove(path);
            self.dims.remove(path);
        }

        if !added.is_empty() {
            let paths = added.iter().map(|w| w.path.clone()).collect();
            self.loader.request_all(paths, ctx.clone());
        }

        self.wallpapers = fresh;
    }

    pub fn drain_actions(&mut self) -> Vec<AppAction> {
        std::mem::take(&mut self.pending_actions)
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

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.pending_actions.push(AppAction::Hide);
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
            self.pending_actions.push(AppAction::Apply(path));
        }
    }

    fn render_list(&mut self, ui: &mut egui::Ui) {
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
            self.pending_actions.push(AppAction::Apply(path));
        }
    }

    /// Runs one frame. Called by the daemon inside `egui::Context::run`.
    pub fn ui(&mut self, ctx: &egui::Context) {
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
                            ui.allocate_ui_with_layout(
                                egui::Vec2::new(LIST_WIDTH, body_height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_min_size(egui::Vec2::new(LIST_WIDTH, body_height));
                                    egui::ScrollArea::vertical()
                                        .max_height(body_height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            self.render_list(ui);
                                        });
                                },
                            );

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
```

- [ ] **Step 2: Commit**

```bash
cd /home/ryan/Projects/cherry
git add src/app.rs
git commit -m "refactor: make CherryApp a daemon-owned struct with action draining"
```

(This will not compile standalone yet — `daemon.rs` referencing `CherryApp` is added in Task 4 and `main.rs` is fixed in Task 5. The full build is verified at the end of Task 5.)

---

### Task 4: Add `daemon.rs`

**Files:**
- Create: `src/daemon.rs`

- [ ] **Step 1: Write `src/daemon.rs`**

```rust
use std::os::unix::net::UnixListener;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{Window, WindowAttributes, WindowId};

const SOCKET_PATH: &str = "/tmp/cherry.sock";
const LOGICAL_WIDTH: f64 = 900.0;
const LOGICAL_HEIGHT: f64 = 520.0;

#[derive(Debug)]
enum UserEvent {
    Toggle,
    Kill,
}

/// GPU resources that survive hide/show cycles.
struct GpuState {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    egui_renderer: egui_wgpu::Renderer,
    surface_format: wgpu::TextureFormat,
}

/// Per-window state created on show, destroyed on hide.
///
/// `wgpu_surface` MUST be declared before `window` so it is dropped first —
/// wgpu holds its own Arc<Window> reference internally and must release it
/// before we drop our Arc here, allowing the Wayland wl_surface to be destroyed.
struct WindowState {
    wgpu_surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    window: Arc<Window>,
    egui_state: egui_winit::State,
}

/// `app` is created once in `init_full` and never rebuilt — this is what
/// makes the wallpaper texture/dimension cache persist across toggles.
struct State {
    gpu: GpuState,
    win: Option<WindowState>,
    egui_ctx: egui::Context,
    app: crate::app::CherryApp,
}

impl State {
    fn reconfigure_surface(&self) {
        if let Some(win) = &self.win {
            win.wgpu_surface.configure(&self.gpu.device, &win.surface_config);
        }
    }

    fn request_redraw(&self) {
        if let Some(win) = &self.win {
            win.window.request_redraw();
        }
    }
}

struct Daemon {
    state: Option<State>,
    proxy: EventLoopProxy<UserEvent>,
}

fn window_attrs() -> WindowAttributes {
    WindowAttributes::default()
        .with_title("Cherry")
        .with_name("uk.co.ryannavsaria.cherry", "cherry")
        .with_inner_size(LogicalSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT))
        .with_min_inner_size(LogicalSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT))
        .with_max_inner_size(LogicalSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT))
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
}

impl Daemon {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Daemon { state: None, proxy }
    }

    /// Full init: create GPU + window together (needed so adapter is surface-compatible),
    /// then create the app and kick off its initial background decode.
    fn init_full(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = window_attrs();
        let window = Arc::new(event_loop.create_window(attrs).expect("Failed to create window"));
        let scale = window.scale_factor();
        let phys = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let wgpu_surface = instance.create_surface(window.clone()).expect("Failed to create wgpu surface");

        let (adapter, device, queue) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&wgpu_surface),
                    ..Default::default()
                })
                .await
                .expect("No compatible GPU adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Failed to get device");
            (adapter, device, queue)
        });

        let caps = wgpu_surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|&m| m == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        wgpu_surface.configure(&device, &surface_config);

        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);

        let egui_ctx = egui::Context::default();
        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = crate::ui::theme::BG;
        egui_ctx.set_visuals(visuals);
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        egui_ctx.set_fonts(fonts);

        let max_texture_side = device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale as f32),
            None,
            Some(max_texture_side),
        );

        let app = crate::app::CherryApp::new();
        app.request_initial_load(egui_ctx.clone());

        self.state = Some(State {
            gpu: GpuState { instance, adapter, device, queue, egui_renderer, surface_format: format },
            win: Some(WindowState { wgpu_surface, surface_config, window, egui_state }),
            egui_ctx,
            app,
        });
    }

    /// Recreate only the window + surface using existing GPU state and the
    /// existing `app` (and its caches).
    fn open_window(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        let attrs = window_attrs();
        let window = Arc::new(event_loop.create_window(attrs).expect("Failed to create window"));
        let scale = window.scale_factor();
        let phys = window.inner_size();

        let wgpu_surface = state.gpu.instance
            .create_surface(window.clone())
            .expect("Failed to create wgpu surface");
        let caps = wgpu_surface.get_capabilities(&state.gpu.adapter);

        let format = caps.formats.iter().copied()
            .find(|&f| f == state.gpu.surface_format)
            .unwrap_or(caps.formats[0]);
        if format != state.gpu.surface_format {
            state.gpu.surface_format = format;
            state.gpu.egui_renderer =
                egui_wgpu::Renderer::new(&state.gpu.device, format, None, 1, false);
        }

        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|&m| m == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        wgpu_surface.configure(&state.gpu.device, &surface_config);

        let max_texture_side = state.gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            state.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale as f32),
            None,
            Some(max_texture_side),
        );

        state.win = Some(WindowState { wgpu_surface, surface_config, window, egui_state });
    }

    /// No fade animation: showing/hiding a window is instant (matches
    /// cherry's current UX). `showing` == a window currently exists.
    fn toggle(&mut self, event_loop: &ActiveEventLoop) {
        let showing = self.state.as_ref().map_or(false, |s| s.win.is_some());

        if showing {
            let state = self.state.as_mut().unwrap();
            state.app.on_hide();
            state.win = None;
        } else {
            if self.state.is_none() {
                self.init_full(event_loop);
            } else {
                self.open_window(event_loop);
            }
            if let Some(state) = self.state.as_mut() {
                let ctx = state.egui_ctx.clone();
                state.app.on_show(&ctx);
                state.request_redraw();
            }
        }
    }

    fn render(&mut self) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };
        if state.win.is_none() {
            return;
        }

        let raw_input = {
            let win = state.win.as_mut().unwrap();
            win.egui_state.take_egui_input(&win.window)
        };

        let full_output = state.egui_ctx.run(raw_input, |ctx| {
            state.app.ui(ctx);
        });

        {
            let win = state.win.as_mut().unwrap();
            win.egui_state.handle_platform_output(&win.window, full_output.platform_output);
        }

        for (id, delta) in &full_output.textures_delta.set {
            state.gpu.egui_renderer.update_texture(&state.gpu.device, &state.gpu.queue, *id, delta);
        }
        for id in &full_output.textures_delta.free {
            state.gpu.egui_renderer.free_texture(id);
        }

        let screen_descriptor = {
            let win = state.win.as_ref().unwrap();
            egui_wgpu::ScreenDescriptor {
                size_in_pixels: [win.surface_config.width, win.surface_config.height],
                pixels_per_point: win.window.scale_factor() as f32,
            }
        };

        let surface_texture = {
            let win = state.win.as_mut().unwrap();
            match win.wgpu_surface.get_current_texture() {
                Ok(t) => t,
                Err(wgpu::SurfaceError::Lost) => {
                    win.wgpu_surface.configure(&state.gpu.device, &win.surface_config);
                    return;
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    eprintln!("wgpu: out of memory");
                    return;
                }
                Err(_) => return,
            }
        };

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor::default(),
        );

        let primitives = state.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        state.gpu.egui_renderer.update_buffers(
            &state.gpu.device,
            &state.gpu.queue,
            &mut encoder,
            &primitives,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let mut render_pass = render_pass.forget_lifetime();
            state.gpu.egui_renderer.render(&mut render_pass, &primitives, &screen_descriptor);
        }

        state.gpu.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        // Drain actions after presenting, so Hide/Apply this frame still shows
        // the frame that triggered them (e.g. the row highlight on click).
        let actions = state.app.drain_actions();
        for action in actions {
            match action {
                crate::app::AppAction::Hide => {
                    state.app.on_hide();
                    state.win = None;
                    return;
                }
                crate::app::AppAction::Apply(path) => match crate::apply::apply(&path) {
                    Ok(()) => {
                        state.app.on_hide();
                        state.win = None;
                        return;
                    }
                    Err(err) => {
                        eprintln!("cherry: {err}");
                        crate::apply::notify(&err);
                        std::process::exit(1);
                    }
                },
            }
        }

        for (_id, viewport) in &full_output.viewport_output {
            if viewport.repaint_delay.is_zero() {
                state.request_redraw();
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for Daemon {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Lazy init: GPU and window are created on first toggle-show.
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::Init = cause {
            let proxy = self.proxy.clone();
            std::thread::spawn(move || {
                let _ = std::fs::remove_file(SOCKET_PATH);
                let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind Unix socket");
                for stream in listener.incoming() {
                    if let Ok(mut stream) = stream {
                        let mut buf = String::new();
                        use std::io::Read;
                        let _ = stream.read_to_string(&mut buf);
                        for line in buf.lines() {
                            match line.trim() {
                                "toggle" => {
                                    let _ = proxy.send_event(UserEvent::Toggle);
                                }
                                "kill" => {
                                    let _ = proxy.send_event(UserEvent::Kill);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Toggle => self.toggle(event_loop),
            UserEvent::Kill => {
                let _ = std::fs::remove_file(SOCKET_PATH);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Feed event to egui-winit for input translation
        {
            let state = match self.state.as_mut() {
                Some(s) => s,
                None => return,
            };
            let win = match state.win.as_mut() {
                Some(w) => w,
                None => return,
            };
            let response = win.egui_state.on_window_event(&win.window, &event);
            if response.repaint {
                win.window.request_redraw();
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                let state = match self.state.as_mut() {
                    Some(s) => s,
                    None => return,
                };
                if width > 0 && height > 0 {
                    if let Some(win) = state.win.as_mut() {
                        win.surface_config.width = width;
                        win.surface_config.height = height;
                    }
                    state.reconfigure_surface();
                    state.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let state = match self.state.as_mut() {
                    Some(s) => s,
                    None => return,
                };
                let phys = state.win.as_ref().map(|w| w.window.inner_size());
                if let Some(phys) = phys {
                    if phys.width > 0 && phys.height > 0 {
                        if let Some(win) = state.win.as_mut() {
                            win.surface_config.width = phys.width;
                            win.surface_config.height = phys.height;
                        }
                        state.reconfigure_surface();
                    }
                }
                state.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            _ => {}
        }
    }
}

pub fn run_daemon() {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut daemon = Daemon::new(proxy);
    event_loop.run_app(&mut daemon).expect("Event loop error");
}
```

- [ ] **Step 2: Commit**

```bash
cd /home/ryan/Projects/cherry
git add src/daemon.rs
git commit -m "feat: add daemon.rs with persistent GPU/app state across toggles"
```

(Still won't build — `main.rs` needs updating in Task 5.)

---

### Task 5: Rewrite `main.rs` for CLI dispatch, then build

**Files:**
- Modify: `src/main.rs` (full rewrite)

- [ ] **Step 1: Replace the entire contents of `src/main.rs`**

```rust
mod app;
mod apply;
mod daemon;
mod loader;
mod ui;
mod wallpapers;

use std::io::Write;
use std::os::unix::net::UnixStream;

const SOCKET_PATH: &str = "/tmp/cherry.sock";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let toggle = args.iter().any(|a| a == "--toggle");
    let kill = args.iter().any(|a| a == "--kill");

    if kill {
        match send_command(b"kill\n") {
            Ok(()) => return,
            Err(_) => {
                eprintln!("cherry: daemon is not running");
                std::process::exit(1);
            }
        }
    } else if toggle {
        match send_toggle() {
            Ok(()) => return,
            Err(_) => {
                start_daemon_background();
                for _ in 0..20 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if send_toggle().is_ok() {
                        return;
                    }
                }
                eprintln!("cherry: daemon did not start in time");
                std::process::exit(1);
            }
        }
    } else {
        daemon::run_daemon();
    }
}

fn send_toggle() -> std::io::Result<()> {
    send_command(b"toggle\n")
}

fn send_command(cmd: &[u8]) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)?;
    stream.write_all(cmd)?;
    Ok(())
}

fn start_daemon_background() {
    let exe = std::env::current_exe().expect("Cannot find current executable");
    std::process::Command::new(exe)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to start daemon");
}
```

- [ ] **Step 2: Build the whole crate**

Run: `cargo build --manifest-path /home/ryan/Projects/cherry/Cargo.toml`
Expected: builds successfully with no errors. Warnings about unused items are acceptable; fix any actual compile errors before proceeding (likely candidates: a missed import, or a `winit`/`wgpu` API mismatch — cross-check against `~/Projects/project-picker/src/daemon.rs`, which uses the identical dependency versions and compiles today).

- [ ] **Step 3: Run the full test suite**

Run: `cargo test --manifest-path /home/ryan/Projects/cherry/Cargo.toml`
Expected: all tests pass, including `diff_scan_detects_added_and_removed` and `diff_scan_no_changes_is_empty` from Task 2, plus the pre-existing `wallpapers.rs` tests.

- [ ] **Step 4: Commit**

```bash
cd /home/ryan/Projects/cherry
git add src/main.rs
git commit -m "feat: add --toggle/--kill CLI dispatch for the daemon"
```

---

### Task 6: Update README for the daemon workflow

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the "Usage" section**

Replace:

```markdown
## Usage

```sh
cherry
```

Type to fuzzy-filter wallpapers. The preview pane shows the highlighted image. `Enter` applies it (awww grow transition), updates the `~/.local/share/wallpaper/current` symlink, sends a notification, and exits.
```

with:

```markdown
## Usage

Cherry runs as a resident background daemon so wallpaper thumbnails and
dimensions stay decoded in memory between launches — only newly-added files
in `~/Pictures/backgrounds` get decoded on each open.

```sh
cherry --toggle   # show the picker, or hide it if already open
cherry --kill     # stop the daemon
cherry            # run the daemon in the foreground (used internally; --toggle
                   # auto-starts it in the background if it isn't running)
```

Type to fuzzy-filter wallpapers. The preview pane shows the highlighted image. `Enter` applies it (awww grow transition), updates the `~/.local/share/wallpaper/current` symlink, sends a notification, and hides the picker — the daemon keeps running.
```

- [ ] **Step 2: Update the "Hyprland Setup" section**

Replace:

```markdown
**Key binding** — add to `hyprland.conf` or your keybinds config:

```ini
bind = CTRL SUPER, Space, exec, cherry
```
```

with:

```markdown
**Key binding** — add to `hyprland.conf` or your keybinds config:

```ini
bind = CTRL SUPER, Space, exec, cherry --toggle
```

**Autostart** (optional but recommended — starts decoding wallpapers at login so the first toggle is instant):

```ini
exec-once = cherry
```
```

- [ ] **Step 3: Commit**

```bash
cd /home/ryan/Projects/cherry
git add README.md
git commit -m "docs: document daemon --toggle/--kill workflow"
```

---

### Task 7: Manual verification

**Files:** none (verification only)

- [ ] **Step 1: Update the Hyprland config**

Edit `~/.config/hypr/hyprland.conf`:
- change `bind=CTRL SUPER, SPACE, exec, cherry` to `bind=CTRL SUPER, SPACE, exec, cherry --toggle`
- add `exec-once=cherry` near the other `exec-once` lines (e.g. next to the existing `exec-once=mycelium`)

Reload Hyprland config (`hyprctl reload`) or note that a full re-login is needed for `exec-once` to take effect.

- [ ] **Step 2: Build a release binary and install it**

```bash
cd /home/ryan/Projects/cherry
cargo build --release
cp target/release/cherry /usr/local/bin/cherry
```

(Confirm with the user before running `cp` into `/usr/local/bin` if it requires `sudo` — match whatever the existing binary's permissions require.)

- [ ] **Step 3: Exercise the daemon lifecycle**

```bash
pkill -f 'target/release/cherry' 2>/dev/null || true   # stop any stale instance
cherry &                                                 # start daemon in foreground for log visibility
sleep 0.5
cherry --toggle    # window should appear
cherry --toggle    # window should disappear
cherry --toggle    # window should reappear — confirm this is fast (no visible decode delay)
```

- [ ] **Step 4: Verify cache correctness across a directory change**

```bash
touch ~/Pictures/backgrounds/__cherry_test__.png   # or copy a real small png in
cherry --toggle   # hide if open
cherry --toggle   # show — the new file should appear in the list without a full-library redecode delay
rm ~/Pictures/backgrounds/__cherry_test__.png
cherry --toggle
cherry --toggle   # show — the removed file should no longer appear
```

- [ ] **Step 5: Verify apply-and-hide**

In the picker, select a wallpaper and press Enter. Confirm: the wallpaper changes, a notification appears, the picker hides, and the daemon process is still running (`pgrep -f 'target/release/cherry|/usr/local/bin/cherry'` still shows it) — then toggle it open again and confirm it's instant.

- [ ] **Step 6: Verify `--kill`**

```bash
cherry --kill
pgrep -f cherry   # should show nothing
```

- [ ] **Step 7: Report results to the user**

Summarize what was verified and any deviations from expected behavior. Do not mark this plan complete until all six manual checks above pass.
