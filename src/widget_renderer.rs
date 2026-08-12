//! Widget renderer — converts DrawCommands into compositor render elements.
//!
//! Each [`DrawCommand`] variant is mapped to a [`TextureRenderElement`] suitable
//! for inclusion in the compositor's render pipeline. Text is rasterised via
//! [`fontdue`] and cached by (content, size, colour) so identical strings are
//! only rasterised once per frame.

use std::collections::HashMap;

use fontdue::Font;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::{
    element::{
        texture::{TextureBuffer, TextureRenderElement},
        Kind,
    },
    gles::{GlesRenderer, GlesTexture},
};
use smithay::utils::{Logical, Physical, Point, Size, Transform};

// ═══════════════════════════════════════════════════════════════
// Local colour type — matches TontooUI's text_engine::Color
// ═══════════════════════════════════════════════════════════════

/// Simple RGBA colour with f32 channels in `0.0 ..= 1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Convert to premultiplied RGBA bytes.
    fn to_premultiplied_rgba8(self) -> [u8; 4] {
        let a = self.a;
        [
            (self.r * a * 255.0) as u8,
            (self.g * a * 255.0) as u8,
            (self.b * a * 255.0) as u8,
            (a * 255.0) as u8,
        ]
    }
}

// ═══════════════════════════════════════════════════════════════
// DrawCommand — local mirror of TontooUI's flattened rendering
//               instructions so the compositor crate stays
//               dependency-free.
// ═══════════════════════════════════════════════════════════════

/// A single rendering instruction produced by flattening the widget tree.
///
/// This mirrors `tontoo_ui::widget_tree::DrawCommand` exactly.  The two
/// types are intentionally kept in sync.
#[derive(Debug, Clone)]
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

// ═══════════════════════════════════════════════════════════════
// WidgetRenderer
// ═══════════════════════════════════════════════════════════════

/// Cache key for a rasterised text entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_size: u32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

/// A cached rasterised text entry: raw RGBA pixels + dimensions.
struct CachedText {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    texture: Option<TextureBuffer<GlesTexture>>,
}

/// GPU-side widget renderer.
///
/// Holds a [`fontdue::Font`] for text rasterisation and a cache of
/// previously rasterised strings so identical content is only
/// rasterised once per frame.
pub struct WidgetRenderer {
    font: Option<Font>,
    text_cache: HashMap<TextCacheKey, CachedText>,
}

impl WidgetRenderer {
    /// Create a new renderer, attempting to load a system font.
    pub fn new() -> Self {
        let font = Self::load_system_font();
        Self {
            font,
            text_cache: HashMap::new(),
        }
    }

