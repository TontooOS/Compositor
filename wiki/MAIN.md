# Tontoo Compositor – Wiki

The Wayland compositor for TontooOS, built on smithay 0.7. It renders the
desktop shell (dock, window decorations) on the GPU, runs on either
the winit or the udev/DRM backend, and exposes a custom `tontoo_ui` Wayland
protocol for server-side-rendered applications. The top menu bar is not
rendered here; it is the external `Menubar.app` system app (see
[Menubar.md](Menubar.md)).

- Repository: https://github.com/TontooOS/Libs
- License: TCL v26.1
- Version: 26.1.0

## Feature Index

| Feature | File | Description |
|---|---|---|
| Main index | [MAIN.md](MAIN.md) | This page |
| Rules | [RULE.md](RULE.md) | Development and usage rules |
| State | [State.md](State.md) | Core `TontooCompositor` state and initialization |
| Configuration | [Configuration.md](Configuration.md) | Color scheme, theme config, environment |
| Animation | [Animation.md](Animation.md) | Animation and animation manager |
| Accessibility | [Accessibility.md](Accessibility.md) | Reduce transparency / reduce motion settings |
| Cursor | [Cursor.md](Cursor.md) | XCursor loading, magnification, render elements |
| Wallpaper | [Wallpaper.md](Wallpaper.md) | Wallpaper loading |
| Shell | [Shell.md](Shell.md) | Aggregated shell state |
| Dock | [Dock.md](Dock.md) | macOS-style dock with magnification |
| Menubar | [Menubar.md](Menubar.md) | External `Menubar.app` system app (removed from compositor) |
| Launcher | [Launcher.md](Launcher.md) | Application launcher overlay |
| Topbar | [Topbar.md](Topbar.md) | Simple glass top bar |
| WindowControls | [WindowControls.md](WindowControls.md) | Traffic light window buttons |
| Input | [Input.md](Input.md) | Input event processing and shortcuts |
| Grabs | [Grabs.md](Grabs.md) | Move and resize pointer grabs |
| WaylandHandlers | [WaylandHandlers.md](WaylandHandlers.md) | smithay protocol handler implementations |
| TontooUiProtocol | [TontooUiProtocol.md](TontooUiProtocol.md) | Custom `tontoo_ui` Wayland protocol |
| WidgetTree | [WidgetTree.md](WidgetTree.md) | Binary widget tree parser and hit testing |
| WidgetRenderer | [WidgetRenderer.md](WidgetRenderer.md) | Draw commands and GPU widget rasterization |
| Rendering | [Rendering.md](Rendering.md) | Winit render pipeline and z-order |
| RenderCache | [RenderCache.md](RenderCache.md) | Cached compositor textures |
| TextureCache | [TextureCache.md](TextureCache.md) | Content-hashed GPU texture cache |
| UdevBackend | [UdevBackend.md](UdevBackend.md) | Udev/DRM/libseat backend |
| XWayland | [XWayland.md](XWayland.md) | X11 apps via embedded XWayland server + window manager |
| WindowsIpc | [WindowsIpc.md](WindowsIpc.md) | CoreWindows socket: window listing and actions |
| Shaders | [Shaders.md](Shaders.md) | Gaussian blur GLSL shaders |

## Quick Start

TontooCompositor is a binary crate. Build and run it on an existing display
server (development) with the winit backend:

```bash
cargo build
cargo run -- --winit
```

On a TTY with the udev feature (DRM/KMS, libseat, libinput), build the full
binary first:

```bash
cargo build --features udev
cargo run --no-default-features --features udev -- --udev
```

When neither flag is passed, the compositor defaults to the udev backend if
the `udev` feature is compiled in, otherwise to winit.

See [State.md](State.md) for the initialization flow and [Input.md](Input.md)
for built-in shortcuts.

## Changelog

