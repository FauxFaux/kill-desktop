extern crate dirs;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
extern crate nix;
extern crate regex;
extern crate toml;
extern crate xcb;

use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::thread;

use failure::Error;
use failure::ResultExt;
use regex::Regex;
use xcb::xproto as xp;

#[derive(Clone, Debug)]
struct Proc {
    window_id: xcb::Window,
    class: String,
    pid: u32,
    supported_protocols: Vec<xcb::Atom>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawConfig {
    ignore: Vec<String>,
    ignores_delete: Vec<String>,
}

struct Config {
    ignore: Vec<Regex>,
    ignores_delete: Vec<Regex>,
}

#[derive(Copy, Clone, Debug)]
struct ExtraAtoms {
    net_client_list: xcb::Atom,
    net_wm_pid: xcb::Atom,
    wm_protocols: xcb::Atom,
    wm_delete_window: xcb::Atom,
}

fn main() -> Result<(), Error> {
    let mut args = env::args_os();
    let _us = args.next();
    if let Some(val) = args.next() {
        bail!("no arguments expected, got: {:?}", val);
    }

    let config = load_config()?.into_config()?;

    // TODO
    let no_act = true;

    let (conn, _preferred_screen) = xcb::Connection::connect(None).with_context(|_| {
        format_err!(
            "connecting using DISPLAY={:?}",
            env::var("DISPLAY").unwrap_or_else(|_| "{unspecified/invalid}".to_string())
        )
    })?;

    let setup = conn.get_setup();

    let atoms = ExtraAtoms {
        net_client_list: existing_atom(&conn, "_NET_CLIENT_LIST")?,
        net_wm_pid: existing_atom(&conn, "_NET_WM_PID")?,
        wm_protocols: existing_atom(&conn, "WM_PROTOCOLS")?,
        wm_delete_window: existing_atom(&conn, "WM_DELETE_WINDOW")?,
    };

    let mut procs = Vec::with_capacity(16);

    for screen in setup.roots() {
        for window_id in get_property::<xcb::Window>(
            &conn,
            screen.root(),
            atoms.net_client_list,
            xp::ATOM_WINDOW,
            4_096,
        )? {
            if let Some(proc) = gather_window_details(&config, &conn, atoms, window_id)? {
                procs.push(proc);
            }
        }
    }

    procs.sort_by_key(|proc| (proc.class.to_string(), proc.pid));

    if !no_act {
        for proc in &procs {
            delete_window(&conn, atoms, proc)?;
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
            println!("{:>6} {}", proc.pid, proc.class)
        }

        sleep_ms(1_000);
    }

    Ok(())
}

fn gather_window_details(
    config: &Config,
    conn: &xcb::Connection,
    atoms: ExtraAtoms,
    window_id: xcb::Window,
) -> Result<Option<Proc>, Error> {
    let class = read_class(&conn, window_id)?;
    for ignore in &config.ignore {
        if ignore.is_match(&class) {
            return Ok(None);
        }
    }

    let pids = get_property::<u32>(&conn, window_id, atoms.net_wm_pid, xp::ATOM_CARDINAL, 2)?;

    let pid = match pids.len() {
        1 => pids[0],
        _ => {
            eprintln!(
                "a window, {:?} ({}), has the wrong number of pids: {:?}",
                class, window_id, pids
            );
            return Ok(None);
        }
    };

    match alive(pid) {
        Ok(true) => (),
        Ok(false) => {
            eprintln!("{:?} (pid {}), wasn't even alive to start with", class, pid);
            return Ok(None);
        }
        Err(other) => eprintln!("{:?} (pid {}): kill test failed: {:?}", class, pid, other),
    }

    Ok(Some(Proc {
        window_id,
        pid,
        class,
        supported_protocols: get_property::<xcb::Atom>(
            &conn,
            window_id,
            atoms.wm_protocols,
            xp::ATOM_ATOM,
            1_024,
        )?,
    }))
}

fn find_config() -> Result<PathBuf, Error> {
    let mut tried = Vec::new();

    if let Some(mut config) = dirs::config_dir() {
        config.push("kill-desktop");
        fs::create_dir_all(&config)?;
        config.push("config.toml");
        if config.is_file() {
            return Ok(config);
        }

        tried.push(config);
    }

    if let Some(mut config) = dirs::home_dir() {
        config.push(".kill-desktop.toml");
        if config.is_file() {
            return Ok(config);
        }

        tried.push(config);
    }

    let config = PathBuf::from("kill-desktop.toml");
    if config.is_file() {
        return Ok(config);
    }

    tried.push(config);

    Err(format_err!(
        "couldn't find a config file, tried: {:?}",
        tried
    ))
}

fn load_config() -> Result<RawConfig, Error> {
    let path = find_config()?;
    let mut file = fs::File::open(&path).with_context(|_| format_err!("reading {:?}", path))?;
    let mut bytes = Vec::with_capacity(4096);
    file.read_to_end(&mut bytes)?;
    Ok(toml::from_slice(&bytes)?)
}

impl RawConfig {
    fn into_config(self) -> Result<Config, Error> {
        Ok(Config {
            ignore: self
                .ignore
                .into_iter()
                .map(|s| Regex::new(&s))
                .collect::<Result<Vec<_>, regex::Error>>()?,
            ignores_delete: self
                .ignores_delete
                .into_iter()
                .map(|s| Regex::new(&s))
                .collect::<Result<Vec<_>, regex::Error>>()?,
        })
    }
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

fn delete_window(conn: &xcb::Connection, atoms: ExtraAtoms, proc: &Proc) -> Result<(), Error> {
    let event = xcb::xproto::ClientMessageEvent::new(
        32,
        proc.window_id,
        atoms.wm_protocols,
        xcb::xproto::ClientMessageData::from_data32([
            atoms.wm_delete_window,
            xcb::CURRENT_TIME,
            0,
            0,
            0,
        ]),
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
