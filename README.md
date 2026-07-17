# cherry

A keyboard-driven wallpaper picker for Hyprland/Wayland, based on [mycelium](https://github.com/RNAV2019/mycelium)'s UI. Lists the images in `~/Pictures/backgrounds/` with a live preview pane, and applies the selection via [awww](https://github.com/dawsers/awww).

## Requirements

- Rust (stable)
- A Wayland compositor (Hyprland recommended)
- `awww` and `notify-send` on `PATH` at runtime

## Build & Install

```sh
git clone https://github.com/RNAV2019/cherry.git
cd cherry
cargo build --release
sudo cp target/release/cherry /usr/local/bin/
```

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

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑` / `↓` / `Tab` / `Shift+Tab` | Navigate list |
| `Enter` | Apply selected wallpaper |
| `Esc` | Close |

## Hyprland Setup

**Key binding** — add to `hyprland.conf` or your keybinds config:

```ini
bind = CTRL SUPER, Space, exec, cherry --toggle
```

**Autostart** (optional but recommended — starts decoding wallpapers at login so the first toggle is instant):

```ini
exec-once = cherry
```

**Window rules**:

```ini
windowrule = match:class uk.co.ryannavsaria.cherry, float on
windowrule = match:class uk.co.ryannavsaria.cherry, center on
windowrule = match:class uk.co.ryannavsaria.cherry, border_size 0
windowrule = match:class uk.co.ryannavsaria.cherry, rounding 18
```
