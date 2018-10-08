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
use std::sync::mpsc;
use std::time::Duration;

mod config;
mod term;
mod x;

use failure::Error;
use failure::ResultExt;
use nix::sys::termios;

use config::Config;

#[derive(Clone, Debug, PartialEq, Eq)]
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

    let mut conn = x::XServer::new()?;

    let _drop =
        term::StdOutTermios::with_setup(|attr| attr.local_flags ^= termios::LocalFlags::ICANON)?;
    let stdin = term::async_stdin();

    let mut last_procs = Vec::new();

    'app: loop {
        let procs = find_procs(&config, &mut conn)?;

        if procs.is_empty() {
            println!("No applications found, exiting.\r");
            break;
        }

        if procs != last_procs {
            println!();
            println!("Waiting for: {}", compressed_list(&procs));
            println!("Action? [d]elete, [t]erm, [k]ill, [q]uit)? ");
            last_procs = procs.clone();
        }

        match stdin.recv_timeout(Duration::from_millis(50)) {
            Ok(b'd') => {
                println!("Asking everyone to quit.");
                for proc in &procs {
                    conn.delete_window(proc.window)?;
                }
            }
            Ok(b't') => {
                println!("Telling everyone to quit.");
                for proc in &procs {
                    let _ = kill(proc.pid, Some(nix::sys::signal::SIGTERM))?;
                }
            }
            Ok(b'k') => {
                println!("Quitting everyone.");
                for proc in &procs {
                    let _ = kill(proc.pid, Some(nix::sys::signal::SIGKILL))?;
                }
            }
            Ok(b'q') => {
                println!("User asked, exiting");
                break 'app;
            }
            Ok(other) => println!("unsupported command: {:?}", other as char),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("End of commands, exiting");
                break 'app;
            }
        }
    }

    Ok(())
}

fn find_procs(config: &Config, conn: &mut x::XServer) -> Result<Vec<Proc>, Error> {
    let mut procs = Vec::with_capacity(16);

    conn.for_windows(|conn, window_id| {
        match gather_window_details(&config, conn, window_id) {
            Ok(Some(proc)) => procs.push(proc),
            Ok(None) => (),
            Err(e) => eprintln!(
                "couldn't get details (window vanished?): {:?} {:?}",
                window_id, e
            ),
        }
        Ok(())
    })?;

    procs.sort_by_key(|proc| (proc.class.to_string(), proc.pid));

    Ok(procs)
}

fn compressed_list(procs: &[Proc]) -> String {
    let mut buf = String::with_capacity(procs.len() * 32);
    let mut last = procs[0].class.to_string();
    buf.push_str(&last);
    buf.push_str(" (");
    for proc in procs {
        if proc.class != last && !buf.is_empty() {
            buf.pop(); // comma space
            buf.pop(); // comma space
            buf.push_str("), ");
            buf.push_str(&proc.class);
            buf.push_str(" (");
            last = proc.class.to_string();
        }
        buf.push_str(&format!("{}, ", proc.pid));
    }
    buf.pop(); // comma space
    buf.pop(); // comma space
    buf.push(')');
    buf
}

fn gather_window_details(
    config: &Config,
    conn: &x::XServer,
    window: x::XWindow,
) -> Result<Option<Proc>, Error> {
    let class = conn
        .read_class(window)
        .with_context(|_| format_err!("finding class of {:?}", window))?;

    for ignore in &config.ignore {
        if ignore.is_match(&class) {
            return Ok(None);
        }
    }

    let pids = conn
        .pids(window)
        .with_context(|_| format_err!("finding pid of {:?} ({:?})", class, window))?;

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

    match kill(pid, None) {
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
        supported_protocols: conn
            .supported_protocols(window)
            .with_context(|_| format_err!("finding protocols of {:?} (pid {})", class, pid))?,
        class,
    }))
}

fn kill(pid: u32, signal: Option<nix::sys::signal::Signal>) -> Result<bool, Error> {
    use nix::errno::Errno;
    use nix::sys::signal;
    use nix::unistd::Pid;
    assert!(pid <= ::std::i32::MAX as u32);

    Ok(match signal::kill(Pid::from_raw(pid as i32), signal) {
        Ok(()) => true,
        Err(nix::Error::Sys(Errno::ESRCH)) => false,
        other => bail!("kill {} failed: {:?}", pid, other),
    })
}

#[cfg(test)]
mod tests {
    fn proc(class: &str, pid: u32) -> ::Proc {
        ::Proc {
            class: class.to_string(),
            pid,
            supported_protocols: Vec::new(),
            window: ::x::XWindow(0),
        }
    }

    #[test]
    fn test_compressed_list() {
        assert_eq!(
            "aba (12), bar (34)",
            ::compressed_list(&[proc("aba", 12), proc("bar", 34)])
        );
        assert_eq!(
            "aba (12, 23), bar (34)",
            ::compressed_list(&[proc("aba", 12), proc("aba", 23), proc("bar", 34)])
        );
    }
}
