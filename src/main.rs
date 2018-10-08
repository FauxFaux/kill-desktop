extern crate dirs;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
extern crate nix;
extern crate regex;
extern crate toml;
extern crate xcb;

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::io;
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

mod config;
mod term;
mod x;

use failure::Error;
use nix::sys::termios;

use config::Config;
use x::XWindow;

// TODO: don't want this to be PartialEq/Eq
#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowInfo {
    pids: HashSet<u32>,
    supports_delete: bool,
    class: String,
    title: String,
}

#[derive(Clone, Debug)]
struct PidInfo {
    windows: Vec<XWindow>,
    exe: String,
}

#[derive(Clone, Debug, Default)]
struct Hatred {
    blacklisted_pids: HashMap<u32, String>,
    blacklisted_windows: HashMap<XWindow, String>,
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

    //    let mut seen_windows = HashMap::new();
    //    let mut seen_pids = HashMap::new();
    let mut last_procs = Vec::new();

    'app: loop {
        let procs = find_procs(&config, &mut conn)?;

        if procs.is_empty() {
            println!("No applications found, exiting.\r");
            break;
        }

        if procs != last_procs {
            println!();
            println!(
                "Waiting for: {}",
                compressed_list(&procs.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>())
            );
            print!("Action? [d]elete, [t]erm, [k]ill, [q]uit)? ");
            io::stdout().flush()?;
            last_procs = procs.clone();
        }

        match stdin.recv_timeout(Duration::from_millis(50)) {
            Ok(b'd') => {
                println!("Asking everyone to quit.");
                for (window, _info) in &procs {
                    conn.delete_window(window)?;
                }
            }
            Ok(b't') => {
                println!("Telling everyone to quit.");
                for (_window, info) in &procs {
                    for pid in &info.pids {
                        let _ = kill(*pid, Some(nix::sys::signal::SIGTERM))?;
                    }
                }
            }
            Ok(b'k') => {
                println!("Quitting everyone.");
                for (_window, info) in &procs {
                    for pid in &info.pids {
                        let _ = kill(*pid, Some(nix::sys::signal::SIGKILL))?;
                    }
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

fn find_procs(config: &Config, conn: &mut x::XServer) -> Result<Vec<(XWindow, WindowInfo)>, Error> {
    let mut procs = Vec::with_capacity(16);

    conn.for_windows(|conn, window_id| {
        match gather_window_details(&config, conn, window_id, &mut Hatred::default()) {
            Ok(Some(proc)) => procs.push((window_id, proc)),
            Ok(None) => (),
            Err(e) => eprintln!(
                "couldn't get details (window vanished?): {:?} {:?}",
                window_id, e
            ),
        }
        Ok(())
    })?;

    procs.sort_by_key(|(_window, info)| info.class.to_string());

    Ok(procs)
}

fn compressed_list(procs: &[WindowInfo]) -> String {
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
        match proc.pids.len() {
            0 => buf.push_str("?, "),
            _ => {
                for pid in &proc.pids {
                    buf.push_str(&format!("{}, ", pid));
                }
            }
        }
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
    hatred: &mut Hatred,
) -> Result<Option<WindowInfo>, Error> {
    let class = match conn.read_class(window) {
        Ok(class) => class,
        Err(e) => {
            hatred
                .blacklisted_windows
                .insert(window, format!("read class failed: {:?}", e));
            return Ok(None);
        }
    };

    for ignore in &config.ignore {
        if ignore.is_match(&class) {
            return Ok(None);
        }
    }

    let mut pids = match conn.pids(window) {
        Ok(pids) => pids,
        Err(_) => {
            // TODO: complain somewhere?
            Vec::new()
        }
    }
    .into_iter()
    .collect::<HashSet<_>>();

    // Note: `pids` will almost always have a length of 1, sometimes zero.
    // We're processing it as a list for code simplicity

    // if it's already blacklisted, we don't care
    pids.retain(|pid| !hatred.blacklisted_pids.contains_key(pid));

    pids.retain(|&pid| match kill(pid, None) {
        Ok(true) => true,
        Ok(false) => {
            hatred
                .blacklisted_pids
                .insert(pid, "reported nonexistent pid".to_string());
            false
        }
        Err(e) => {
            hatred
                .blacklisted_pids
                .insert(pid, format!("reported erroring pid: {:?}", e));
            false
        }
    });

    let supports_delete = match conn.supports_delete(window) {
        Ok(val) => val,
        Err(_) => {
            // TODO: don't totally ignore
            false
        }
    };

    Ok(Some(WindowInfo {
        pids,
        class,
        supports_delete,
        title: String::new(), // TODO
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
    use std::collections::HashSet;

    fn proc(class: &str, pid: u32) -> ::WindowInfo {
        let mut pids = HashSet::new();
        pids.insert(pid);
        ::WindowInfo {
            class: class.to_string(),
            pids,
            supports_delete: true,
            title: String::new(),
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
