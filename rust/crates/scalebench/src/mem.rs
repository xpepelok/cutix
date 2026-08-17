#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool: usize,
    quota_paged_pool: usize,
    quota_peak_nonpaged_pool: usize,
    quota_nonpaged_pool: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(windows)]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(
        process: isize,
        counters: *mut ProcessMemoryCounters,
        size: u32,
    ) -> i32;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub working_set: usize,
    pub private: usize,
    pub peak_working_set: usize,
}

#[cfg(windows)]
pub fn snapshot() -> Snapshot {
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        ..ProcessMemoryCounters::default()
    };
    let size = counters.cb;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
    if ok == 0 {
        return Snapshot::default();
    }
    Snapshot {
        working_set: counters.working_set_size,
        private: counters.pagefile_usage,
        peak_working_set: counters.peak_working_set_size,
    }
}

#[cfg(not(windows))]
pub fn snapshot() -> Snapshot {
    Snapshot::default()
}

pub fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

pub fn report(label: &str) {
    let snapshot = snapshot();
    println!(
        "  [mem] {label:<28} working_set {:>8.1} MB  private {:>8.1} MB",
        mb(snapshot.working_set),
        mb(snapshot.private)
    );
}
