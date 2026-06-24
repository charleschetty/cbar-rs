// modules/workspace/mod.rs — workspace module, dispatches to the active WM.
//
// To add a new window manager:
//   1. Create a file like hyprland.rs next to i3.rs
//   2. Add `pub mod hyprland;` below
//   3. Add a match arm in init()

pub mod i3;

use std::sync::mpsc;

use crate::config;
use crate::modules::AppState;
use crate::modules::workspace::i3::Workspace;

pub struct WsHandle {
    rx: mpsc::Receiver<Vec<Workspace>>,
}

pub fn init(state: &mut AppState) -> Option<WsHandle> {
    match config::WORKSPACE_TYPE {
        "i3" | "sway" => i3::init(state),
        _ => {
            eprintln!("unsupported WORKSPACE_TYPE: {}", config::WORKSPACE_TYPE);
            None
        }
    }
}

pub fn poll(handle: &WsHandle, state: &mut AppState) {
    while let Ok(ws) = handle.rx.try_recv() {
        state.i3workspace = Some(ws);
    }
}

pub use i3::MODULE;
