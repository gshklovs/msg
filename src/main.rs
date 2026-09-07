mod action;
mod config;
mod index;
mod matcher;
mod model;
mod tui;

use anyhow::Result;
use std::io::IsTerminal;

const USAGE: &str = "\
msg - instant iMessage conversation picker

  msg              fuzzy picker in the terminal
  msg index        rebuild the people cache
  msg daemon       stay resident, popup on the global hotkey
  msg install      start the daemon at login (launchd)
  msg uninstall    stop starting the daemon at login
  msg --help       this text
";

fn main() {
    if let Err(e) = run() {
        eprintln!("msg: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => pick(),
        Some("index") => {
            let start = std::time::Instant::now();
            let people = index::rebuild()?;
            let groups = people
                .iter()
                .filter(|p| p.kind == model::Kind::Group)
                .count();
            println!(
                "indexed {} people and {} groups in {} ms -> {}",
                people.len() - groups,
                groups,
                start.elapsed().as_millis(),
                index::cache_path()?.display()
            );
            Ok(())
        }
        Some("daemon") => daemon(),
        Some("install") => install(),
        Some("uninstall") => uninstall(),
        Some("--help" | "-h" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            eprint!("unknown command `{other}`\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

fn pick() -> Result<()> {
    let people = index::load()?;
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("not a terminal; try `msg daemon` or run msg from a tty");
    }
    if let Some(person) = tui::run(people)? {
        action::open(&person)?;
    }
    Ok(())
}

fn daemon() -> Result<()> {
    anyhow::bail!("daemon mode is not wired up yet")
}

fn install() -> Result<()> {
    anyhow::bail!("install is not wired up yet")
}

fn uninstall() -> Result<()> {
    anyhow::bail!("uninstall is not wired up yet")
}
