// modules/clock.rs — HH:MM:SS (local time via libc, already a transitive dep)

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn local_hms() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;

    // SAFETY: localtime_r writes to the caller-provided `tm` buffer.
    // `now` is a valid timestamp; `tm` is zeroed stack memory.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm); }

    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

pub fn draw(cr: &cairo::Context, x: f64, bh: i32, _state: &AppState, dry_run: bool) -> f64 {
    super::simple_draw(cr, x, bh, config::FONT_SIZE_ICON, &local_hms(), dry_run)
}

pub const MODULE: Module = Module { draw, update: None };
