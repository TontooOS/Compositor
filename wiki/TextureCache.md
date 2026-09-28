# TextureCache

The texture cache avoids re-uploading identical pixel data to the GPU every
frame by keying cached textures on a content hash of the pixel data and
dimensions.

## TextureKey

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureKey(u64);
```

A hash-based key for identifying texture content. Uses Rust's
`DefaultHasher`.

## TextureCache

```rust
pub struct TextureCache {
    textures: HashMap<TextureKey, GlesTexture>,
    pending_uploads: Vec<PendingUpload>,
}
```

### TextureCache::new

```rust
pub fn new() -> Self
```

Creates an empty cache.

### TextureCache::compute_key

```rust
pub fn compute_key(pixels: &[u8], width: i32, height: i32) -> TextureKey
```

Computes a cache key from pixel data and dimensions:

1. Hash the dimensions.
2. Hash the pixel length.
3. For pixel buffers of 4096 bytes or fewer, hash all bytes.
4. For larger buffers, sample every `len / 128` bytes to avoid hashing the
   entire buffer.

### TextureCache::get

```rust
pub fn get(&self, key: TextureKey) -> Option<&GlesTexture>
```

Returns the cached texture for a key, or `None`.

### TextureCache::insert

```rust
pub fn insert(&mut self, key: TextureKey, texture: GlesTexture)
```

Inserts a texture into the cache.

### TextureCache::request_upload

```rust
pub fn request_upload(
    &mut self,
    key: TextureKey,
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    fourcc: Fourcc,
)
```

Queues a texture upload to be processed on the next `process_uploads` call.

### TextureCache::process_uploads

```rust
pub fn process_uploads(
    &mut self,
    renderer: &mut GlesRenderer,
) -> Vec<(TextureKey, GlesTexture)>
```

Uploads all pending textures to the GPU, skipping keys that are already
cached. Returns the newly created textures. Failed uploads log a warning
and are skipped.

### TextureCache::clear

```rust
pub fn clear(&mut self)
```

Removes all cached textures and pending uploads.

## PendingUpload

```rust
struct PendingUpload {
    key: TextureKey,
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    fourcc: Fourcc,
}
```

## Cross References

- [State.md](State.md) -- `TontooCompositor::texture_cache` field
- [Rendering.md](Rendering.md) -- textures are uploaded during rendering
