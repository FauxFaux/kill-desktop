extern crate clap;
#[macro_use]
extern crate failure;
extern crate nix;
extern crate xcb;

use std::env;
use std::thread;

use clap::Arg;
use failure::Error;
use failure::ResultExt;
use xcb::xproto as xp;

#[derive(Clone, Debug)]
struct Proc {
    window_id: xcb::Window,
    class: String,
    pid: u32,
    supported_protocols: Vec<xcb::Atom>,
}

fn main() -> Result<(), Error> {
    let matches = clap::App::new("kill-desktop")
        .arg(
            Arg::with_name("no-act")
                .short("n")
                .long("no-act")
                .help("list what needs to be killed, but don't do it"),
        )
        .get_matches();

    let no_act = matches.is_present("no-act");

    let (conn, _preferred_screen) = xcb::Connection::connect(None).with_context(|_| {
        format_err!(
            "connecting using DISPLAY={:?}",
            env::var("DISPLAY").unwrap_or_else(|_| "{unspecified/invalid}".to_string())
        )
    })?;

    let setup = conn.get_setup();

    let atom_client_list = existing_atom(&conn, "_NET_CLIENT_LIST")?;
    let atom_wm_pid = existing_atom(&conn, "_NET_WM_PID")?;
    let atom_wm_proto = existing_atom(&conn, "WM_PROTOCOLS")?;

    let mut procs = Vec::with_capacity(16);

    for screen in setup.roots() {
        let root = screen.root();
        for window_id in
            get_property::<xcb::Window>(&conn, root, atom_client_list, xp::ATOM_WINDOW, 4_096)?
        {
            let class = read_class(&conn, window_id)?;

            if class.contains("term") {
                eprintln!("ignoring terminal: {:?}", class);
                continue;
            }

            let pids = get_property::<u32>(&conn, window_id, atom_wm_pid, xp::ATOM_CARDINAL, 2)?;
            let pid = match pids.len() {
                1 => pids[0],
                _ => {
                    eprintln!(
                        "a window, {:?} ({}), has the wrong number of pids: {:?}",
                        class, window_id, pids
                    );
                    continue;
                }
            };

            match alive(pid) {
                Ok(true) => (),
                Ok(false) => {
                    eprintln!("{:?} (pid {}), wasn't even alive to start with", class, pid);
                    continue;
                }
                Err(other) => eprintln!("{:?} (pid {}): kill test failed: {:?}", class, pid, other),
            }

            let supported_protocols =
                get_property::<xcb::Atom>(&conn, window_id, atom_wm_proto, xp::ATOM_ATOM, 1_024)?;

            procs.push(Proc {
                window_id,
                pid,
                class,
                supported_protocols,
            })
        }
    }

    procs.sort_by_key(|proc| (proc.class.to_string(), proc.pid));

    let atom_wm_delete = existing_atom(&conn, "WM_DELETE_WINDOW")?;

    if !no_act {
        for proc in &procs {
            delete_window(&conn, atom_wm_proto, atom_wm_delete, proc)?;
        }
    }

    loop {
        procs.retain(|proc| match alive(proc.pid) {
            Ok(alive) => alive,
            Err(e) => {
                eprintln!("{:>6} {}: pid dislikes kill: {:?}", proc.pid, proc.class, e);
                true
            }
        });

        if procs.is_empty() {
            break;
        }

        println!();
        println!("Still waiting for...");
        for proc in &procs {
            if !no_act {
                delete_window(&conn, atom_wm_proto, atom_wm_delete, proc)?;
            }

            println!("{:>6} {}", proc.pid, proc.class)
        }

        sleep_ms(1_000);
    }

    Ok(())
}

fn alive(pid: u32) -> Result<bool, Error> {
    use nix::errno::Errno;
    use nix::sys::signal;
    use nix::unistd::Pid;
    assert!(pid <= ::std::i32::MAX as u32);

    Ok(match signal::kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(nix::Error::Sys(Errno::ESRCH)) => false,
        other => bail!("kill {} failed: {:?}", pid, other),
    })
}

fn read_class(conn: &xcb::Connection, window_id: xcb::Window) -> Result<String, Error> {
    let class = get_property::<u8>(&conn, window_id, xp::ATOM_WM_CLASS, xp::ATOM_STRING, 1024)?;
    let class = match class.iter().position(|b| 0 == *b) {
        Some(pos) => &class[..pos],
        None => &class[..],
    };
    Ok(String::from_utf8_lossy(class).to_string())
}

fn delete_window(
    conn: &xcb::Connection,
    atom_wm_proto: u32,
    atom_wm_delete: u32,
    proc: &Proc,
) -> Result<(), Error> {
    let event = xcb::xproto::ClientMessageEvent::new(
        32,
        proc.window_id,
        atom_wm_proto,
        xcb::xproto::ClientMessageData::from_data32([atom_wm_delete, xcb::CURRENT_TIME, 0, 0, 0]),
    );
    xcb::send_event_checked(
        &conn,
        true,
        proc.window_id,
        xcb::xproto::EVENT_MASK_NO_EVENT,
        &event,
    )
    .request_check()?;
    Ok(())
}

fn sleep_ms(ms: u64) {
    thread::sleep(::std::time::Duration::from_millis(ms))
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
