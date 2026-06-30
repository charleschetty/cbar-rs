// workspace/i3.rs — i3 / sway IPC + workspace button drawing

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use serde::Deserialize;

use super::super::*;

// ── types ──

const I3_MAGIC: &[u8; 6] = b"i3-ipc";

#[repr(u32)]
#[allow(dead_code)]
enum MsgType {
    RunCommand = 0,
    GetWorkspaces = 1,
    Subscribe = 2,
}

#[derive(Deserialize, Debug)]
pub struct Workspace {
    pub name: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub visible: bool,
    #[serde(default)]
    pub urgent: bool,
}

// ── IPC init / thread ──

pub fn init(state: &mut AppState) -> Option<super::WsHandle> {
    let sp = find_socket()?;
    let mut stream = UnixStream::connect(sp).ok()?;

    send(&mut stream, MsgType::Subscribe, Some(r#"["workspace"]"#)).ok()?;
    send(&mut stream, MsgType::GetWorkspaces, None).ok()?;
    if let Ok((_, reply)) = recv(&mut stream) {
        state.i3workspace = Some(serde_json::from_str(&reply).unwrap_or_default());
    }

    let (tx, rx) = mpsc::channel();

    // The IPC thread runs until the process exits — the OS reclaims it.
    // A read timeout would interfere with Ctrl+C responsiveness.
    std::thread::spawn(move || {
        while let Ok((_, _body)) = recv(&mut stream) {
            let _ = send(&mut stream, MsgType::GetWorkspaces, None);
            if let Ok((_, reply)) = recv(&mut stream) {
                let ws: Vec<Workspace> = serde_json::from_str(&reply).unwrap_or_default();
                if tx.send(ws).is_err() {
                    break;
                }
            }
        }
    });

    Some(super::WsHandle { rx })
}

fn find_socket() -> Option<String> {
    if let Ok(env) = std::env::var("I3SOCK") {
        return Some(env);
    }
    if let Ok(env) = std::env::var("SWAYSOCK") {
        return Some(env);
    }
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        let p = format!("{}/i3", rt);
        if let Ok(dir) = fs::read_dir(&p) {
            for entry in dir.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with("ipc-socket.") {
                    return Some(format!("{}/i3/{}", rt, name.to_string_lossy()));
                }
            }
        }
    }
    None
}

fn send(stream: &mut UnixStream, msg_type: MsgType, payload: Option<&str>) -> std::io::Result<()> {
    let len = payload.map_or(0, |s| s.len() as u32);
    let type_val = msg_type as u32;
    let mut hdr = [0u8; 14];
    hdr[..6].copy_from_slice(I3_MAGIC);
    hdr[6..10].copy_from_slice(&len.to_le_bytes());
    hdr[10..14].copy_from_slice(&type_val.to_le_bytes());
    stream.write_all(&hdr)?;
    if let Some(p) = payload
        && !p.is_empty()
    {
        stream.write_all(p.as_bytes())?;
    }
    Ok(())
}

fn recv(stream: &mut UnixStream) -> std::io::Result<(u32, String)> {
    let mut hdr = [0u8; 14];
    stream.read_exact(&mut hdr)?;
    if hdr[..6] != *I3_MAGIC {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid i3-ipc magic bytes"));
    }
    let len = u32::from_le_bytes([hdr[6], hdr[7], hdr[8], hdr[9]]);
    let type_val = u32::from_le_bytes([hdr[10], hdr[11], hdr[12], hdr[13]]);
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf)?;
    String::from_utf8(buf)
        .map(|s| (type_val, s))
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8"))
}

// ── draw ──

pub fn draw(cr: &cairo::Context, x: f64, bh: i32, state: &AppState, dry_run: bool) -> f64 {
    let ws = match &state.i3workspace {
        Some(w) => w,
        None => return 0.0,
    };
    let pad = config::WS_PAD;
    let gap = config::WS_GAP;
    cr.set_font_size(config::FONT_SIZE_MAIN);
    let fe = match cr.font_extents() {
        Ok(f) => f,
        Err(_) => return 0.0,
    };
    let baseline = (bh as f64 + fe.ascent() - fe.descent()) / 2.0;

    let mut lx = x;
    for w in ws {
        let te = match cr.text_extents(&w.name) {
            Ok(t) => t,
            Err(_) => return 0.0,
        };
        let bw = te.x_advance() + pad * 2.0;
        let by = 4.0;

        if !dry_run {
            if w.focused {
                cr.set_source_rgb(0x4c as f64 / 255., 0x56 as f64 / 255., 0x6a as f64 / 255.);
                rrect(cr, lx, by, bw, bh as f64 - by * 2.0, 3.0);
                let _ = cr.fill();
                cr.set_source_rgb(0xd8 as f64 / 255., 0xde as f64 / 255., 0xe9 as f64 / 255.);
            } else if w.urgent {
                cr.set_source_rgb(0xbd as f64 / 255., 0x2c as f64 / 255., 0x40 as f64 / 255.);
                rrect(cr, lx, by, bw, bh as f64 - by * 2.0, 3.0);
                let _ = cr.fill();
                cr.set_source_rgb(0.0, 0.0, 0.0);
            } else if w.visible {
                cr.set_source_rgb(0x55 as f64 / 255., 0x55 as f64 / 255., 0x55 as f64 / 255.);
                cr.rectangle(lx, bh as f64 - 2.0, bw, 2.0);
                let _ = cr.fill();
                cr.set_source_rgb(0.8, 0.8, 0.8);
            } else {
                cr.set_source_rgb(0.6, 0.6, 0.6);
            }
            cr.move_to(lx + pad, baseline);
            let _ = cr.show_text(&w.name);
        }
        lx += bw + gap;
    }
    lx - x - gap
}

pub const MODULE: Module = Module { draw, update: None };
