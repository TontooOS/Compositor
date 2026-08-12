# Shell

The `ShellState` struct aggregates all desktop shell components into a single
state object. It is stored on `TontooCompositor` as `self.shell`.

## ShellState

```rust
pub struct ShellState {
    pub dock: Dock,
    pub menubar: Menubar,
    pub launcher: Launcher,
    pub topbar: Topbar,
    pub window_controls: std::collections::HashMap<String, WindowControls>,
}
```

### ShellState::new

```rust
pub fn new() -> Self
```

Creates the shell state with a pre-populated dock containing five icons:
Finder, Terminal, Settings, Notes, Podcasts. The menubar, launcher, and topbar
are initialized to their defaults. The `window_controls` map starts empty.

### ShellState::launcher_visible

```rust
pub fn launcher_visible(&self) -> bool
```

Returns `true` when the launcher overlay is visible.

### ShellState::toggle_launcher

```rust
pub fn toggle_launcher(&mut self)
```

Toggles the launcher overlay between visible and hidden.

### ShellState::dock_height

```rust
pub fn dock_height(&self) -> f32
```

Returns the dock panel height in logical pixels (currently 78.0).

### ShellState::menubar_height

```rust
pub fn menubar_height(&self) -> f32
```

Returns the menubar height in logical pixels (currently 28.0).

## Window Controls Map

The `window_controls` field maps window identifiers
(`"{x}_{y}"` strings) to their `WindowControls` traffic-light state. Entries
are created on first hit-test and persist for the lifetime of the compositor.

## Cross References

- [State.md](State.md) -- `TontooCompositor::shell` field
- [Dock.md](Dock.md) -- the `Dock` component
- [Menubar.md](Menubar.md) -- the `Menubar` component
- [Launcher.md](Launcher.md) -- the `Launcher` component
- [Topbar.md](Topbar.md) -- the `Topbar` component
- [WindowControls.md](WindowControls.md) -- the `WindowControls` component
