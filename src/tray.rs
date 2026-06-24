use crate::config;
use std::error::Error;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;


const MAX_CLIENTS: usize = 32;
const TRAY_SIZE: u16 = 20;

x11rb::atom_manager! {
    pub TrayAtoms: TrayAtomsCookie {
        _NET_SYSTEM_TRAY_OPCODE,
        _XEMBED,
        _XEMBED_INFO,
        MANAGER,
        _NET_SYSTEM_TRAY_ORIENTATION,
        WM_NORMAL_HINTS,
    }
}

struct TrayClient {
    wrapper: u32,
    window: u32,
    mapped: bool,
    hidden: bool,
    xembed: bool,
    xembed_version: u32,
    xembed_flags: u32,
}

pub struct TrayState {
    pub active: bool,
    pub dirty: bool,
    pub tray_width: i32,
    bar: u32,
    tray_atom: u32,
    atoms: Option<TrayAtoms>,
    clients: Vec<TrayClient>,
}

fn is_xembed_mapped(flags: u32) -> bool {
    (flags & 1) != 0
}

fn query_xembed<C: Connection>(conn: &C, win: u32, xembed_info_atom: u32) -> (bool, u32, u32) {
    let cookie = conn.get_property(false, win, xembed_info_atom, AtomEnum::CARDINAL, 0, 2);
    let reply = match cookie {
        Ok(c) => match c.reply() {
            Ok(r) => r,
            Err(_) => return (false, 0, 0),
        },
        Err(_) => return (false, 0, 0),
    };
    if reply.value.len() < 8 {
        return (false, 0, 0);
    }
    let data = unsafe {
        std::slice::from_raw_parts(reply.value.as_ptr() as *const u32, reply.value.len() / 4)
    };
    (true, data[0], data[1])
}

fn send_visibility<C: Connection>(conn: &C, window: u32, state: u8) -> Result<(), Box<dyn Error>> {
    let mut ev = [0u8; 32];
    ev[0] = 15; // VisibilityNotify
    ev[1] = 0; // unused
    ev[4..8].copy_from_slice(&window.to_ne_bytes());
    ev[8] = state;
    conn.send_event(false, window, EventMask::NO_EVENT, ev)?;
    Ok(())
}

impl TrayState {
    pub fn new() -> Self {
        TrayState {
            active: false,
            dirty: false,
            tray_width: 0,
            bar: 0,
            tray_atom: 0,
            atoms: None,
            clients: Vec::new(),
        }
    }

    pub fn find(&self, w: u32) -> Option<usize> {
        self.clients
            .iter()
            .position(|c| c.wrapper == w || c.window == w)
    }

    fn atoms(&self) -> &TrayAtoms {
        self.atoms.as_ref().unwrap()
    }

    fn remove_client<C: Connection>(&mut self, conn: &C, idx: usize) -> Result<(), Box<dyn Error>> {
        let wrapper = self.clients[idx].wrapper;
        let client = self.clients[idx].window;

        let _ = conn.unmap_window(client);
        let _ = conn.reparent_window(client, conn.setup().roots[0].root, 0, 0);
        let _ = conn.destroy_window(wrapper);
        self.clients.remove(idx);
        self.dirty = true;
        Ok(())
    }

    fn client_by_window(&self, w: u32) -> Option<usize> {
        self.clients.iter().position(|c| c.window == w)
    }

    fn client_by_wrapper(&self, w: u32) -> Option<usize> {
        self.clients.iter().position(|c| c.wrapper == w)
    }

