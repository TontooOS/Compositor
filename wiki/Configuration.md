# Configuration

TontooOS default config: `wayfire.ini` (installed to `~/.config/wayfire.ini` on first boot).

## Core

```ini
[core]
plugins = alpha animate autostart command decoration foreign-toplevel grid idle move place resize switcher vswitch wayfire-shell window-rules wm-actions blur expo cube wobbly zoom
preferred_decoration_mode = client
```

`client` = CSD-first: GTK/Qt apps draw own traffic lights via `tontoo-theme-apply`. Wayfire `decoration` only for XWayland.

## Decoration (SF Pro, Tontoo colors)

```ini
[decoration]
font = SF Pro Display
active_color = #1d1d1ddd   # AGENTS.md dark #1d1d1d
inactive_color = #ecececcc  # AGENTS.md light #ececec
title_height = 28
border_size = 0
```

Fonts loaded from `/usr/share/fonts/OTF/SF-Pro-Display-Regular.otf` etc. (stage-fonts.sh).

## Autostart

```ini
[autostart]
autostart_wf_shell = false
menubar = /usr/local/bin/start-menubar.sh
dock    = /usr/local/bin/start-dock.sh
settings_daemon = /usr/local/bin/start-settingsdaemon.sh
wallpaper = swaybg -i /usr/share/wallpapers/tontoo/Tahoe-Light-6K.png -m fill || true
theme = /usr/local/bin/tontoo-theme-apply 2>/dev/null || true
```

`start-*.sh` wait for Wayland socket before launching `tapp /System/Applications/*.app`.

## Workarounds (NVIDIA)

```ini
[workarounds]
# force_frame_sync = true  # uncomment on NVIDIA flickering (explicit sync PR #3080)
```

Alternatively run `WLR_RENDERER=vulkan wayfire` on NVIDIA 555+.

## Inputs / Outputs

Use `wlr-randr` or `kanshi` for `[output:*]` sections. Fractional scaling is auto crisp via Wayfire 0.11+ floating geometry.