    /// Try to load a sans-serif font from well-known Linux paths.
    fn load_system_font() -> Option<Font> {
        let candidates = [
            "/usr/share/fonts/OTF/SF-Pro-Display-Regular.otf",
            "/usr/share/fonts/OTF/SF-Pro-Text-Regular.otf",
            "/usr/share/fonts/TTF/SF-Pro.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation-mono/LiberationMono-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        ];

        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                match Font::from_bytes(data, fontdue::FontSettings::default()) {
                    Ok(font) => {
                        tracing::info!("WidgetRenderer: loaded font from {}", path);
                        return Some(font);
                    }
                    Err(e) => {
                        tracing::debug!("WidgetRenderer: failed to parse {}: {}", path, e);
                    }
                }
            }
        }
        tracing::warn!("WidgetRenderer: no system font found — text will not render");
        None
    }

    // ── Text rasterisation ──────────────────────────────────

    /// Rasterise `text` at `font_size` and return the raw RGBA pixel
    /// buffer, width, and height.
    fn rasterise_text(&self, text: &str, font_size: f32) -> Option<(Vec<u8>, i32, i32)> {
        let font = self.font.as_ref()?;
        let px_size = font_size.max(1.0);

        // Compute total advance width and max height.
        let mut cursor_x: u32 = 0;
        let mut total_w: u32 = 0;
        let mut max_h: u32 = 0;

        struct Glyph {
            x: u32,
            width: u32,
            height: u32,
            bitmap: Vec<u8>,
        }
        let mut glyphs: Vec<Glyph> = Vec::new();

        for ch in text.chars() {
            let (metrics, bitmap) = font.rasterize(ch, px_size);
            let w = metrics.width as u32;
            let h = metrics.height as u32;
            if w > 0 && h > 0 {
                glyphs.push(Glyph {
                    x: cursor_x,
                    width: w,
                    height: h,
                    bitmap,
                });
                total_w = total_w.max(cursor_x + w);
            }
            cursor_x += metrics.advance_width as u32;
            if h > max_h {
                max_h = h;
            }
        }

        let total_h = if max_h > 0 { max_h } else { px_size as u32 };

        if total_w == 0 || total_h == 0 {
            return None;
        }

        let mut rgba = vec![0u8; (total_w * total_h * 4) as usize];

        for g in &glyphs {
            for row in 0..g.height {
                for col in 0..g.width {
                    let alpha = g.bitmap[(row * g.width + col) as usize];
                    // Flip output buffer vertically: OpenGL's glTexImage2D
                    // stores row 0 as the bottom of the texture, but fontdue
                    // returns row 0 as the top of the glyph.
                    let out_row = total_h - 1 - row;
                    let px = ((out_row * total_w + g.x + col) * 4) as usize;
                    rgba[px] = 255;
                    rgba[px + 1] = 255;
                    rgba[px + 2] = 255;
                    rgba[px + 3] = alpha;
                }
            }
        }

        Some((rgba, total_w as i32, total_h as i32))
    }

    /// Produce an RGBA buffer tinted with `color` from a raw white-on-alpha
    /// rasterised buffer.
    fn tint_text(rgba: &[u8], color: Color) -> Vec<u8> {
        let c = color.to_premultiplied_rgba8();
        let mut out = Vec::with_capacity(rgba.len());
        for chunk in rgba.chunks_exact(4) {
            let alpha = chunk[3] as f32 / 255.0;
            out.push((c[0] as f32 * alpha) as u8);
            out.push((c[1] as f32 * alpha) as u8);
            out.push((c[2] as f32 * alpha) as u8);
            out.push((c[3] as f32 * alpha) as u8);
        }
        out
    }

    /// Render a text draw command into a [`TextureRenderElement`].
    fn render_text_cmd(
        &mut self,
        renderer: &mut GlesRenderer,
        content: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
    ) -> Option<TextureRenderElement<GlesTexture>> {
        if content.is_empty() {
            return None;
        }

        let key = TextCacheKey {
            content: content.to_string(),
            font_size: font_size as u32,
            r: (color.r * 255.0) as u8,
            g: (color.g * 255.0) as u8,
            b: (color.b * 255.0) as u8,
            a: (color.a * 255.0) as u8,
        };

        // Rasterise on miss, then insert and borrow.
        if !self.text_cache.contains_key(&key) {
            let (raw, w, h) = self
                .rasterise_text(content, font_size)
                .unwrap_or_else(|| (vec![0u8; 4], 1, 1));
            let tinted = Self::tint_text(&raw, color);
            self.text_cache.insert(
                key.clone(),
                CachedText {
                    pixels: tinted,
                    width: w,
                    height: h,
                    texture: None,
                },
            );
        }

        // Upload to GPU on first use, then reuse the cached texture.
        let cached = self.text_cache.get_mut(&key).unwrap();
        if cached.texture.is_none() {
            cached.texture = TextureBuffer::from_memory(
                renderer,
                &cached.pixels,
                Fourcc::Abgr8888,
                (cached.width, cached.height),
                false,
                1,
                Transform::Normal,
                None,
            )
            .ok();
        }

        let buffer = cached.texture.as_ref()?;
        let pos = Point::<f64, Physical>::from((x as f64, y as f64));
        let size = Size::<i32, Logical>::from((cached.width, cached.height));

        Some(TextureRenderElement::from_texture_buffer(
            pos,
            buffer,
            None,
            None,
            Some(size),
            Kind::Unspecified,
        ))
    }

    // ── Solid colour rect ───────────────────────────────────

    /// Render a 1×1 solid-colour texture, scaled to `size`.
    pub(crate) fn render_rect_cmd(
        renderer: &mut GlesRenderer,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    ) -> Option<TextureRenderElement<GlesTexture>> {
        let pixel = color.to_premultiplied_rgba8();
        let pos = Point::<f64, Physical>::from((x as f64, y as f64));
        let size = Size::<i32, Logical>::from((width as i32, height as i32));

        let buffer = TextureBuffer::from_memory(
            renderer,
            &pixel,
            Fourcc::Abgr8888,
            (1, 1),
            false,
            1,
            Transform::Normal,
            None,
        )
        .ok()?;

        Some(TextureRenderElement::from_texture_buffer(
            pos,
            &buffer,
            None,
            None,
            Some(size),
            Kind::Unspecified,
        ))
    }

    // ── Glass panel ─────────────────────────────────────────

    /// Render a semi-transparent glass-panel rectangle.
    pub(crate) fn render_glass_cmd(
        renderer: &mut GlesRenderer,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        milkiness: f32,
        alpha: f32,
    ) -> Option<TextureRenderElement<GlesTexture>> {
        let a = alpha;
        let m = milkiness;
        let pixel: [u8; 4] = [
            (m * a * 255.0) as u8,
            (m * a * 255.0) as u8,
            (m * a * 255.0) as u8,
            (a * 255.0) as u8,
        ];
        let pos = Point::<f64, Physical>::from((x as f64, y as f64));
        let size = Size::<i32, Logical>::from((width as i32, height as i32));

        let buffer = TextureBuffer::from_memory(
            renderer,
            &pixel,
            Fourcc::Abgr8888,
            (1, 1),
            false,
            1,
            Transform::Normal,
            None,
        )
        .ok()?;

        Some(TextureRenderElement::from_texture_buffer(
            pos,
            &buffer,
            None,
            None,
            Some(size),
            Kind::Unspecified,
        ))
    }
}

