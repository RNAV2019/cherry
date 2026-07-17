# Cherry: daemon architecture for image/dimension caching

## Problem

Cherry currently runs as a brand-new `eframe` process on every launch. Each
launch rescans `~/Pictures/backgrounds`, decodes every image, and downscales
it to a GPU texture from scratch — causing a subtle delay when opening the
picker.

## Goal

Give cherry the same resident-daemon architecture as `mycelium` and
`project-picker`, so decoded images, filenames, and dimensions persist in
memory across toggles instead of being rebuilt on every launch. Keep cherry's
existing UI layout (search bar + left list + right preview pane) — this is
not a UI redesign, just a process-model change plus the diff-based rescan
needed to keep the cache correct.

## Architecture

### Process model

- `main.rs` gains CLI dispatch identical to `project-picker`/`mycelium`:
  - `cherry --toggle` — send `"toggle\n"` over the Unix socket
    (`/tmp/cherry.sock`); if connect fails, spawn `cherry` as a background
    daemon and retry for up to 2s.
  - `cherry --kill` — send `"kill\n"`.
  - `cherry` (no args) — run the daemon event loop.
- Hyprland config: `bind=CTRL SUPER, Space, exec, cherry --toggle` plus
  `exec-once=cherry` so the daemon (and its background image decode) is
  already warm at login, before the hotkey is ever pressed.

### daemon.rs

Ported from `project-picker`'s `daemon.rs` (closest match — no animation
state, unlike mycelium):

- `GpuState`: `instance`/`adapter`/`device`/`queue`/`egui_renderer`/
  `surface_format` — created once in `init_full`, never recreated.
- `WindowState`: `wgpu_surface`/`surface_config`/`window`/`egui_state` —
  created on show, dropped on hide (releases the Wayland surface).
- `State`: `gpu`, `win: Option<WindowState>`, `egui_ctx`, `app: CherryApp`.
  `app` is created once and never rebuilt — this is what makes the texture
  and dimension cache persistent.
- `toggle()`: same show/hide state machine as the sibling apps, minus
  animation — window is created/destroyed instantly (matches cherry's
  current UX; no fade).
- Unix socket listener on a background thread, `"toggle"`/`"kill"` commands.

### App state (`app.rs`, née `CherryApp`)

- Becomes a plain struct owned by `daemon::State`, not an `eframe::App`.
- `ui(&mut self, ctx: &egui::Context)` replaces the `eframe::App::update`
  impl; the daemon's `render()` calls it inside `egui_ctx.run(...)`.
- `on_show(&mut self)`: called on every toggle-open. Resets `query` and
  `selected_idx` (matches current behavior), then rescans and diffs (see
  below).
- `on_hide(&mut self)`: no-op placeholder for symmetry with the sibling
  apps (no animation state to reset).
- Applying a wallpaper no longer exits the process. `handle_keyboard`/
  `render_list` push `AppAction::Apply(PathBuf)` instead of calling
  `apply_and_close` directly; the daemon drains actions after each frame
  (same pattern as `project-picker`'s `AppAction::OpenTerminal`), runs
  `apply::apply()`, and on success triggers hide instead of
  `ViewportCommand::Close`. On `apply::apply()` error, keep today's
  behavior (print to stderr, notify, `std::process::exit(1)`) — a failed
  apply is still fatal, it just no longer needs to close a window that a
  whole-process-exit would otherwise handle.
- Escape now hides the window (`AppAction::Hide`) instead of closing the
  viewport.

### Rescan-and-diff on show

On every `on_show()`:

1. `wallpapers::scan_wallpapers(&dir)` → fresh `Vec<Wallpaper>`.
2. Diff against the cached list by path:
   - **added** (in fresh, not in cached): appended to `self.wallpapers`,
     queued via `Loader::request_all` for background decode.
   - **removed** (in cached, not in fresh): dropped from `self.wallpapers`;
     their entries in `textures` and `dims` are removed (`HashMap::remove`
     drops the `TextureHandle`, which frees the GPU texture via egui's
     refcounting — no manual GPU cleanup needed).
   - **unchanged**: left as-is; no decode, no texture reload.
3. Selection/query reset happens after the diff so filtering reflects the
   current file list.

The diff itself is a pure function (`Vec<Wallpaper>`, `Vec<Wallpaper>` in,
added/removed out) with no GPU or egui dependency, so it's unit-testable
like the existing `wallpapers.rs` tests.

### UI layout — unchanged

The search bar, left scrollable list (filename + dimensions subtitle), and
right preview pane stay exactly as they are today (`ui/list.rs`,
`ui/preview.rs`, `ui/search.rs`, `ui/hints.rs`, `ui/theme.rs` are untouched).
This is explicitly *not* a switch to mycelium's single-list layout — mycelium
is the architectural reference (daemon/socket/GPU lifecycle), not the visual
one.

### Dependencies (Cargo.toml)

Replace `eframe` with the stack `project-picker` already uses:
`winit`, `wgpu`, `egui-wgpu`, `egui-winit`, `pollster`. Keep `egui`,
`egui-phosphor`, `image`.

## Data flow summary

```
login: exec-once=cherry → daemon starts → init_full() creates GpuState +
  CherryApp (empty) → CherryApp kicks off Loader::request_all for the
  initial scan in the background

hotkey: cherry --toggle → socket → daemon.toggle() → open_window() (reuses
  GpuState) → app.on_show() → rescan+diff → only new files decode;
  everything else served from the in-memory textures/dims maps

Enter: AppAction::Apply(path) → apply::apply() → hide (daemon keeps running,
  cache stays warm) — or exit(1) on failure, as today

Escape / re-press hotkey while open: AppAction::Hide → window dropped,
  GpuState + CherryApp (cache) persist
```

## Error handling

No new error paths beyond what already exists: image decode failures are
already skipped (`let Ok(img) = image::open(&path) else { continue }`);
`apply::apply()` failure behavior is preserved as described above. Socket
bind/connect errors follow the exact pattern already proven in
`project-picker`/`mycelium`.

## Testing

- Existing `wallpapers.rs` unit tests (scan, fuzzy match, filter) are
  unaffected.
- Add unit tests for the added/removed diff function using `tempfile`
  directories, mirroring `scan_finds_only_images_sorted`.
- Manual verification (per the `run`/`verify` workflow): build, run the
  daemon, toggle open/closed via socket commands, add/remove a file in
  `~/Pictures/backgrounds` and confirm it appears/disappears on next
  toggle without re-decoding unrelated images, apply a wallpaper and
  confirm the daemon survives and the next toggle is instant.