- 2026-09-10: CSD-first theming, enforcement removed — the compositor
  never forces decorations again (`request_mode` only honors explicit
  opt-in); one central theme push instead: `tontoo-theme-apply`
  (`theme.service` at login, also called by Settings) writes gsettings,
  GTK `settings.ini`, qt5ct/qt6ct configs with TontooOS palettes, the
  Chromium Wayland hint and Flatpak overrides from
  `~/.config/tontoo/theme.conf`; session env gains
  `QT_QPA_PLATFORMTHEME=qt5ct` and `ELECTRON_OZONE_PLATFORM_HINT=auto`.
  BaseOS adds `dconf`, `gsettings-desktop-schemas`,
  `xdg-desktop-portal-gtk`, `qt5ct`, `qt6ct`, a portal routing config,
  an SF Pro fontconfig default and a Chromium Wayland-hint skel file;
  Chrome/VSCode skel seeds switched back to app-drawn headers
  (CSD-first). See   [WindowControls.md](WindowControls.md),
  [Configuration.md](Configuration.md) and
  [WaylandHandlers.md](WaylandHandlers.md).
- 2026-09-09: Windows IPC socket for CoreWindows (`src/windows_ipc.rs`,
  `/run/tontoo-windows.sock`, `WINDOWS_SOCKET` override): `ping`,
  `list_windows` (mapped + minimized, with app id/title/pid), minimize
  (reuses SSD minimize-to-dock), fullscreen set/unset (geometry saved
  and restored), graceful close (`send_close` / `WM_DELETE_WINDOW`).
  Stable per-surface window ids, non-fatal bind, both backends. See
  [WindowsIpc.md](WindowsIpc.md).
- 2026-09-08: Ported to smithay git master (pinned rev `d4bb0de`, pre-0.8.0):
  all `delegate_*` macros replaced by `delegate_dispatch2!`, new
  `RenderElement::draw` cache param, `InputTime` instead of `time_msec`,
  `NodeFilter::None` for the GBM exporter, `PhysicalProperties`
  `serial_number`, new `XWayland::spawn`/`start_wm` signatures, empty
  `WaylandDndGrabHandler`/`DndGrabHandler`/`PointerConstraintsHandler`
  impls, `with_committed_state` instead of `ToplevelSurface::current_state`.
  Backup of the 0.7 tree at `../compositor-0.7-backup-20260908`.
- 2026-09-08: Layer-shell bars render (Menubar.app): the render pump
  composites Top/Overlay layer surfaces above windows and
  Background/Bottom below (both backends), and `CompositorHandler::commit`
  re-arranges the layer map plus `send_pending_configure` on layer commits
  (without it clients wait for the initial configure forever). Menubar runs
  with `GDK_BACKEND=wayland` + `gtk4-layer-shell` (Top anchors, 36px
  exclusive zone). See [Menubar.md](Menubar.md), [WaylandHandlers.md](WaylandHandlers.md).
- 2026-09-08: X11 panic-safety: `client_compositor_state` serves the
  XWayland-internal client's own state (smithay inserts
  `XWaylandClientData`, not `ClientState`) with a static fallback instead
  of unwrapping; `state::window_wl_surface_any` replaces all
  `.toplevel().unwrap()` space scans (X11 windows have no xdg toplevel).
  Fixed a crash-loop once XWayland actually started. See
  [XWayland.md](XWayland.md).
- 2026-09-07: Added XWayland support (udev backend): embedded Xwayland server (`XWAYLAND_NO_GLAMOR=1`), `X11Wm` window manager mapping X11 top-levels into `Space`, `DISPLAY` export, X11-aware click focus. See [XWayland.md](XWayland.md).

- 2026-09-07: Enforce SSD for foreign-header apps — new
  `shell::ssd::FORCE_SSD_APP_IDS` list (Chrome, Firefox, VSCode);
  `new_decoration`, `request_mode` and `unset_mode` always answer
  `ServerSide` for listed app IDs, so no per-app setup is needed.
  Also removed a duplicate unconditional `mod udev` in `main.rs`.
  (Reverted 2026-09-10 in favor of CSD-first, see top entry.)
