# Tontoo Compositor (Wayfire) — Wiki

> Wayfire 0.12 (wlroots 0.20) themed for TontooOS. Previous smithay Rust compositor is archived at `../compositor_backup_20260915_071838/wiki/`.

- Repository: https://github.com/TontooOS/Compositor (now Wayfire-based)
- Upstream: https://github.com/WayfireWM/wayfire
- Version: 0.12.0 (TontooOS 26.1.0)
- License: MIT (Wayfire) + TCL for TontooOS overlays
- Config: `wayfire.ini` (TontooOS default), `wayfire.ini.upstream` (upstream), `wayfire.ini.tontoo` (duplicate)

## Contents

| Page | File | Description |
|---|---|---|
| Overview | [MAIN.md](./MAIN.md) | This page |
| Building | [Building.md](./Building.md) | meson/ninja, subprojects, WSL verification |
| Configuration | [Configuration.md](./Configuration.md) | TontooOS wayfire.ini, decoration, autostart |
| Theme | [Theme.md](./Theme.md) | SF Pro, #1d1d1d/#ececec, GTK/Qt theming |
| Wayfire Plugins | [Plugins.md](./Plugins.md) | Enabled plugins, blur, animation |
| Hardware | [Hardware.md](./Hardware.md) | Mesa/NVIDIA, Vulkan, HiDPI, Pi |
| BaseOS Integration | [BaseOS.md](./BaseOS.md) | stage-compositor, airootfs, ISO |
| Migration | [Migration.md](./Migration.md) | smithay → Wayfire, backup, compat |

## Quick Start

```sh
# dev build (Arch/WSL)
meson setup build --prefix=/usr -Duse_system_wlroots=disabled -Duse_system_wfconfig=disabled -Dxwayland=enabled
ninja -C build
build/src/wayfire -c wayfire.ini

# ISO stage (from BaseOS)
BaseOS/scripts/stage-compositor.sh
BaseOS/scripts/build-iso.sh
```

Wayfire reads `~/.config/wayfire.ini` first, then `/usr/share/wayfire/wayfire.ini.tontoo` fallback.

## Why Wayfire?

See user summary in initial task: Wayfire 0.11 is production-close, active maintenance, explicit sync for NVIDIA (PR #3080), Vulkan renderer, fractional scaling crisp, runs on ARM (Pi), Mesa out-of-box. Smithay compositor was custom but required maintaining entire compositor stack; Wayfire gives stability, hardware coverage, and Compiz-like effects while TontooOS keeps its shell (Menubar.app, Dock.app) on top.

## Changelog

- 2026-09-15: **Replace smithay compositor with Wayfire 0.12** — backup to `compositor_backup_20260915_071838`, clone Wayfire, fix subprojects to wlroots 0.20.2 commits, verify meson+ninja build (1212/1212), add TontooOS `wayfire.ini` (SF Pro, #1d1d1d, CSD-first, Tontoo autostart), `lang/` bilingual, `stage-compositor.sh` meson build, `start-compositor.sh` wayfire exec, `packages.x86_64` + `profiledef.sh` + `customize_airootfs.sh` integration. See [Migration.md](./Migration.md).
