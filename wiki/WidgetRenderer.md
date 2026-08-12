# WidgetRenderer

The widget renderer converts `DrawCommand`s into compositor render elements
suitable for the GPU pipeline. Text is rasterized via `fontdue` and cached
by content, size, and color so identical strings are only rasterized once
per renderer instance.

## Color

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
```

### Constants

`Color::TRANSPARENT` is `(0.0, 0.0, 0.0, 0.0)`.
`Color::WHITE` is `(1.0, 1.0, 1.0, 1.0)`.

### Color::new

```rust
pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self
```

### Color::to_premultiplied_rgba8

```rust
fn to_premultiplied_rgba8(self) -> [u8; 4]
```

Converts to premultiplied RGBA bytes.

## DrawCommand

```rust
pub enum DrawCommand {
    Text {
        content: String,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        max_width: Option<f32>,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        corner_radius: f32,
    },
    GlassPanel {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        milkiness: f32,
        alpha: f32,
        corner_radius: f32,
    },
    Texture {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        texture_id: u64,
    },
}
```

`Texture` is a placeholder -- the texture_id lookup is not yet implemented.

## WidgetRenderer

```rust
pub struct WidgetRenderer {
    font: Option<Font>,
    text_cache: HashMap<TextCacheKey, CachedText>,
}
```

### WidgetRenderer::new

```rust
pub fn new() -> Self
```

Loads a system font by searching well-known Linux paths. The search order
includes SF Pro, DejaVu, Liberation, and Noto fonts.

## Rendering Methods

### render_text_cmd

```rust
fn render_text_cmd(
    &mut self,
    renderer: &mut GlesRenderer,
    content: &str,
    x: f32,
    y: f32,
    font_size: f32,
    color: Color,
) -> Option<TextureRenderElement<GlesTexture>>
```

Rasterizes the text, tints it with the provided color, uploads to GPU on first
use, and returns a render element. Empty content returns `None`. The rasterized
pixels are Y-flipped for OpenGL compatibility.

### render_rect_cmd

```rust
pub(crate) fn render_rect_cmd(
    renderer: &mut GlesRenderer,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color,
) -> Option<TextureRenderElement<GlesTexture>>
```

Creates a 1x1 solid-color texture scaled to the requested size.

### render_glass_cmd

```rust
pub(crate) fn render_glass_cmd(
    renderer: &mut GlesRenderer,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    milkiness: f32,
    alpha: f32,
) -> Option<TextureRenderElement<GlesTexture>>
```

Creates a 1x1 semi-transparent glass texture scaled to the requested size.
The RGB channels are `milkiness * alpha`.

## Public API

### render_draw_commands

```rust
pub fn render_draw_commands(
    renderer: &mut GlesRenderer,
    commands: &[DrawCommand],
) -> Vec<TextureRenderElement<GlesTexture>>
```

Converts `DrawCommand`s to render elements. Creates a fresh `WidgetRenderer`
on each call -- the text cache does not persist across frames.

### render_draw_commands_with

```rust
pub fn render_draw_commands_with(
    renderer: &mut GlesRenderer,
    commands: &[DrawCommand],
    widget_renderer: &mut WidgetRenderer,
) -> Vec<TextureRenderElement<GlesTexture>>
```

Like `render_draw_commands` but reuses an existing `WidgetRenderer` so its
text cache persists across frames.

## Text Rasterization

Text is rendered character-by-character using `fontdue::Font::rasterize`.
Each glyph bitmap is composited onto an RGBA buffer. The Y axis is flipped
for OpenGL. The text cache key is `(content, font_size, r, g, b, a)`.

## Cross References

- [WidgetTree.md](WidgetTree.md) -- `DrawCommand` is consumed from the widget
  tree
- [TontooUiProtocol.md](TontooUiProtocol.md) -- `TontooUiSurfaceState` holds
  a `WidgetRenderer` for persistence
- [Rendering.md](Rendering.md) -- draw commands are rendered as
  `TontooUiTextureElement`s
