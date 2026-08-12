use std::path::PathBuf;

pub const TITLEBAR_HEIGHT: i32 = 32;

const CONFIG_DIR: &str = "tontoo";
const CONFIG_FILE: &str = "theme.conf";
const KEY_COLOR_SCHEME: &str = "color-scheme";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    Dark,
    Light,
}

impl Default for ColorScheme {
    fn default() -> Self {
        ColorScheme::Dark
    }
}

impl ColorScheme {
    pub fn as_env_str(&self) -> &'static str {
        match self {
            ColorScheme::Dark => "dark",
            ColorScheme::Light => "light",
        }
    }

    pub fn gtk_theme_name(&self) -> &'static str {
        match self {
            ColorScheme::Dark => "MacTahoe-Dark-blue",
            ColorScheme::Light => "MacTahoe-Light-blue",
        }
    }

    pub fn prefers_color_scheme(&self) -> &'static str {
        match self {
            ColorScheme::Dark => "prefer-dark",
            ColorScheme::Light => "prefer-light",
        }
    }

    pub fn clear_color(&self) -> [f32; 4] {
        match self {
            ColorScheme::Dark => [0.11, 0.11, 0.11, 1.0],
            ColorScheme::Light => [0.93, 0.93, 0.93, 1.0],
        }
    }

    pub fn cursor_theme_name(&self) -> &'static str {
        match self {
            ColorScheme::Dark => "MacTahoe-dark-cursors",
            ColorScheme::Light => "MacTahoe-cursors",
        }
    }
}

fn config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_DIR);
    config_dir.join(CONFIG_FILE)
}

pub fn load_color_scheme() -> ColorScheme {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => parse_config(&contents),
        Err(_) => {
            let scheme = ColorScheme::default();
            tracing::info!(
                "No theme config found at {:?}, using default: {:?}",
                path,
                scheme
            );
            scheme
        }
    }
}

pub fn save_color_scheme(scheme: ColorScheme) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = format!("{}={}\n", KEY_COLOR_SCHEME, scheme.as_env_str());
    std::fs::write(&path, &contents)?;
    tracing::info!("Saved color scheme {:?} to {:?}", scheme, path);
    Ok(())
}

fn parse_config(contents: &str) -> ColorScheme {
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == KEY_COLOR_SCHEME {
                match value.trim() {
                    "dark" => return ColorScheme::Dark,
                    "light" => return ColorScheme::Light,
                    other => {
                        tracing::warn!("Unknown color-scheme value: '{}', using default", other);
                    }
                }
            }
        }
    }
    ColorScheme::default()
}

pub fn apply_color_scheme_env(scheme: ColorScheme) {
    unsafe {
        std::env::set_var("TONTOO_COLOR_SCHEME", scheme.as_env_str());
        std::env::set_var("GTK_THEME", scheme.gtk_theme_name());
        std::env::set_var("COLOR_SCHEME", scheme.prefers_color_scheme());
        std::env::set_var("XCURSOR_THEME", scheme.cursor_theme_name());
        std::env::set_var("XCURSOR_SIZE", "24");
        std::env::set_var("TERMINAL", "foot");
    }
}