    pub fn layout<C: Connection>(
        &mut self,
        conn: &C,
        sw: i32,
        bh: i32,
        right_margin: &mut i32,
    ) -> Result<(), Box<dyn Error>> {
        if self.atoms.is_none() || self.bar == 0 {
            return Ok(());
        }
        let icon_sz = config::FONT_SIZE_ICON as i32 + 6;
        let pad = 6;

        let xembed_info_atom = self.atoms()._XEMBED_INFO;

        // Pass 1: refresh XEMBED state and count mapped clients
        let mut total_w = 0i32;
        for i in 0..self.clients.len() {
            let (is_xembed, _version, flags) = query_xembed(conn, self.clients[i].window, xembed_info_atom);
            self.clients[i].xembed = is_xembed;
            if is_xembed {
                self.clients[i].xembed_flags = flags;
            }

            let should_map = if self.clients[i].hidden {
                false
            } else if is_xembed {
                is_xembed_mapped(flags)
            } else {
                true
            };
            self.clients[i].mapped = should_map;

            if should_map {
                if total_w > 0 {
                    total_w += pad;
                }
                total_w += icon_sz;
            }
        }

        self.tray_width = total_w;
        *right_margin = if total_w > 0 { total_w + 18 } else { 12 };

        // Pass 2: position wrappers starting from right edge of bar
        let mut x = sw - total_w - 6;
        for i in 0..self.clients.len() {
            if !self.clients[i].mapped {
                let _ = conn.unmap_window(self.clients[i].window);
                let _ = conn.unmap_window(self.clients[i].wrapper);
                continue;
            }

            let wrapper = self.clients[i].wrapper;
            let client = self.clients[i].window;
            let y = (bh - icon_sz) / 2;

            let _ = conn.configure_window(
                wrapper,
                &ConfigureWindowAux::new()
                    .x(x)
                    .y(y)
                    .width(icon_sz as u32)
                    .height(icon_sz as u32),
            );
            let _ = conn.configure_window(
                client,
                &ConfigureWindowAux::new()
                    .width(icon_sz as u32)
                    .height(icon_sz as u32),
            );

            let _ = conn.map_window(wrapper);
            let _ = conn.map_window(client);

            let _ = conn.clear_area(false, wrapper, 0, 0, 0, 0);
            let _ = send_visibility(conn, client, 2); // FullyObscured
            let _ = send_visibility(conn, client, 0); // Unobscured
            let _ = conn.clear_area(true, client, 0, 0, 0, 0);

            x += icon_sz + pad;
        }

        conn.configure_window(
            self.bar,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        conn.flush()?;
        Ok(())
    }

    pub fn init<C: Connection>(
        &mut self,
        conn: &C,
        bar: u32,
        screen_num: usize,
    ) -> Result<(), Box<dyn Error>> {
        self.bar = bar;
        let screen = &conn.setup().roots[screen_num];

        let suffix = format!("_NET_SYSTEM_TRAY_S{screen_num}");
        let cookie = conn.intern_atom(false, suffix.as_bytes())?;
        self.tray_atom = cookie.reply()?.atom;
        if self.tray_atom == 0 {
            eprintln!("tray: intern failed");
            return Ok(());
        }

        self.atoms = Some(TrayAtoms::new(conn)?.reply()?);

        conn.set_selection_owner(bar, self.tray_atom, x11rb::CURRENT_TIME)?;
        conn.flush()?;

        let sel_reply = conn.get_selection_owner(self.tray_atom)?.reply()?;
        if sel_reply.owner != bar {
            eprintln!("tray: cannot acquire selection");
            return Ok(());
        }

        conn.change_property32(
            PropMode::REPLACE,
            screen.root,
            self.tray_atom,
            AtomEnum::WINDOW,
            &[bar],
        )?;

        conn.change_property32(
            PropMode::REPLACE,
            bar,
            self.atoms()._NET_SYSTEM_TRAY_ORIENTATION,
            AtomEnum::CARDINAL,
            &[0],
        )?;
        conn.flush()?;

        let mev = ClientMessageEvent::new(
            32,
            screen.root,
            self.atoms().MANAGER,
            [x11rb::CURRENT_TIME, self.tray_atom, bar, 0u32, 0u32],
        );
        conn.send_event(false, screen.root, EventMask::STRUCTURE_NOTIFY, mev)?;
        conn.flush()?;

        self.active = true;
        self.dirty = true;
        Ok(())
    }

    fn dock<C: Connection>(&mut self, conn: &C, client: u32) -> Result<(), Box<dyn Error>> {
        if self.clients.len() >= MAX_CLIENTS {
            return Ok(());
        }
        if self.client_by_window(client).is_some() {
            return Ok(());
        }

        let attrs = match conn.get_window_attributes(client)?.reply() {
            Ok(a) => a,
            Err(_) => return Ok(()),
        };
        let geom = match conn.get_geometry(client)?.reply() {
            Ok(g) => g,
            Err(_) => return Ok(()),
        };
        let client_depth = geom.depth;
        let client_visual = attrs.visual;
        let client_colormap = attrs.colormap;

        let wrapper = conn.generate_id()?;
        let screen = &conn.setup().roots[0];

        conn.create_window(
            client_depth,
            wrapper,
            self.bar,
            0,
            0,
            TRAY_SIZE,
            TRAY_SIZE,
            0,
            WindowClass::INPUT_OUTPUT,
            client_visual,
            &CreateWindowAux::new()
                .background_pixel(0x2e3440)
                .border_pixel(screen.black_pixel)
                .colormap(client_colormap)
                .event_mask(
                    EventMask::SUBSTRUCTURE_REDIRECT
                        | EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::EXPOSURE
                        | EventMask::PROPERTY_CHANGE,
                )
                .backing_store(BackingStore::WHEN_MAPPED)
                .save_under(1u32),
        )?;

        conn.change_save_set(SetMode::INSERT, client)?;
        conn.change_window_attributes(
            client,
            &ChangeWindowAttributesAux::new().event_mask(
                EventMask::STRUCTURE_NOTIFY
                    | EventMask::PROPERTY_CHANGE
                    | EventMask::RESIZE_REDIRECT,
            ),
        )?;
        conn.reparent_window(client, wrapper, 0, 0)?;

        let size_hints: [u32; 18] = [
            3, 0, 0, 0, 0, TRAY_SIZE as u32, TRAY_SIZE as u32, TRAY_SIZE as u32, TRAY_SIZE as u32, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        conn.change_property32(
            PropMode::REPLACE,
            client,
            self.atoms().WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &size_hints,
        )?;

        let (is_xembed, version, flags) =
            query_xembed(conn, client, self.atoms()._XEMBED_INFO);

        self.clients.push(TrayClient {
            wrapper,
            window: client,
            mapped: false,
            hidden: false,
            xembed: is_xembed,
            xembed_version: version,
            xembed_flags: flags,
        });

        if is_xembed {
            let xe = ClientMessageEvent::new(
                32,
                client,
                self.atoms()._XEMBED,
                [0u32, 0u32, wrapper, 0u32, 0u32],
            );
            conn.send_event(false, client, EventMask::NO_EVENT, xe)?;
        }

        conn.flush()?;
        self.dirty = true;
        Ok(())
    }

    fn update_xembed<C: Connection>(&mut self, conn: &C, idx: usize) {
        let client = self.clients[idx].window;
        let (is_xembed, version, flags) =
            query_xembed(conn, client, self.atoms()._XEMBED_INFO);
        self.clients[idx].xembed = is_xembed;
        self.clients[idx].xembed_version = version;
        self.clients[idx].xembed_flags = flags;
    }

    fn deactivate<C: Connection>(&mut self, conn: &C) -> Result<(), Box<dyn Error>> {
        eprintln!("tray: lost selection, cleaning up");
        self.active = false;
        self.dirty = true;
        self.tray_width = 0;
        for c in self.clients.drain(..) {
            let _ = conn.unmap_window(c.window);
            let _ = conn.reparent_window(c.window, conn.setup().roots[0].root, 0, 0);
            let _ = conn.destroy_window(c.wrapper);
        }
        conn.flush()?;
        Ok(())
    }

    pub fn handle_event<C: Connection>(
        &mut self,
        conn: &C,
        ev: &Event,
    ) -> Result<(), Box<dyn Error>> {
        if !self.active {
            return Ok(());
        }
        match ev {
            Event::ClientMessage(cm_ev)
                if cm_ev.type_ == self.atoms()._NET_SYSTEM_TRAY_OPCODE =>
            {
                let data = cm_ev.data.as_data32();
                if data[1] == 0 {
                    self.dock(conn, data[2])?;
                }
            }
            Event::DestroyNotify(ev) => {
                if let Some(i) = self.client_by_window(ev.window) {
                    self.remove_client(conn, i)?;
                } else if let Some(i) = self.client_by_wrapper(ev.window) {
                    self.remove_client(conn, i)?;
                }
            }
            Event::PropertyNotify(ev) if ev.atom == self.atoms()._XEMBED_INFO => {
                if let Some(i) = self.client_by_window(ev.window) {
                    self.update_xembed(conn, i);
                    self.dirty = true;
                }
            }
            Event::ConfigureRequest(ev) => {
                if self.client_by_window(ev.window).is_some() {
                    conn.configure_window(
                        ev.window,
                        &ConfigureWindowAux::new()
                            .width(TRAY_SIZE as u32)
                            .height(TRAY_SIZE as u32),
                    )?;
                    conn.flush()?;
                }
            }
            Event::ReparentNotify(ev) => {
                if let Some(i) = self.client_by_window(ev.window) {
                    if ev.parent != self.clients[i].wrapper {
                        self.remove_client(conn, i)?;
                    }
                }
            }
            Event::SelectionClear(ev) if ev.selection == self.tray_atom => {
                self.deactivate(conn)?;
            }
            _ => {}
        }
        Ok(())
    }
}
