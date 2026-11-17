//! Resident memory of this process, without extra dependencies.

/// Bytes of RAM in use by the process (`None` where it cannot be read).
pub fn resident_bytes() -> Option<u64> {
    imp()
}

#[cfg(windows)]
fn imp() -> Option<u64> {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_non_paged: usize,
        quota_non_paged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    // SAFETY: plain kernel32 calls with a correctly sized, zero-initialised out struct.
    unsafe {
        let mut c: Counters = std::mem::zeroed();
        c.cb = std::mem::size_of::<Counters>() as u32;
        (K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) != 0).then_some(c.working_set as u64)
    }
}

#[cfg(target_os = "linux")]
fn imp() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn imp() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_some_memory_here() {
        if cfg!(any(windows, target_os = "linux")) {
            assert!(super::resident_bytes().unwrap() > 1_000_000);
        }
    }
}
