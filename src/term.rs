use std::io;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::sync::mpsc;
use std::thread;

use anyhow::Error;
use nix::sys::termios;

pub struct StdOutTermios {
    fd: RawFd,
    original: termios::Termios,
}

impl StdOutTermios {
    pub fn with_setup<F>(func: F) -> Result<StdOutTermios, Error>
    where
        F: FnOnce(&mut termios::Termios),
    {
        let out = io::stdout();
        let fd = out.as_raw_fd();
        let original = termios::tcgetattr(fd)?;
        let mut updated = original.clone();
        func(&mut updated);
        termios::tcsetattr(fd, termios::SetArg::TCSANOW, &updated)?;
        Ok(StdOutTermios { fd, original })
    }
}

impl Drop for StdOutTermios {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(self.fd, termios::SetArg::TCSANOW, &self.original);
    }
}

pub fn async_stdin() -> mpsc::Receiver<u8> {
    let (tx, rx) = mpsc::sync_channel(0);
    thread::spawn(move || {
        loop {
            let mut buf = [0u8; 16];
            let found = io::stdin().read(&mut buf).unwrap();
            if 0 == found {
                // implicit EOF
                break;
            }

            let buf = &buf[..found];
            for byte in buf {
                if 4 == *byte {
                    // We're assuming this is a ctrl+d, exit now
                    // TODO: presumably this can occur in the middle of other streams
                    // TODO: doesn't matter too much for the "ask for a single letter" case
                    break;
                }

                tx.send(*byte).unwrap();
            }
        }
    });
    rx
}
