extern crate pretty_env_logger;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate xcb;

use std::collections::HashSet;
use std::env;

use failure::Error;
use failure::ResultExt;
use xcb::xproto;

#[derive(Clone, Debug)]
struct Proc {
    window_id: xcb::Window,
    class: String,
    pid: Option<u32>,
}

fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let (conn, _preferred_screen) = xcb::Connection::connect(None).with_context(|_| {
        format_err!(
            "connecting using DISPLAY={:?}",
            env::var("DISPLAY").unwrap_or_else(|_| "{unspecified/invalid}".to_string())
        )
    })?;

    let setup = conn.get_setup();

    // TODO: `AtomEnum`
    let atom_window = existing_atom(&conn, "WINDOW")?;
    let atom_cardinal = existing_atom(&conn, "CARDINAL")?;
    let atom_client_list = existing_atom(&conn, "_NET_CLIENT_LIST")?;
    let atom_wm_pid = existing_atom(&conn, "_NET_WM_PID")?;
    let atom_wm_proto = existing_atom(&conn, "WM_PROTOCOLS")?;
    let atom_wm_delete = existing_atom(&conn, "WM_DELETE_WINDOW")?;

    let mut procs = Vec::with_capacity(16);

    for screen in setup.roots() {
        let root = screen.root();
        for window_id in get_property::<u32>(&conn, root, atom_client_list, atom_window, 1_024)? {
            let pids = get_property::<u32>(&conn, window_id, atom_wm_pid, atom_cardinal, 2)?;
            let pid = match pids.len() {
                1 => Some(pids[0]),
                other => {
                    warn!(
                        "a window, {}, has the wrong number of pids: {:?}",
                        window_id, pids
                    );
                    None
                }
            };

            let class = get_property::<u8>(
                &conn,
                window_id,
                xproto::ATOM_WM_CLASS,
                xproto::ATOM_STRING,
                1024,
            )?;
            let class = match class.iter().position(|b| 0 == *b) {
                Some(pos) => &class[..pos],
                None => &class[..],
            };
            let class = String::from_utf8_lossy(class).to_string();

            procs.push(Proc {
                window_id,
                pid,
                class,
            })
        }
    }

    for proc in &procs {
        println!("{} {} {:?}", proc.window_id, proc.class, proc.pid)
    }

    Ok(())
}

fn existing_atom(conn: &xcb::Connection, name: &'static str) -> Result<xcb::Atom, Error> {
    Ok(xcb::intern_atom(&conn, true, name)
        .get_reply()
        .with_context(|_| format_err!("WM doesn't support {}", name))?
        .atom())
}

fn get_property<T: Clone>(
    conn: &xcb::Connection,
    window: xcb::Window,
    property: xcb::Atom,
    prop_type: xcb::Atom,
    length: u32,
) -> Result<Vec<T>, Error> {
    let req = xcb::get_property(&conn, false, window, property, prop_type, 0, length);
    let reply = req.get_reply()?;
    Ok(reply.value::<T>().to_vec())
}
