use std::env;

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Error;

use xcb::{x as xp, Xid};

pub struct XServer {
    conn: xcb::Connection,
    atoms: ExtraAtoms,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct XWindow(pub xp::Window);

xcb::atoms_struct! {
    #[derive(Debug)]
    struct ExtraAtoms {
        net_client_list  => b"_NET_CLIENT_LIST",
        net_wm_pid       => b"_NET_WM_PID",
        net_wm_name      => b"_NET_WM_NAME",
        wm_protocols     => b"WM_PROTOCOLS",
        wm_delete_window => b"WM_DELETE_WINDOW",
        utf8             => b"UTF8_STRING",

    }
}

impl XServer {
    pub fn new() -> Result<XServer, Error> {
        let (conn, _preferred_screen) = xcb::Connection::connect(None).with_context(|| {
            anyhow!(
                "connecting using DISPLAY={:?}",
                env::var("DISPLAY").unwrap_or_else(|_| "{unspecified/invalid}".to_string())
            )
        })?;

        let atoms = ExtraAtoms::intern_all(&conn)?;

        Ok(XServer { conn, atoms })
    }

    pub fn for_windows<F>(&self, mut func: F) -> Result<(), Error>
    where
        F: FnMut(&XServer, XWindow),
    {
        for screen in self.conn.get_setup().roots() {
            for window_id in self.get_property::<xp::Window>(
                XWindow(screen.root()),
                self.atoms.net_client_list,
                xp::ATOM_WINDOW,
                4_096,
            )? {
                func(self, XWindow(window_id));
            }
        }
        Ok(())
    }

    pub fn read_class(&self, window_id: XWindow) -> Result<String, Error> {
        self.read_string(window_id, xp::ATOM_WM_CLASS, xp::ATOM_STRING)
    }

    pub fn read_title(&self, window_id: XWindow) -> Result<String, Error> {
        self.read_string(window_id, self.atoms.net_wm_name, self.atoms.utf8)
    }

    fn read_string(
        &self,
        window_id: XWindow,
        atom: xp::Atom,
        prop_type: xp::Atom,
    ) -> Result<String, Error> {
        let string = self.get_property::<u8>(window_id, atom, prop_type, 1024)?;
        let string = match string.iter().position(|b| 0 == *b) {
            Some(pos) => &string[..pos],
            None => &string[..],
        };
        Ok(String::from_utf8_lossy(string).to_string())
    }

    pub fn delete_window(&mut self, window: &XWindow) -> Result<(), Error> {
        let event = xp::ClientMessageEvent::new(
            window.0,
            self.atoms.wm_protocols,
            xp::ClientMessageData::Data32([
                self.atoms.wm_delete_window.resource_id(),
                xp::CURRENT_TIME,
                0,
                0,
                0,
            ]),
        );

        let cookie = self.conn.send_request_checked(&xp::SendEvent {
            propagate: false,
            destination: xp::SendEventDest::Window(window.0),
            event_mask: xp::EventMask::NO_EVENT,
            event: &event,
        });

        self.conn.check_request(cookie)?;
        Ok(())
    }


    pub fn pids(&self, window: XWindow) -> Result<Vec<u32>, Error> {
        Ok(self.get_property::<u32>(window, self.atoms.net_wm_pid, xp::ATOM_CARDINAL, 64)?)
    }

    pub fn supports_delete(&self, window: XWindow) -> Result<bool, Error> {
        let supported =
            self.get_property::<u32>(window, self.atoms.wm_protocols, xp::ATOM_ATOM, 1_024)?;
        Ok(supported.contains(&self.atoms.wm_delete_window.resource_id()))
    }

    fn get_property<T: Clone + xp::PropEl>(
        &self,
        window: XWindow,
        property: xp::Atom,
        prop_type: xp::Atom,
        length: u32,
    ) -> Result<Vec<T>, Error> {
        let cookie = self.conn.send_request(&xp::GetProperty {
            delete: false,
            window: window.0,
            property,
            r#type: prop_type,
            long_offset: 0,
            long_length: length,
        });
        let reply = self.conn.wait_for_reply(cookie)?;
        Ok(reply.value::<T>().to_vec())
    }
}
