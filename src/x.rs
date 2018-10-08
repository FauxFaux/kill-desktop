use std::env;

use failure::Error;
use failure::ResultExt;
use xcb;
use xcb::xproto as xp;

pub struct XServer {
    conn: xcb::Connection,
    atoms: ExtraAtoms,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct XWindow(pub xcb::Window);

#[derive(Copy, Clone, Debug)]
struct ExtraAtoms {
    net_client_list: xcb::Atom,
    net_wm_pid: xcb::Atom,
    wm_protocols: xcb::Atom,
    wm_delete_window: xcb::Atom,
}

impl XServer {
    pub fn new() -> Result<XServer, Error> {
        let (conn, _preferred_screen) = xcb::Connection::connect(None).with_context(|_| {
            format_err!(
                "connecting using DISPLAY={:?}",
                env::var("DISPLAY").unwrap_or_else(|_| "{unspecified/invalid}".to_string())
            )
        })?;

        let atoms = ExtraAtoms {
            net_client_list: existing_atom(&conn, "_NET_CLIENT_LIST")?,
            net_wm_pid: existing_atom(&conn, "_NET_WM_PID")?,
            wm_protocols: existing_atom(&conn, "WM_PROTOCOLS")?,
            wm_delete_window: existing_atom(&conn, "WM_DELETE_WINDOW")?,
        };

        Ok(XServer { conn, atoms })
    }

    pub fn for_windows<F>(&self, mut func: F) -> Result<(), Error>
    where
        F: FnMut(&XServer, XWindow) -> Result<(), Error>,
    {
        for screen in self.conn.get_setup().roots() {
            for window_id in self.get_property::<xcb::Window>(
                XWindow(screen.root()),
                self.atoms.net_client_list,
                xp::ATOM_WINDOW,
                4_096,
            )? {
                func(self, XWindow(window_id))?;
            }
        }
        Ok(())
    }

    pub fn read_class(&self, window_id: XWindow) -> Result<String, Error> {
        let class = self.get_property::<u8>(window_id, xp::ATOM_WM_CLASS, xp::ATOM_STRING, 1024)?;
        let class = match class.iter().position(|b| 0 == *b) {
            Some(pos) => &class[..pos],
            None => &class[..],
        };
        Ok(String::from_utf8_lossy(class).to_string())
    }

    pub fn delete_window(&mut self, window: &XWindow) -> Result<(), Error> {
        let event = xcb::xproto::ClientMessageEvent::new(
            32,
            window.0,
            self.atoms.wm_protocols,
            xcb::xproto::ClientMessageData::from_data32([
                self.atoms.wm_delete_window,
                xcb::CURRENT_TIME,
                0,
                0,
                0,
            ]),
        );
        xcb::send_event_checked(
            &self.conn,
            true,
            window.0,
            xcb::xproto::EVENT_MASK_NO_EVENT,
            &event,
        )
        .request_check()?;
        Ok(())
    }

    pub fn pids(&self, window: XWindow) -> Result<Vec<u32>, Error> {
        Ok(self.get_property::<u32>(window, self.atoms.net_wm_pid, xp::ATOM_CARDINAL, 64)?)
    }

    pub fn supports_delete(&self, window: XWindow) -> Result<bool, Error> {
        let supported =
            self.get_property::<u32>(window, self.atoms.wm_protocols, xp::ATOM_ATOM, 1_024)?;
        Ok(supported.contains(&self.atoms.wm_delete_window))
    }

    fn get_property<T: Clone>(
        &self,
        window: XWindow,
        property: xcb::Atom,
        prop_type: xcb::Atom,
        length: u32,
    ) -> Result<Vec<T>, Error> {
        let req = xcb::get_property(&self.conn, false, window.0, property, prop_type, 0, length);
        let reply = req.get_reply()?;
        Ok(reply.value::<T>().to_vec())
    }
}

fn existing_atom(conn: &xcb::Connection, name: &'static str) -> Result<xcb::Atom, Error> {
    Ok(xcb::intern_atom(&conn, true, name)
        .get_reply()
        .with_context(|_| format_err!("WM doesn't support {}", name))?
        .atom())
}
