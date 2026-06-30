// util.rs — small helpers available everywhere

pub fn read_int(path: &str) -> Option<i32> {
    std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
}

/// Read PulseAudio volume and mute state via FFI.
///
/// # Safety
///
/// Must only be called from the main thread — the underlying C code uses
/// global mutable state (`static pa_mainloop`, `pa_context`) without
/// synchronisation.
pub unsafe fn pulse_read_volume(vol: *mut i32, muted: *mut i32) {
    unsafe extern "C" {
        fn read_volume_both(vol: *mut i32, muted: *mut i32);
    }
    unsafe {
        read_volume_both(vol, muted);
    }
}

/// Free PulseAudio resources (mainloop + context).
///
/// Called once at shutdown so that Valgrind / ASan runs report zero leaks.
pub fn pulse_cleanup() {
    unsafe extern "C" {
        fn pulse_cleanup();
    }
    unsafe { pulse_cleanup(); }
}
