# Shaders

The shaders module defines GLSL shaders for a two-pass separable Gaussian
blur that can be applied to layer surfaces (topbar, dock, and similar). The
shaders are defined as constants and exposed through the `BlurPass` struct.

> **Note:** `BlurPass` is a placeholder showing the shader approach. The
> actual GL calls are not yet integrated into smithay's `GlesRenderer`
> frame loop. The software box-blur used by the render pipeline in
> [Rendering.md](Rendering.md) is the currently active implementation.

## BlurPass

```rust
pub struct BlurPass {
    pub sigma: f32,
    pub enabled: bool,
}
```

Default sigma is 18.0 and the pass is enabled.

### BlurPass::new

```rust
pub fn new(sigma: f32) -> Self
```

Creates a blur pass with the given sigma and `enabled = true`.

### BlurPass::vertex_shader

```rust
pub fn vertex_shader(&self) -> &str
```

Returns the vertex shader source: a simple fullscreen quad pass.

### BlurPass::fragment_shader

```rust
pub fn fragment_shader(&self) -> &str
```

Returns the fragment shader source: a two-pass separable Gaussian blur.

## Vertex Shader

`#version 300 es` fullscreen quad. Inputs are `position` and `tex_coords`;
output is `v_tex_coords`.

## Fragment Shader

`#version 300 es` separable Gaussian blur.

Uniforms:

| Uniform | Type | Description |
|---|---|---|
| `u_texture` | `sampler2D` | The source texture |
| `u_texel_size` | `vec2` | `1.0 / texture_size` |
| `u_horizontal` | `bool` | Blur horizontally when true, vertically when false |
| `u_sigma` | `float` | Gaussian standard deviation |

Constants:

| Constant | Value |
|---|---|
| `MAX_RADIUS` | 64 |

The blur radius is `ceil(sigma * 2.0)` clamped to `MAX_RADIUS`. When
`u_horizontal` is true, the pass samples `texture(u_texture, v_tex_coords +
float(i) * u_texel_size.x)`; otherwise it offsets on the y axis.

## Cross References

- [Rendering.md](Rendering.md) -- the software blur currently used for
  glass panels
- [TontooUiProtocol.md](TontooUiProtocol.md) -- `GlassConfig.sigma` is the
  intended consumer of this blur
