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
    window: Vec<WindowInfo>,
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

    let mut seen_windows: HashMap<XWindow, WindowInfo> = HashMap::new();
    let mut seen_pids = HashMap::new();
    let mut hatred = Hatred::default();

    'app: loop {
        let mut now_windows = find_windows(&config, &mut conn, &mut hatred)?;

        let mut died_this_time = Vec::new();

        let mut meaningful_change = false;

        // windows we already knew about
        for (window, old_info) in &mut seen_windows {
            match now_windows.remove(&window) {
                Some(new_info) => {
                    // the window was here and is still here, update it:
                    old_info.class = new_info.class;
                    if old_info.title != new_info.title {
                        meaningful_change = true;
                        old_info.title = new_info.title;
                    }
                    old_info.supports_delete = new_info.supports_delete;
                    old_info.pids.extend(new_info.pids);
                }
                None => {
                    died_this_time.push((*window, old_info.clone()));
                    meaningful_change = true;
                }
            }
        }

        // windows that have gone, but aren't forgotten
        for (window, old_info) in died_this_time {
            for &pid in &old_info.pids {
                seen_pids.insert(pid, old_info.clone());
            }

            seen_windows.remove(&window);
        }

        // windows that are actually new
        for (window, new_info) in now_windows {
            seen_windows.insert(window, new_info);
            meaningful_change = true;
        }

        // TODO: ignoring errors here
        seen_pids.retain(|&pid, _| if kill(pid, None).unwrap_or(false) {
            true
        } else {
            meaningful_change = true;
            false
        });

        if seen_windows.is_empty() && seen_pids.is_empty() {
            println!("No applications found, exiting.");
            break;
        }

        if meaningful_change {
            println!(); // end of the prompt
            println!();
            println!(
                "Windows: {}",
                compressed_list(
                    &seen_windows
                        .iter()
                        .map(|(_, v)| v.clone())
                        .collect::<Vec<_>>()
                )
            );
            if !seen_pids.is_empty() {
                println!("  Procs: {:?}", seen_pids.keys());
            }
            print!("Action?  [d]elete, [t]erm, [k]ill, [q]uit)? ");
            io::stdout().flush()?;
        }

        match stdin.recv_timeout(Duration::from_millis(50)) {
            Ok(b'd') => {
                println!();
                println!("Asking everyone to quit.");
                for (window, _info) in &seen_windows {
                    conn.delete_window(window)?;
                }
            }
            Ok(b't') => {
                println!();
                println!("Telling everyone to quit.");
                for (_window, info) in &seen_windows {
                    for pid in &info.pids {
                        let _ = kill(*pid, Some(nix::sys::signal::SIGTERM))?;
                    }
                }
            }
            Ok(b'k') => {
                println!();
                println!("Quitting everyone.");
                for (_window, info) in &seen_windows {
                    for pid in &info.pids {
                        let _ = kill(*pid, Some(nix::sys::signal::SIGKILL))?;
                    }
                }
            }
            Ok(b'q') => {
                println!();
                println!("User asked, exiting.");
                break 'app;
            }
            Ok(other) => println!("unsupported command: {:?}", other as char),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!();
                println!("End of commands, exiting");
                break 'app;
            }
        }
    }

    Ok(())
}

fn find_windows(
    config: &Config,
    conn: &mut x::XServer,
    hatred: &mut Hatred,
) -> Result<HashMap<XWindow, WindowInfo>, Error> {
    let mut windows = HashMap::with_capacity(16);

    conn.for_windows(|conn, window_id| {
        if hatred.blacklisted_windows.contains_key(&window_id) {
            return;
        }

        if let Some(proc) = gather_window_details(&config, conn, window_id, hatred) {
            windows.insert(window_id, proc);
        }
    })?;

    Ok(windows)
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
) -> Option<WindowInfo> {
    let class = match conn.read_class(window) {
        Ok(class) => class,
        Err(e) => {
            hatred
                .blacklisted_windows
                .insert(window, format!("read class failed: {:?}", e));
            return None;
        }
    };

    for ignore in &config.ignore {
        if ignore.is_match(&class) {
            return None;
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

    Some(WindowInfo {
        pids,
        class,
        supports_delete,
        title: String::new(), // TODO
    })
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
