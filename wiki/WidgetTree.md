# WidgetTree

The widget tree module deserializes a compact binary widget tree sent by
`tontoo_ui` clients and converts it to a flat list of draw commands for
server-side rendering.

## WidgetType

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WidgetType {
    Text = 1,
    Button = 2,
    Image = 3,
    Toggle = 4,
    TextField = 5,
    Divider = 6,
    Spacer = 7,
    Card = 8,
    VStack = 10,
    HStack = 11,
    ZStack = 12,
    Padding = 13,
    Frame = 14,
}
```

## FlatWidget

```rust
pub struct FlatWidget {
    pub widget_type: WidgetType,
    pub node_id: usize,
    pub bounds: [f32; 4],
    pub properties: WidgetProperties,
    pub children: Vec<usize>,
    pub interactive: bool,
}
```

`interactive` is `true` for `Button`, `Toggle`, and `TextField`.

## WidgetProperties

Variants carry the per-type data:

| Variant | Fields |
|---|---|
| `Text` | `content`, `font_size`, `color`, `max_width` |
| `Button` | `label`, `background`, `text_color`, `corner_radius` |
| `Image` | `path` |
| `Toggle` | `is_on`, `label` |
| `TextField` | `placeholder`, `value` |
| `Divider` | `color`, `thickness` |
| `Spacer` | `min_length` |
| `Card` | `background`, `corner_radius`, `milkiness` |
| `VStack` | `spacing`, `alignment` |
| `HStack` | `spacing`, `alignment` |
| `ZStack` | (no fields) |
| `Padding` | `top`, `right`, `bottom`, `left` |
| `Frame` | `width`, `height` (both optional) |

## Binary Format

The serialized widget tree is read by `BinaryReader` as follows:

1. Node count: `u32` LE.
2. Per node:
   - Type tag: `u8` (1-8, 10-14). Unknown tags cause parse failure.
   - Properties (type-dependent, see below).
   - Bounds: `f32 x4` (x, y, width, height) LE.
   - Child count: `u32` LE.
   - Children: `u32[]` (node indices) LE.

### Property encoding

| Type | Fields |
|---|---|
| `Text` | string, f32, color(4xf32), optional f32 |
| `Button` | string, color, color, f32 |
| `Image` | string |
| `Toggle` | u8 (0/1), string |
| `TextField` | string, string |
| `Divider` | color, f32 |
| `Spacer` | f32 |
| `Card` | color, f32, f32 |
| `VStack` | f32, u8 |
| `HStack` | f32, u8 |
| `ZStack` | (nothing) |
| `Padding` | f32 x4 |
| `Frame` | optional f32, optional f32 |

Strings are length-prefixed: `u32 LE` byte count, then raw UTF-8 bytes.
Colors are 4x `f32 LE` (r, g, b, a). Optional f32 uses a `u8` tag: `1` =
present (followed by f32), otherwise absent.

## Parsing

### parse_widget_tree

```rust
pub fn parse_widget_tree(data: &[u8]) -> Option<Vec<FlatWidget>>
```

Returns a vector of `FlatWidget`s or `None` if the binary data is malformed.
All node IDs are sequential starting at 0.

## Hit Testing

### hit_test

```rust
pub fn hit_test(widgets: &[FlatWidget], px: f32, py: f32) -> Option<usize>
```

Iterates the widget list in reverse (top-most first) and returns the
`node_id` of the first interactive widget whose bounds contain the point.
Returns `None` when no interactive widget matches.

## Rendering

### widget_tree_to_draw_commands

```rust
pub fn widget_tree_to_draw_commands(
    widgets: &[FlatWidget],
    offset_x: f32,
    offset_y: f32,
) -> Vec<DrawCommand>
```

Converts a flat widget tree into `DrawCommand`s. Each widget is offset by the
provided translation. Containers (`VStack`, `HStack`, `ZStack`, `Padding`,
`Frame`) are not layouted -- their children's bounds are used as provided by
the client.

## Cross References

- [TontooUiProtocol.md](TontooUiProtocol.md) -- `update_widget_tree` sends
  the binary data
- [WidgetRenderer.md](WidgetRenderer.md) -- `DrawCommand` enum and GPU
  rasterization
