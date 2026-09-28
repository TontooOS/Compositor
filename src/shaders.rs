//! Glass blur shaders for TontooOS compositor.
//!
//! Provides Gaussian blur effects for layer surfaces (topbar, etc.).

/// Vertex shader — simple fullscreen quad pass.
const BLUR_VERTEX_SHADER: &str = r#"#version 300 es
layout(location = 0) in vec2 position;
layout(location = 1) in vec2 tex_coords;
out vec2 v_tex_coords;
void main() {
    v_tex_coords = tex_coords;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#;

/// Fragment shader — two-pass separable Gaussian blur.
/// When `horizontal` is true, blur is applied horizontally; otherwise vertically.
const BLUR_FRAGMENT_SHADER: &str = r#"#version 300 es
precision mediump float;
in vec2 v_tex_coords;
out vec4 frag_color;
uniform sampler2D u_texture;
uniform vec2 u_texel_size; // 1.0 / texture_size
uniform bool u_horizontal;
uniform float u_sigma;

const int MAX_RADIUS = 64;

float gaussian(float x, float sigma) {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

void main() {
    int radius = int(ceil(u_sigma * 2.0));
    if (radius > MAX_RADIUS) radius = MAX_RADIUS;

    vec4 result = vec4(0.0);
    float weight_sum = 0.0;

    for (int i = -MAX_RADIUS; i <= MAX_RADIUS; i++) {
        if (abs(i) > radius) continue;
        float w = gaussian(float(i), u_sigma);
        vec2 offset;
        if (u_horizontal) {
            offset = vec2(float(i) * u_texel_size.x, 0.0);
        } else {
            offset = vec2(0.0, float(i) * u_texel_size.y);
        }
        result += texture(u_texture, v_tex_coords + offset) * w;
        weight_sum += w;
    }

    frag_color = result / weight_sum;
}
"#;

/// A simple blur pass that reads from `src_texture`, blurs to an intermediate FBO,
/// then blurs again to the destination.
///
/// This is a placeholder showing the shader approach. The actual GL calls need
/// to be integrated into smithay's GlesRenderer frame loop.
pub struct BlurPass {
    pub sigma: f32,
    pub enabled: bool,
}

impl Default for BlurPass {
    fn default() -> Self {
        Self {
            sigma: 18.0,
            enabled: true,
        }
    }
}

impl BlurPass {
    pub fn new(sigma: f32) -> Self {
        Self {
            sigma,
            enabled: true,
        }
    }

    pub fn vertex_shader(&self) -> &str {
        BLUR_VERTEX_SHADER
    }

    pub fn fragment_shader(&self) -> &str {
        BLUR_FRAGMENT_SHADER
    }
}
