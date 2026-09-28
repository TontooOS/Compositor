use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::{
    gles::{GlesRenderer, GlesTexture},
    ImportMem,
};

/// A hash-based key for identifying texture content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureKey(u64);

/// A pending texture upload waiting to be processed on the GPU thread.
struct PendingUpload {
    key: TextureKey,
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    fourcc: Fourcc,
}

/// GPU texture cache to avoid re-uploading identical pixel data every frame.
pub struct TextureCache {
    textures: HashMap<TextureKey, GlesTexture>,
    pending_uploads: Vec<PendingUpload>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            pending_uploads: Vec::new(),
        }
    }

    /// Compute a cache key from pixel data and dimensions.
    pub fn compute_key(pixels: &[u8], width: i32, height: i32) -> TextureKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // Hash dimensions first (fast, distinguishes most wallpapers from cursors)
        (width as u64).hash(&mut hasher);
        (height as u64).hash(&mut hasher);
        // For large textures, sample rather than hashing every byte
        let len = pixels.len();
        len.hash(&mut hasher);
        if len <= 4096 {
            pixels.hash(&mut hasher);
        } else {
            let step = len / 128;
            for i in (0..len).step_by(step.max(1)) {
                pixels[i].hash(&mut hasher);
            }
        }
        TextureKey(hasher.finish())
    }

    /// Retrieve a cached texture.
    pub fn get(&self, key: TextureKey) -> Option<&GlesTexture> {
        self.textures.get(&key)
    }

    /// Insert a texture into the cache.
    pub fn insert(&mut self, key: TextureKey, texture: GlesTexture) {
        self.textures.insert(key, texture);
    }

    /// Queue a texture for upload on the next `process_uploads` call.
    pub fn request_upload(
        &mut self,
        key: TextureKey,
        pixels: Vec<u8>,
        width: i32,
        height: i32,
        fourcc: Fourcc,
    ) {
        self.pending_uploads.push(PendingUpload {
            key,
            pixels,
            width,
            height,
            fourcc,
        });
    }

    /// Upload all pending textures and return the newly created textures.
    pub fn process_uploads(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Vec<(TextureKey, GlesTexture)> {
        let pending: Vec<PendingUpload> = self.pending_uploads.drain(..).collect();
        let mut results = Vec::new();

        for upload in pending {
            if self.textures.contains_key(&upload.key) {
                continue;
            }

            match renderer.import_memory(
                &upload.pixels,
                upload.fourcc,
                (upload.width, upload.height).into(),
                false,
            ) {
                Ok(texture) => {
                    self.textures.insert(upload.key, texture.clone());
                    results.push((upload.key, texture));
                }
                Err(e) => {
                    tracing::warn!("Failed to upload texture to GPU: {:?}", e);
                }
            }
        }

        results
    }

    /// Remove all cached textures and pending uploads.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.pending_uploads.clear();
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}
