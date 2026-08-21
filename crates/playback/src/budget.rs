const MB: usize = 1024 * 1024;

const TOTAL_FRACTION: f64 = 0.10;
const TOTAL_FLOOR: usize = 384 * MB;
const TOTAL_CEILING: usize = 1536 * MB;
const ASSUMED_PHYSICAL: usize = 8 * 1024 * MB;

pub const DECODER_FOOTPRINT: usize = 36 * MB;

pub const UNBOUNDED: usize = usize::MAX / 8;

pub const MIN_DECODERS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryBudget {
    pub decoders: usize,
    pub stills: usize,
    pub rasters: usize,
    pub layer_textures: usize,
    pub render_pool: usize,
}

impl MemoryBudget {
    pub fn from_total_bytes(total: usize) -> Self {
        if total >= UNBOUNDED {
            return Self {
                decoders: UNBOUNDED,
                layer_textures: UNBOUNDED,
                stills: UNBOUNDED,
                rasters: UNBOUNDED,
                render_pool: UNBOUNDED,
            };
        }
        Self {
            decoders: total * 9 / 20,
            layer_textures: total / 4,
            render_pool: total / 5,
            stills: total / 20,
            rasters: total / 20,
        }
    }

    pub fn detect() -> Self {
        Self::from_total_bytes(cache_allowance())
    }

    pub fn total(&self) -> usize {
        self.decoders
            .saturating_add(self.stills)
            .saturating_add(self.rasters)
            .saturating_add(self.layer_textures)
            .saturating_add(self.render_pool)
    }

    pub fn max_decoders(&self) -> usize {
        if self.decoders >= UNBOUNDED {
            return usize::MAX;
        }
        (self.decoders / DECODER_FOOTPRINT).max(MIN_DECODERS)
    }
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self::detect()
    }
}

pub fn cache_allowance() -> usize {
    if let Some(override_bytes) = allowance_override() {
        return override_bytes;
    }
    let total = physical_memory().unwrap_or(ASSUMED_PHYSICAL);
    ((total as f64 * TOTAL_FRACTION) as usize).clamp(TOTAL_FLOOR, TOTAL_CEILING)
}

#[cfg(not(target_arch = "wasm32"))]
fn allowance_override() -> Option<usize> {
    let raw = std::env::var("CUTIX_CACHE_BUDGET_MB").ok()?;
    let megabytes: usize = raw.trim().parse().ok()?;
    if megabytes == 0 {
        return Some(UNBOUNDED);
    }
    Some(megabytes * MB)
}

#[cfg(target_arch = "wasm32")]
fn allowance_override() -> Option<usize> {
    None
}

#[cfg(windows)]
#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_physical: u64,
    available_physical: u64,
    total_page_file: u64,
    available_page_file: u64,
    total_virtual: u64,
    available_virtual: u64,
    available_extended_virtual: u64,
}

#[cfg(windows)]
// The signature is checked against the Win32 headers by hand; nothing verifies it
// for us, which is what declaring the block unsafe acknowledges.
unsafe extern "system" {
    fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
}

#[cfg(windows)]
pub fn physical_memory() -> Option<usize> {
    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_physical: 0,
        available_physical: 0,
        total_page_file: 0,
        available_page_file: 0,
        total_virtual: 0,
        available_virtual: 0,
        available_extended_virtual: 0,
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 || status.total_physical == 0 {
        return None;
    }
    Some(status.total_physical as usize)
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
pub fn physical_memory() -> Option<usize> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kilobytes: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * 1024)
}

#[cfg(not(any(windows, all(unix, not(target_arch = "wasm32")))))]
pub fn physical_memory() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowance_stays_inside_its_clamp() {
        let allowance = cache_allowance();
        assert!(allowance >= TOTAL_FLOOR);
        assert!(allowance <= TOTAL_CEILING);
    }

    #[test]
    fn a_tiny_budget_still_keeps_enough_decoders_for_one_frame() {
        let budget = MemoryBudget::from_total_bytes(16 * MB);
        assert_eq!(budget.max_decoders(), MIN_DECODERS);
    }

    #[test]
    fn the_split_never_exceeds_the_allowance() {
        let budget = MemoryBudget::from_total_bytes(1000 * MB);
        assert!(budget.total() <= 1000 * MB);
    }
}