- 2026-09-07: Server-side decorations for foreign apps — `request_mode`
  now honors client requests (KWin-style) instead of forcing CSD; new
  shared `shell::ssd` module renders a traffic-light titlebar (Dark
  `#1d1d1d` / Light `#ececec`) for SSD windows on both winit and udev
  backends; close/maximize/minimize/drag work on the bar, minimize pins
  a temporary dock icon for restore (all strictly opt-in since
  2026-09-10). See [WindowControls.md](WindowControls.md) and
  [WaylandHandlers.md](WaylandHandlers.md).
- 2026-09-07: Fix traffic lights and theme toggle — traffic lights
  (`button-layout`, `gtk-decoration-layout`) are fixed once and no longer
  switch; only `gtk-theme` and `color-scheme` toggle via
  `org.gnome.desktop.interface`; default `icon-theme` is now `MacTahoe`
  (was `Adwaita`) in `90_tontoo.gschema.override`; theme switcher scripts
  preserve the decoration layout. See [Configuration.md](Configuration.md).

- 2026-09-07: Fix oversized udev framebuffer (4K+ on smaller screens) — `scan_connectors` picked the largest advertised DRM mode by area; both output and surface now use the native EDID `PREFERRED` mode via `pick_connector_mode` with full mode logging. See [UdevBackend.md](UdevBackend.md).
- 2026-09-06: Remove the compositor-internal menubar — deleted `shell::menubar` (`Menubar`, `MenubarItem`), the `MenuBar` render element, menubar glass/logo/clock rendering (winit + udev), `RenderCache::{menubar_glass, tontoo_logo, clock_text}`, `TontooCompositor::last_clock_minute`, `current_minute_of_day` and the clock-minute render pump trigger. The top bar is now the external `Menubar.app` system app (TBuild bundle at `/System/Applications/Menubar.app`, `menubar` LaunchPad service). The compositor only reserves the 30.0 px top strut (`ShellState::menubar_height`); windows map below it. See [Menubar.md](Menubar.md).
- 2026-09-06: Fix ~30s typing stall in Wayland clients on the udev backend — root cause was a missing `DisplayHandle::flush_clients` after input processing and after the render timer (events/frame callbacks sat in userspace buffers until unrelated client traffic flushed them; the winit backend already flushed). Added both flushes in `src/udev.rs`. See [UdevBackend.md](UdevBackend.md) troubleshooting section.
- 2026-09-06: Fix black screen on VirtualBox boot (SSH works, display black) — root cause was `plymouthd` holding the DRM master (`seatd: Could not make device fd drm master: Device or resource busy`, compositor `scan_connectors: Permission denied` on `/dev/dri/card0`). Boot integration fix in `BaseOS`: add `0755` `file_permissions` for `start-compositor.sh`/`tontoo-sshd.sh`/`tontoo-net-up.sh` in `profiledef.sh`, harden `start-compositor.sh` (sudo plymouth quit, 10s wait, stderr logging). See [UdevBackend.md](UdevBackend.md) troubleshooting section.

- 2026-08-28: Fix compositor `stopped` on boot — add `XDG_RUNTIME_DIR` fallback in `init_wayland_listener` (`RuntimeDirNotSet` panic at `src/state.rs:203` when started without LaunchPad env), fix `start-compositor.sh` to set `XDG_RUNTIME_DIR`, handle plymouth DRM master (`plymouth deactivate/quit` via sudo, `pkill @lymouthd`, wait for `fuser /dev/dri/card0`), make script executable, clean stale `wayland-*.lock`; fix LaunchPad `restart` for stopped services and retry on spawn failure with backoff, add `.sh` fallback via `/bin/sh`.
- 2026-08-27: Remove compositor-side topbar/titlebar — switch to Client-Side Decorations (CSD): apps now draw their own decoration bar; compositor keeps only shadow (improved 3-layer shadow with vertical bias, pad 64, offset 12, radii 10) + rounded border; `XdgDecorationHandler` now forces `ClientSide`; input titlebar/traffic-light handling removed; docs updated.
- 2026-08-23: Fix 60 fps DRM commit storm on VirtualBox vmwgfx — render pump is now event-driven (only on `pending_redraw`, dock/animations, or clock minute change); dock uses real `dt` and `is_animating()`.
- 2026-08-12: Initial wiki, extracted from the current source tree.
