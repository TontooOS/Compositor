# Menubar

> **Removed.** The compositor no longer renders its own menu bar. The
> `shell::menubar` module (`Menubar`, `MenubarItem`), the `MenuBar`
> render element, the menubar glass/logo/clock textures and the menubar
> clock in the render pump were deleted. The top bar is now the external
> `Menubar.app` system app.

## External Menubar.app

The system menu bar is a standalone TontooOS app built with TBuild from
`TontooProgramms/Menubar` (`bundle_id` `com.tontoo.menubar`):

- The ISO build stages it as an extracted bundle at
  `/System/Applications/Menubar.app` (see `BaseOS/scripts/stage-menubar.sh`).
- At boot the `menubar` LaunchPad service
  (`Library/System/Launchpads/menubar.service`, type `sys`) starts it via
  `/usr/local/bin/start-menubar.sh`, which waits for the compositor
  Wayland socket and then execs `/usr/bin/tapp
  /System/Applications/Menubar.app`. It depends on `compositor` and
  `live-setup` and restarts on crash.
- On installed systems `install-system.sh` writes a per-user variant of
  the service (`user: <username>`); the live ISO uses `liveuser`.
- Language files are staged at `/usr/share/tontoo/menubar/lang/`.

## Reserved Top Strut

The compositor renders nothing at the top of the screen. Windows are
placed below the reserved strut (see
[WaylandHandlers.md](WaylandHandlers.md)).

### ShellState::menubar_height

```rust
pub fn menubar_height(&self) -> f32
```

Returns the reserved top strut in logical pixels (currently 30.0). The
value is a constant: there is no `Menubar` state left on `ShellState`.

## Cross References

- [Shell.md](Shell.md) -- `ShellState::menubar_height` reserved strut
- [Rendering.md](Rendering.md) -- no `MenuBar` element in the z-order
- [WaylandHandlers.md](WaylandHandlers.md) -- windows map below the strut
