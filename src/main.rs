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
use std::thread;

mod config;
mod x;

use failure::Error;

use config::Config;

#[derive(Clone, Debug)]
struct Proc {
    window: x::XWindow,
    class: String,
    pid: u32,
    supported_protocols: Vec<xcb::Atom>,
}

fn main() -> Result<(), Error> {
    let mut args = env::args_os();
    let _us = args.next();
    if let Some(val) = args.next() {
        bail!("no arguments expected, got: {:?}", val);
    }

    let config = config::config()?;

    // TODO
    let no_act = true;

    let mut conn = x::XServer::new()?;

    let mut procs = Vec::with_capacity(16);

    conn.for_windows(|conn, window_id| {
        if let Some(proc) = gather_window_details(&config, conn, window_id)? {
            procs.push(proc);
        }
        Ok(())
    })?;

    procs.sort_by_key(|proc| (proc.class.to_string(), proc.pid));

    if !no_act {
        for proc in &procs {
            conn.delete_window(proc.window)?;
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
    conn: &x::XServer,
    window: x::XWindow,
) -> Result<Option<Proc>, Error> {
    let class = conn.read_class(window)?;
    for ignore in &config.ignore {
        if ignore.is_match(&class) {
            return Ok(None);
        }
    }

    let pids = conn.pids(window)?;

    let pid = match pids.len() {
        1 => pids[0],
        _ => {
            eprintln!(
                "a window, {:?} ({:?}), has the wrong number of pids: {:?}",
                class, window, pids
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
        window,
        pid,
        class,
        supported_protocols: conn.supported_protocols(window)?,
    }))
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

fn sleep_ms(ms: u64) {
    thread::sleep(::std::time::Duration::from_millis(ms))
}
