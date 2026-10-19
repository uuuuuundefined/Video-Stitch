/// macOS 使用内核物理内存计费口径，避免直接累加共享 RSS。
#[cfg(target_os = "macos")]
pub fn process_memory(pid: u32) -> Option<u64> {
    #[repr(C)]
    struct Usage {
        uuid: [u8; 16],
        fields: [u64; 10],
    }
    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut *mut std::ffi::c_void) -> i32;
    }
    let mut usage = Usage {
        uuid: [0; 16],
        fields: [0; 10],
    };
    // RUSAGE_INFO_V0: ri_phys_footprint is the eighth u64 after the UUID.
    let result = unsafe {
        proc_pid_rusage(
            i32::try_from(pid).ok()?,
            0,
            (&mut usage as *mut Usage).cast(),
        )
    };
    (result == 0).then_some(usage.fields[7])
}

/// Windows 使用进程私有提交内存，无法读取时不伪造数值。
#[cfg(windows)]
pub fn process_memory(pid: u32) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
        },
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut counters = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS_EX>::zeroed();
    let result = unsafe {
        GetProcessMemoryInfo(
            process,
            counters.as_mut_ptr().cast(),
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
    };
    unsafe { CloseHandle(process) };
    if result == 0 {
        None
    } else {
        Some(unsafe { counters.assume_init() }.PrivateUsage as u64)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn process_memory(_pid: u32) -> Option<u64> {
    None
}
