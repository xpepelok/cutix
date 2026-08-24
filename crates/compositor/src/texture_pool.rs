use std::collections::HashMap;

use gpu::{GpuContext, wgpu};

type TextureKey = (u32, u32);

pub struct TexturePool {
    available: HashMap<TextureKey, Vec<wgpu::Texture>>,
    in_use: Vec<(TextureKey, wgpu::Texture)>,
    idle_bytes: usize,
    budget: usize,
    reuses: u64,
    allocations: u64,
    drops: u64,
}

const DEFAULT_BUDGET: usize = 192 * 1024 * 1024;

const UNBOUNDED: usize = usize::MAX / 8;

impl Default for TexturePool {
    fn default() -> Self {
        Self {
            available: HashMap::new(),
            in_use: Vec::new(),
            idle_bytes: 0,
            budget: DEFAULT_BUDGET,
            reuses: 0,
            allocations: 0,
            drops: 0,
        }
    }
}

fn key_bytes(key: TextureKey) -> usize {
    key.0 as usize * key.1 as usize * 4
}

impl TexturePool {
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
        self.trim();
    }

    pub fn stats(&self) -> (usize, u64, u64, u64) {
        (self.idle_bytes, self.reuses, self.allocations, self.drops)
    }

    pub fn recycle_frame(&mut self) {
        for (key, texture) in self.in_use.drain(..) {
            self.idle_bytes += key_bytes(key);
            self.available.entry(key).or_default().push(texture);
        }
        self.trim();
    }

    fn trim(&mut self) {
        if self.budget >= UNBOUNDED {
            return;
        }
        while self.idle_bytes > self.budget {
            let widest = self
                .available
                .iter()
                .filter(|(_, textures)| !textures.is_empty())
                .max_by_key(|(key, textures)| (key_bytes(**key), textures.len()))
                .map(|(key, _)| *key);
            let Some(key) = widest else {
                break;
            };
            let Some(textures) = self.available.get_mut(&key) else {
                break;
            };
            if textures.pop().is_none() {
                break;
            }
            self.idle_bytes = self.idle_bytes.saturating_sub(key_bytes(key));
            self.drops += 1;
        }
    }

    pub fn acquire(
        &mut self,
        context: &GpuContext,
        width: u32,
        height: u32,
        label: &'static str,
    ) -> wgpu::Texture {
        let key = (width, height);
        let texture = match self.available.get_mut(&key).and_then(Vec::pop) {
            Some(texture) => {
                self.idle_bytes = self.idle_bytes.saturating_sub(key_bytes(key));
                self.reuses += 1;
                texture
            }
            None => {
                self.allocations += 1;
                context.create_render_texture(width, height, label)
            }
        };
        self.in_use.push((key, texture.clone()));
        texture
    }
}
