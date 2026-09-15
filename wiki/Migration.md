# Migration: smithay → Wayfire

Date: 2026-09-15

## What happened

1. Backup: `compositor/` (smithay Rust, cargo, TontooCompositor state, IPC, wallpaper, shaders, widgets) copied to `compositor_backup_20260915_071838/` (8.6 GB including target/).
2. Clear: `compositor/` emptied except `.git`.
3. Clone: `WayfireWM/wayfire` (v0.12, 0.11 is current stable) shallow-cloned and moved into `compositor/`.
4. Subprojects: manually cloned `wlroots` etc., then checked out Wayfire-pinned commits (`wlroots d7835334` = 0.20.2, `wf-config add9ba7`, etc.) to fix `wlr_xdg_decoration_manager_v1_create` mismatch.
5. Build: `meson setup /tmp/wfbuild/build --prefix=/usr -Duse_system_wlroots=disabled ...` + `ninja -j4` → `[1212/1212] Linking target src/wayfire` (verified in WSL archlinux).
6. Tontoo theming: new `wayfire.ini` (SF Pro, #1d1d1d, CSD-first, Tontoo autostart), `lang/` restored, `wayfire.ini.upstream` preserved.
7. BaseOS: `stage-compositor.sh` now builds Wayfire via meson, `start-compositor.sh` execs `wayfire`, `packages.x86_64`/`profiledef.sh`/`customize_airootfs.sh` updated, `README.md` + `wiki/` added.

## Compat

- `/usr/bin/tontoo-compositor` remains as symlink to `wayfire` so old LaunchPad/services keep working.
- Wayland socket still at `$XDG_RUNTIME_DIR/wayland-0`; old custom sockets (`tontoo-compositor.sock`, `tontoo-windows.sock`) are gone — Dock/Menubar already migrated to layer-shell + wayfire IPC. If any app still uses custom IPC, will need Wayfire IPC plugin.
- `tontoo-theme-apply` still runs before compositor to set GTK `TontooOS-Dark` etc.

## Reverting

```sh
rm -rf compositor
cp -a compositor_backup_20260915_071838 compositor
cd compositor && cargo build --release --no-default-features --features udev
```

## Upstream sync

To pull latest Wayfire (e.g. 0.11.1):

```sh
cd /tmp && git clone https://github.com/WayfireWM/wayfire.git wayfire_tmp
# check git ls-tree HEAD subprojects/wlroots etc. for new pins
# copy compositor/wayfire.ini.tontoo aside, re-clone, re-apply tontoo config
```
