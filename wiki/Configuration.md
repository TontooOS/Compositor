# Configuration

The compositor persists the active color scheme in a simple key/value file and
exports the setting to environment variables that GTK clients, cursors, and the
terminal use.

## ColorScheme

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    Dark,
    Light,
}
```

The default scheme is `Dark`.

### ColorScheme::as_env_str

```rust
pub fn as_env_str(&self) -> &'static str
```

Returns `"dark"` or `"light"`.

### ColorScheme::gtk_theme_name

```rust
pub fn gtk_theme_name(&self) -> &'static str
```

Returns the GTK theme name. `Dark` maps to `"MacTahoe-Dark-blue"`, `Light` maps
to `"MacTahoe-Light-blue"`.

### ColorScheme::prefers_color_scheme

```rust
pub fn prefers_color_scheme(&self) -> &'static str
```

Returns `"prefer-dark"` or `"prefer-light"` for the `COLOR_SCHEME` variable.

### ColorScheme::clear_color

```rust
pub fn clear_color(&self) -> [f32; 4]
```

Returns the clear color used as the render background when no wallpaper is
present:

| Scheme | Value |
|---|---|
| `Dark` | `[0.11, 0.11, 0.11, 1.0]` |
| `Light` | `[0.93, 0.93, 0.93, 1.0]` |

### ColorScheme::cursor_theme_name

```rust
pub fn cursor_theme_name(&self) -> &'static str
```

Returns `"MacTahoe-dark-cursors"` for dark and `"MacTahoe-cursors"` for light.

## Config File

The config file lives at `<config_dir>/tontoo/theme.conf` where `config_dir`
comes from `dirs::config_dir()` (defaults to `.` when unavailable).

Format is `key=value`, one per line, with `#` line comments:

```
color-scheme=dark
```

### load_color_scheme

```rust
pub fn load_color_scheme() -> ColorScheme
```

Reads and parses `theme.conf`. When the file is missing or unreadable it logs an
`info` message and returns the default. Invalid `color-scheme` values log a
warning and fall back to the default.

### save_color_scheme

```rust
pub fn save_color_scheme(scheme: ColorScheme) -> Result<(), Box<dyn std::error::Error>>
```

Creates the parent directory if needed and writes `color-scheme=<value>\n`.
Returns `Err` when the directory or file cannot be written.

### parse_config

```rust
fn parse_config(contents: &str) -> ColorScheme
```

Parses the file contents. Unknown `color-scheme` values log a warning and the
function returns the default scheme.

## Environment

### apply_color_scheme_env

```rust
pub fn apply_color_scheme_env(scheme: ColorScheme)
```

Sets the following environment variables:

| Variable | Dark | Light |
|---|---|---|
| `TONTOO_COLOR_SCHEME` | `dark` | `light` |
| `GTK_THEME` | `MacTahoe-Dark-blue` | `MacTahoe-Light-blue` |
| `COLOR_SCHEME` | `prefer-dark` | `prefer-light` |
| `XCURSOR_THEME` | `MacTahoe-dark-cursors` | `MacTahoe-cursors` |
| `XCURSOR_SIZE` | `24` | `24` |
| `TERMINAL` | `foot` | `foot` |

## Constants

```rust
pub const TITLEBAR_HEIGHT: i32 = 32;
```

The server-side window titlebar height used by the render pipeline and window
hit testing.

## Cross References

- [State.md](State.md) -- `TontooCompositor::set_color_scheme` calls into this module
- [Cursor.md](Cursor.md) -- cursor theme selection is derived from `ColorScheme`
- [Rendering.md](Rendering.md) -- `TITLEBAR_HEIGHT` and clear colors are used during rendering
