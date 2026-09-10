use std::collections::HashMap;

use smithay::backend::renderer::gles::GlesTexture;

use crate::config::ColorScheme;

pub type TexBuf = smithay::backend::renderer::element::texture::TextureBuffer<GlesTexture>;

pub struct RenderCache {
    pub window_shadows: HashMap<(i32, i32, ColorScheme), TexBuf>,
    pub window_borders: HashMap<(i32, i32, ColorScheme), TexBuf>,
    /// Cached titlebar textures: (width, height, ColorScheme) -> TexBuf
    pub window_titlebars: HashMap<(i32, i32, ColorScheme), TexBuf>,
    /// Cached traffic light dot textures: (color_name, size, scale, ColorScheme) -> TexBuf
    pub traffic_light_dots: HashMap<(String, i32, i32, ColorScheme), TexBuf>,
    /// Cached traffic light symbol textures: (symbol_char, size, ColorScheme) -> TexBuf
    pub traffic_light_symbols: HashMap<(char, i32, ColorScheme), TexBuf>,
    pub font: Option<fontdue::Font>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            window_shadows: HashMap::new(),
            window_borders: HashMap::new(),
            window_titlebars: HashMap::new(),
            traffic_light_dots: HashMap::new(),
            traffic_light_symbols: HashMap::new(),
            font: None,
        }
    }

    pub fn load_font(&mut self) -> Option<&fontdue::Font> {
        if self.font.is_none() {
            let candidates = [
                "/usr/share/fonts/OTF/SF-Pro-Display-Regular.otf",
                "/usr/share/fonts/OTF/SF-Pro-Text-Regular.otf",
                "/usr/share/fonts/TTF/SF-Pro.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ];
            self.font = candidates.iter().find_map(|p| {
                let data = std::fs::read(p).ok()?;
                fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).ok()
            });
        }
        self.font.as_ref()
    }

    pub fn invalidate(&mut self) {
        self.window_shadows.clear();
        self.window_borders.clear();
        self.window_titlebars.clear();
        self.traffic_light_dots.clear();
        self.traffic_light_symbols.clear();
    }
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}
