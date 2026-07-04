use std::io;

mod copy;
mod gh;
mod model;
mod search;
mod ui;

const HELP: &str = "\
issue-browser - a tmux-popup TUI for GitHub issues

Usage:
  issue-browser            Launch the picker (intended via `tmux popup -E`)
  issue-browser --version  Print version and exit
  issue-browser --help     Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind i display-popup -E -B -w 84 -h 60% \"exec issue-browser\"";

fn main() -> io::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("issue-browser {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            other => {
                eprintln!("issue-browser: unknown argument '{other}'\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }
    println!("issue-browser scaffold OK");
    Ok(())
}
