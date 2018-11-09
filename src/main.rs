extern crate dirs;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
extern crate nix;
extern crate regex;
extern crate terminal_size;
extern crate toml;
extern crate wcwidth;
extern crate xcb;

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::io;
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

mod config;
mod shrinky;
mod term;
mod x;

use failure::Error;
use nix::sys::signal;
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
    let mut start = true;

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
        seen_pids.retain(|&pid, _| {
            if kill(pid, None).unwrap_or(false) {
                true
            } else {
                meaningful_change = true;
                false
            }
        });

        if seen_windows.is_empty() && seen_pids.is_empty() {
            println!(); // end of the prompt
            println!("No applications found, exiting.");
            break;
        }

        if start {
            for (window, info) in &seen_windows {
                if config::any_apply(&info.class, &config.on_start_ask) {
                    conn.delete_window(window)?;
                }
            }

            for (_window, info) in &seen_windows {
                if config::any_apply(&info.class, &config.on_start_tell) {
                    for &pid in &info.pids {
                        kill(pid, Some(signal::SIGTERM))?;
                    }
                }
            }

            for (_window, info) in &seen_windows {
                if config::any_apply(&info.class, &config.on_start_kill) {
                    for &pid in &info.pids {
                        kill(pid, Some(signal::SIGKILL))?;
                    }
                }
            }

            start = false;
        }

        if meaningful_change {
            println!(); // end of the prompt
            println!();
            let width_budget = terminal_size::terminal_size()
                .map(|(w, _h)| w.0)
                .unwrap_or(80) as usize;

            let mut windows = seen_windows
                .values()
                .cloned()
                .map(|info| (if info.pids.is_empty() { '📪' } else { '📫' }, info))
                .collect::<Vec<_>>();

            windows.extend(seen_pids.values().cloned().map(|info| ('📭', info)));

            windows.sort_by_key(|(_status, info)| (info.class.to_string(), info.title.to_string()));
            for (status, info) in windows {
                let used = 3 + info.class.len() + 3;

                let title = if used < width_budget {
                    shrinky::shorten_string_to(&info.title, width_budget - used)
                } else {
                    &info.title
                };

                if title.is_empty() {
                    println!("{} {}", status, info.class)
                } else {
                    println!("{} {} - {}", status, info.class, title)
                }
            }

            action_prompt()?;
        }

        match stdin.recv_timeout(Duration::from_millis(50)) {
            Ok(b'a') | Ok(b'd') /* [d]elete */ => {
                println!();
                println!("Asking everyone to quit.");
                for (window, _info) in &seen_windows {
                    conn.delete_window(window)?;
                }
            }
            Ok(b't') => {
                println!();
                println!("Telling everyone to quit.");
                kill_all(&mut seen_windows, &mut seen_pids, signal::SIGTERM)?;
            }
            Ok(b'k') => {
                println!();
                println!("Quitting everyone.");
                kill_all(&mut seen_windows, &mut seen_pids, signal::SIGKILL)?;
            }
            Ok(b'q') => {
                println!();
                println!("User asked, exiting.");
                break 'app;
            }
            Ok(other) => {
                println!();
                println!("unsupported command: {:?}", other as char);
                action_prompt()?;
            }
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

fn kill_all(
    seen_windows: &mut HashMap<XWindow, WindowInfo>,
    seen_pids: &mut HashMap<u32, WindowInfo>,
    sig: signal::Signal,
) -> Result<bool, Error> {
    let mut did_anything = false;
    for (_window, info) in seen_windows {
        for pid in &info.pids {
            did_anything |= kill(*pid, Some(sig))?;
        }
    }
    for pid in seen_pids.keys() {
        did_anything |= kill(*pid, Some(sig))?;
    }

    action_prompt()?;

    Ok(did_anything)
}

fn action_prompt() -> Result<(), Error> {
    print!("Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit? ");
    io::stdout().flush()?;
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

    if config::any_apply(&class, &config.ignore) {
        return None;
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

    let title = match conn.read_title(window) {
        Ok(val) => val,
        Err(_) => {
            // TODO: don't totally ignore
            String::new()
        }
    };

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
        title,
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