// ═══════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════

/// Convert a list of [`DrawCommand`]s into compositor render elements.
///
/// Each command is rasterised / uploaded to the GPU as needed.  The
/// returned vector can be wrapped in `TontooRenderElements::WidgetTexture(…)`
/// and merged into the compositor's per-frame element list.
///
/// **Note:** A fresh [`WidgetRenderer`] is created on each call, so the
/// text cache does not persist across frames.  For multi-frame use the
/// caller should store the renderer and pass it in.
pub fn render_draw_commands(
    renderer: &mut GlesRenderer,
    commands: &[DrawCommand],
) -> Vec<TextureRenderElement<GlesTexture>> {
    let mut widget_renderer = WidgetRenderer::new();
    render_draw_commands_with(renderer, commands, &mut widget_renderer)
}

/// Like [`render_draw_commands`] but reuses an existing
/// [`WidgetRenderer`] so its text cache persists across frames.
pub fn render_draw_commands_with(
    renderer: &mut GlesRenderer,
    commands: &[DrawCommand],
    widget_renderer: &mut WidgetRenderer,
) -> Vec<TextureRenderElement<GlesTexture>> {
    let mut elements = Vec::with_capacity(commands.len());

    for cmd in commands {
        match cmd {
            DrawCommand::Text {
                content,
                x,
                y,
                font_size,
                color,
                ..
            } => {
                if let Some(elem) =
                    widget_renderer.render_text_cmd(renderer, content, *x, *y, *font_size, *color)
                {
                    elements.push(elem);
                }
            }
            DrawCommand::Rect {
                x,
                y,
                width,
                height,
                color,
                ..
            } => {
                if let Some(elem) =
                    WidgetRenderer::render_rect_cmd(renderer, *x, *y, *width, *height, *color)
                {
                    elements.push(elem);
                }
            }
            DrawCommand::GlassPanel {
                x,
                y,
                width,
                height,
                milkiness,
                alpha,
                ..
            } => {
                if let Some(elem) = WidgetRenderer::render_glass_cmd(
                    renderer, *x, *y, *width, *height, *milkiness, *alpha,
                ) {
                    elements.push(elem);
                }
            }
            DrawCommand::Texture {
                texture_id,
                x,
                y,
                width,
                height,
            } => {
                // Placeholder: texture_id lookup will be wired up
                // when the texture-asset pipeline is ready.
                tracing::trace!(
                    "widget_renderer: Texture draw command id={} at ({}, {}) {}×{} (placeholder)",
                    texture_id,
                    x,
                    y,
                    width,
                    height
                );
            }
        }
    }

    elements
}
