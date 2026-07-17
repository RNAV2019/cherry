mod app;
mod apply;
mod daemon;
mod loader;
mod ui;
mod wallpapers;

use std::io::Write;
use std::os::unix::net::UnixStream;

const SOCKET_PATH: &str = "/tmp/cherry.sock";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let toggle = args.iter().any(|a| a == "--toggle");
    let kill = args.iter().any(|a| a == "--kill");

    if kill {
        match send_command(b"kill\n") {
            Ok(()) => return,
            Err(_) => {
                eprintln!("cherry: daemon is not running");
                std::process::exit(1);
            }
        }
    } else if toggle {
        match send_toggle() {
            Ok(()) => return,
            Err(_) => {
                start_daemon_background();
                for _ in 0..20 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if send_toggle().is_ok() {
                        return;
                    }
                }
                eprintln!("cherry: daemon did not start in time");
                std::process::exit(1);
            }
        }
    } else {
        daemon::run_daemon();
    }
}

fn send_toggle() -> std::io::Result<()> {
    send_command(b"toggle\n")
}

fn send_command(cmd: &[u8]) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)?;
    stream.write_all(cmd)?;
    Ok(())
}

fn start_daemon_background() {
    let exe = std::env::current_exe().expect("Cannot find current executable");
    std::process::Command::new(exe)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to start daemon");
}
