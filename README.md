# Tontoo Compositor — Wayfire Edition

> **TontooOS compositor based on [Wayfire 0.12](https://wayfire.org) (wlroots 0.20)**
> Previous smithay-based Rust compositor is archived at [`../compositor_backup_20260915_071838`](../compositor_backup_20260915_071838).

Wayfire is a 3D Wayland compositor inspired by Compiz, based on wlroots. TontooOS ships a themed fork with:

- **SF Pro** system font (`/usr/share/fonts/OTF/SF-Pro-Display-Regular.otf`), decoration font `SF Pro Display`
- **Dark `#1d1d1d` / Light `#ececec`** decoration colors (see `AGENTS.md`)
- **CSD-first** — apps draw their own traffic-light headers via `tontoo-theme-apply`; Wayfire `decoration` plugin only decorates XWayland fallback windows
- **LiquidGlass** blur (`kawase`, optional) for frosted transparency
- **TontooOS autostart** — `Menubar.app`, `Dock.app`, `Settings daemon` via `start-*.sh` (no `wf-shell`)
- **Vulkan renderer + explicit sync** (`linux-drm-syncobj-v1`) for NVIDIA 555+ stability, fractional scaling crisp text (Wayfire 0.11+)
- Upstream docs: [Tutorial](https://github.com/WayfireWM/wayfire/wiki/Tutorial) · [Configuration](https://github.com/WayfireWM/wayfire/wiki/Configuration)

Upstream: https://github.com/WayfireWM/wayfire — MIT license (see `LICENSE`)

## Quick Start

### Arch Linux (dev)

```sh
# deps
sudo pacman -S meson ninja pkgconf wayland wayland-protocols libdrm mesa libinput pixman cairo pango glm libxkbcommon xorg-xwayland

# Wayfire loads wlroots/wf-config as subprojects if not on system
meson setup build --prefix=/usr -Duse_system_wlroots=disabled -Duse_system_wfconfig=disabled -Dxwayland=enabled
ninja -C build
sudo ninja -C build install  # installs wayfire to /usr/bin/wayfire + plugins to /usr/lib/wayfire
```

Run from TTY:

```sh
wayfire                 # reads ~/.config/wayfire.ini or ./wayfire.ini
wayfire -c wayfire.ini.tontoo
```

### TontooOS ISO

`BaseOS/scripts/stage-compositor.sh` builds Wayfire and installs:

- `airootfs/usr/bin/wayfire` (+ symlink `tontoo-compositor` for compat)
- `airootfs/usr/share/wayfire/wayfire.ini.tontoo` + `wayfire.ini.upstream`
- `airootfs/etc/skel/.config/wayfire.ini` (copied to `liveuser` on first boot)
- `airootfs/usr/share/wayland-sessions/{wayfire,tontoo}.desktop`

`airootfs/usr/local/bin/start-compositor.sh` now execs `wayfire` (falls back to `tontoo-compositor` if wayfire missing).

## Configuration

TontooOS default config: [`wayfire.ini`](./wayfire.ini) (same as `wayfire.ini.tontoo`), upstream default saved as `wayfire.ini.upstream`.

Copy to edit:

```sh
cp wayfire.ini ~/.config/wayfire.ini
# or system-wide: /usr/share/wayfire/wayfire.ini.tontoo -> ~/.config/wayfire.ini
```

Key TontooOS sections:

- `[core]` — lean plugins, `preferred_decoration_mode = client`
- `[decoration]` — `font = SF Pro Display`, `active_color = #1d1d1ddd`, `inactive_color = #ecececcc`
- `[autostart]` — `menubar = /usr/local/bin/start-menubar.sh`, `dock = ...`, `settings_daemon = ...`, `autostart_wf_shell = false`
- `[workarounds]` — uncomment `force_frame_sync = true` on NVIDIA if you see flickering

Wallpaper fallback: `swaybg` pointing at `/usr/share/wallpapers/tontoo/Tahoe-Light-6K.png`; real wallpaper is set by Settings daemon via IPC.

## Colors & Theme

- Dark background color `#1d1d1d`, light `#ececec` per `AGENTS.md` — Wayfire respects them in `decoration.active_color` / `inactive_color`. Light mode is toggled by `tontoo-theme-apply` which also flips GTK `TontooOS-Light` vs `-Dark`.
- Fonts are loaded from system paths `/usr/share/fonts/OTF/SF-Pro-Display-Regular.otf` etc., staged by `BaseOS/scripts/stage-fonts.sh`.

## Languages

App strings remain bilingual (AGENTS.md):

- [`lang/en_us.json`](./lang/en_us.json)
- [`lang/de_de.json`](./lang/de_de.json)

Wayfire itself uses `locale/` gettext.

## Backup

Previous Rust/smithay compositor (v26.1.0) archived to:

```
../compositor_backup_20260915_071838/
```

Includes its `wiki/` (TontooCompositor docs). New Wayfire-based wiki is at [`wiki/`](./wiki) (TODO: port relevant pages).

## Build verification (WSL)

```sh
wsl -d archlinux -- bash -c "
  rm -rf /tmp/wfbuild/build && \
  meson setup /mnt/c/Users/arlo1/Documents/TontooOS/compositor /tmp/wfbuild/build \
    --prefix=/usr -Duse_system_wlroots=disabled -Duse_system_wfconfig=disabled -Dxwayland=enabled && \
  ninja -C /tmp/wfbuild/build -j4
"
```

Should finish `[1212/1212] Linking target src/wayfire`.

## License

Wayfire is MIT (`LICENSE`). TontooOS additions (tontoo config, scripts, fonts staging) follow TCL v26.1 where applicable. SF Pro fonts are Apple-licensed, redistributed via BaseOS/fonts staging only.
