# Tontoo Compositor – Wiki

The Wayland compositor for TontooOS, built on smithay 0.7. It renders the
desktop shell (dock, menubar, window decorations) on the GPU, runs on either
the winit or the udev/DRM backend, and exposes a custom `tontoo_ui` Wayland
protocol for server-side-rendered applications.

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
| Menubar | [Menubar.md](Menubar.md) | macOS-style top menu bar |
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

- 2026-08-12: Initial wiki, extracted from the current source tree.
