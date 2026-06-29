//! Process memory probe. Used for the post-transcription RSS log (so the 2-hour
//! "heavy use" crash leaves a memory trend in the logs) and by the load-test
//! harness in `loadtest.rs`.

/// Current process resident memory in bytes (Windows working set). Returns 0 if
/// the query fails, or on non-Windows targets.
#[cfg(windows)]
pub fn current_rss_bytes() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 {
            counters.WorkingSetSize as u64
        } else {
            0
        }
    }
}

#[cfg(not(windows))]
pub fn current_rss_bytes() -> u64 {
    0
}

/// Resident memory in MB (convenience for logging).
pub fn rss_mb() -> f64 {
    current_rss_bytes() as f64 / (1024.0 * 1024.0)
}
