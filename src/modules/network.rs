// modules/network.rs — WiFi icon + download speed
//
// All its logic is here — nothing in mod.rs.

use super::*;
use std::path::Path;
use std::time::Instant;

#[derive(Default)]
pub struct NetState {
    prev_rx: Option<i64>,
    prev_instant: Option<Instant>,
    pub speed: Option<f64>,
}

impl NetState {
    fn read(&mut self) {
        let s = match std::fs::read_to_string("/sys/class/net/wlan0/statistics/rx_bytes") {
            Ok(s) => s,
            Err(_) => return,
        };
        let rx: i64 = match s.trim().parse() {
            Ok(v) => v,
            Err(_) => return,
        };
        let now = Instant::now();
        if let (Some(prev_rx), Some(prev_instant)) = (self.prev_rx, self.prev_instant) {
            let dt = now.duration_since(prev_instant).as_secs_f64();
            if dt > 0.0 {
                let speed = (rx - prev_rx) as f64 / dt;
                self.prev_rx = Some(rx);
                self.prev_instant = Some(now);
                self.speed = Some(if speed > 0.0 { speed } else { 0.0 });
                return;
            }
        }
        self.prev_rx = Some(rx);
        self.prev_instant = Some(now);
    }
}

fn network_up() -> bool {
    let net_dir = Path::new("/sys/class/net");
    if let Ok(dir) = std::fs::read_dir(net_dir) {
        dir.flatten().any(|entry| {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || name_str == "lo" {
                return false;
            }
            let state_path = format!("/sys/class/net/{}/operstate", name_str);
            std::fs::read_to_string(state_path).map(|s| s.trim() == "up").unwrap_or(false)
        })
    } else {
        false
    }
}

pub fn update(state: &mut AppState) {
    state.net.read();
}

pub fn draw(cr: &cairo::Context, x: f64, bh: i32, state: &AppState, dry_run: bool) -> f64 {
    let text = match (network_up(), state.net.speed) {
        (true, Some(s)) if s >= 0.0 => {
            if s < 1024.0 {
                format!("{} {:.0}B/s", ICON_WIFI, s)
            } else if s < 1024.0 * 1024.0 {
                format!("{} {:.0}K/s", ICON_WIFI, s / 1024.0)
            } else {
                format!("{} {:.1}M/s", ICON_WIFI, s / (1024.0 * 1024.0))
            }
        }
        _ => "\u{2717}".to_string(),
    };
    super::simple_draw(cr, x, bh, config::FONT_SIZE_ICON, &text, dry_run)
}

pub const MODULE: Module = Module { draw, update: Some(update) };
