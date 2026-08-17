use std::collections::HashMap;

use gpu::wgpu;

pub struct StoredTexture {
    texture: wgpu::Texture,
}

impl StoredTexture {
    pub fn new(texture: wgpu::Texture) -> Self {
        Self { texture }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

struct Entry {
    stored: StoredTexture,
    bytes: usize,
    used: u64,
}

pub struct TextureStore {
    textures: HashMap<String, Entry>,
    generation: u64,
    bytes: usize,
    budget: usize,
    evictions: u64,
}

const RETAIN_GENERATIONS: u64 = 2;

const DEFAULT_BUDGET: usize = 384 * 1024 * 1024;

const UNBOUNDED: usize = usize::MAX / 8;

impl Default for TextureStore {
    fn default() -> Self {
        Self {
            textures: HashMap::new(),
            generation: 0,
            bytes: 0,
            budget: DEFAULT_BUDGET,
            evictions: 0,
        }
    }
}

fn texture_bytes(texture: &wgpu::Texture) -> usize {
    let size = texture.size();
    let block = texture.format().block_copy_size(None).unwrap_or(4) as usize;
    size.width as usize * size.height as usize * size.depth_or_array_layers as usize * block
}

impl TextureStore {
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
        self.enforce_limit();
    }

    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    pub fn begin_frame(&mut self) {
        self.generation += 1;
        if self.budget >= UNBOUNDED {
            return;
        }
        let horizon = self.generation.saturating_sub(RETAIN_GENERATIONS);
        let stale: Vec<String> = self
            .textures
            .iter()
            .filter(|(_, entry)| entry.used < horizon)
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            self.drop_entry(&id);
        }
        self.enforce_limit();
    }

    pub fn upsert(&mut self, id: String, texture: wgpu::Texture) {
        let bytes = texture_bytes(&texture);
        if let Some(previous) = self.textures.remove(&id) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        self.bytes += bytes;
        self.textures.insert(
            id,
            Entry {
                stored: StoredTexture::new(texture),
                bytes,
                used: self.generation,
            },
        );
        self.enforce_limit();
    }

    pub fn get(&self, id: &str) -> Option<&StoredTexture> {
        self.textures.get(id).map(|entry| &entry.stored)
    }

    pub fn remove(&mut self, id: &str) {
        self.drop_entry(id);
    }

    pub fn clear(&mut self) {
        self.textures.clear();
        self.bytes = 0;
    }

    fn drop_entry(&mut self, id: &str) {
        if let Some(entry) = self.textures.remove(id) {
            self.bytes = self.bytes.saturating_sub(entry.bytes);
        }
    }

    fn enforce_limit(&mut self) {
        while self.bytes > self.budget {
            let candidate = self
                .textures
                .iter()
                .filter(|(_, entry)| entry.used != self.generation)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(id, _)| id.clone());
            let Some(id) = candidate else {
                break;
            };
            self.drop_entry(&id);
            self.evictions += 1;
        }
    }
}
