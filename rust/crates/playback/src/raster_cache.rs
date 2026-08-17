use std::collections::HashMap;

use cutix_project::model::ParamValues;
use stickers::Raster;

use crate::budget::{MemoryBudget, UNBOUNDED};

pub struct RasterCache {
    entries: HashMap<String, Entry>,
    bytes: usize,
    budget: usize,
    clock: u64,
    misses: u64,
    hits: u64,
    evictions: u64,
}

struct Entry {
    raster: Raster,
    bytes: usize,
    used: u64,
}

const LADDER_STEP: f64 = 1.125;
const LADDER_BASE: u32 = 16;

pub fn size_bucket(extent: u32) -> u32 {
    let extent = extent.max(1);
    if extent <= LADDER_BASE {
        return LADDER_BASE;
    }
    let mut rung = LADDER_BASE as f64;
    while (rung.round() as u32) < extent {
        rung *= LADDER_STEP;
    }
    rung.round() as u32
}

fn params_key(params: &ParamValues) -> String {
    serde_json::to_string(params).unwrap_or_default()
}

fn raster_bytes(raster: &Raster) -> usize {
    raster.rgba.len()
}

impl Default for RasterCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            budget: MemoryBudget::detect().rasters,
            clock: 0,
            misses: 0,
            hits: 0,
            evictions: 0,
        }
    }
}

impl RasterCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_budget(budget: usize) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
        self.enforce_limit(None);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn bucket(&self, extent: u32) -> u32 {
        if self.budget >= UNBOUNDED {
            extent
        } else {
            size_bucket(extent)
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn enforce_limit(&mut self, keep: Option<&str>) {
        while self.bytes > self.budget && self.entries.len() > 1 {
            let candidate = self
                .entries
                .iter()
                .filter(|(key, _)| Some(key.as_str()) != keep)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone());
            let Some(key) = candidate else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.evictions += 1;
            }
        }
    }

    fn store(&mut self, key: String, raster: Raster) {
        let bytes = raster_bytes(&raster);
        let used = self.tick();
        self.bytes += bytes;
        self.entries.insert(
            key.clone(),
            Entry {
                raster,
                bytes,
                used,
            },
        );
        self.enforce_limit(Some(&key));
    }

    fn touch(&mut self, key: &str) -> bool {
        let used = self.clock + 1;
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.used = used;
        self.clock = used;
        true
    }

    pub fn sticker(
        &mut self,
        sticker_id: &str,
        width: u32,
        height: u32,
    ) -> Result<&Raster, stickers::StickerError> {
        let width = self.bucket(width);
        let height = self.bucket(height);
        let key = format!("sticker|{sticker_id}|{width}x{height}");
        if self.touch(&key) {
            self.hits += 1;
        } else {
            let raster = stickers::rasterize_sticker(sticker_id, width, height)?;
            self.store(key.clone(), raster);
            self.misses += 1;
        }
        Ok(&self.entries.get(&key).expect("just inserted").raster)
    }

    pub fn graphic(
        &mut self,
        definition_id: &str,
        params: &ParamValues,
        width: u32,
        height: u32,
    ) -> Option<&Raster> {
        let width = self.bucket(width);
        let height = self.bucket(height);
        let key = format!(
            "graphic|{definition_id}|{}|{width}x{height}",
            params_key(params)
        );
        if self.touch(&key) {
            self.hits += 1;
        } else {
            let raster = stickers::render_graphic(definition_id, params, width, height)?;
            self.store(key.clone(), raster);
            self.misses += 1;
        }
        self.entries.get(&key).map(|entry| &entry.raster)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_lookups_hit_the_cache() {
        let mut cache = RasterCache::new();
        let first = cache.sticker("flags:JP", 60, 40).expect("flag").rgba.len();
        assert_eq!(cache.misses(), 1);
        let second = cache.sticker("flags:JP", 60, 40).expect("flag").rgba.len();
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(first, second);
    }

    #[test]
    fn a_different_size_is_a_different_entry() {
        let mut cache = RasterCache::new();
        cache.sticker("flags:JP", 60, 40).expect("flag");
        cache.sticker("flags:JP", 120, 80).expect("flag");
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 0);
    }

    #[test]
    fn changing_a_graphic_param_invalidates_the_entry() {
        let mut cache = RasterCache::new();
        let mut params = ParamValues::new();
        params.insert("fill".into(), serde_json::Value::String("#ff0000".into()));
        cache.graphic("rectangle", &params, 32, 32).expect("shape");
        params.insert("fill".into(), serde_json::Value::String("#00ff00".into()));
        cache.graphic("rectangle", &params, 32, 32).expect("shape");
        assert_eq!(cache.misses(), 2);
    }

    #[test]
    fn the_bucket_never_rasterises_smaller_than_asked() {
        for extent in [1u32, 17, 63, 100, 511, 1080, 1920, 4096] {
            assert!(size_bucket(extent) >= extent, "{extent}");
            assert!(
                size_bucket(extent) as f64 <= extent.max(16) as f64 * 1.13,
                "{extent}"
            );
        }
    }

    #[test]
    fn an_animated_scale_reuses_a_handful_of_entries() {
        let mut cache = RasterCache::new();
        for frame in 0..240 {
            let scale = 1.0 + frame as f64 * 0.004;
            let extent = (200.0 * scale) as u32;
            cache.sticker("flags:JP", extent, extent).expect("flag");
        }
        assert!(cache.misses() < 10, "misses {}", cache.misses());
        assert!(cache.hits() > 230, "hits {}", cache.hits());
    }

    #[test]
    fn the_cache_stays_under_its_ceiling() {
        let budget = 1024 * 1024;
        let mut cache = RasterCache::with_budget(budget);
        for index in 0..64 {
            let extent = 32 + index * 6;
            cache.sticker("flags:JP", extent, extent).expect("flag");
        }
        assert!(
            cache.resident_bytes() <= budget,
            "{}",
            cache.resident_bytes()
        );
        assert!(cache.evictions() > 0);
    }
}
