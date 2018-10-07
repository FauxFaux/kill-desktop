extern crate dirs;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
extern crate nix;
extern crate regex;
extern crate termion;
extern crate toml;
extern crate xcb;

use std::env;
use std::io;
use std::io::Read;
use std::thread;

mod config;
mod x;

use failure::Error;
use termion::raw::IntoRawMode;

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

    let _stdout = io::stdout().into_raw_mode().unwrap();
    let mut stdin = termion::async_stdin();

    let mut last_procs = Vec::new();

    'app: loop {
        let procs = find_procs(&config, &mut conn)?;

        if procs.is_empty() {
            break;
        }

        if procs != last_procs {
            println!();
            println!("Waiting for: {}\r", compressed_list(&procs));
            println!("Action? [d]elete, [t]erm, [k]ill, [q]uit)?\r");
            last_procs = procs.clone();
        }

        let mut buf = [0u8; 32];
        let found = stdin.read(&mut buf)?;
        let buf = &buf[..found];
        for cmd in buf {
            const CTRL_C: u8 = b'C' - 0x40;
            const CTRL_D: u8 = b'D' - 0x40;
            const CTRL_Z: u8 = b'Z' - 0x40;
            match *cmd {
                b'd' => {
                    for proc in &procs {
                        conn.delete_window(proc.window)?;
                    }
                }
                b't' => {
                    for proc in &procs {
                        let _ = kill(proc.pid, Some(nix::sys::signal::SIGTERM))?;
                    }
                }
                b'k' => {
                    for proc in &procs {
                        let _ = kill(proc.pid, Some(nix::sys::signal::SIGKILL))?;
                    }
                }
                b'q' | CTRL_C | CTRL_D | CTRL_Z => {
                    break 'app;
                }
                other => println!("unsupported command: {:?}\r", other as char),
            }
        }

        sleep_ms(50);
    }

    Ok(())
}

fn find_procs(config: &Config, conn: &mut x::XServer) -> Result<Vec<Proc>, Error> {
    let mut procs = Vec::with_capacity(16);

    conn.for_windows(|conn, window_id| {
        if let Some(proc) = gather_window_details(&config, conn, window_id)? {
            procs.push(proc);
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
        class,
        supported_protocols: conn.supported_protocols(window)?,
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

fn sleep_ms(ms: u64) {
    thread::sleep(::std::time::Duration::from_millis(ms))
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
