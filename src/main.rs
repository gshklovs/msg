mod action;
mod config;
mod draw;
mod gui;
mod index;
mod launchd;
mod matcher;
mod model;
mod recency;
mod theme;
mod tui;

use anyhow::Result;
use std::io::IsTerminal;

const USAGE: &str = "\
msg - instant iMessage conversation picker

  msg              fuzzy picker in the terminal
  msg index        rebuild the people cache
  msg open QUERY   open the best match for QUERY without a picker
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
        Some("open") => {
            let query: Vec<String> = std::env::args().skip(2).collect();
            if query.is_empty() {
                anyhow::bail!("usage: msg open QUERY");
            }
            open_query(&query.join(" "))
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

fn open_query(query: &str) -> Result<()> {
    let people = index::load()?;
    let ranked = matcher::Ranker::new().rank(&people, query);
    let Some(&i) = ranked.first() else {
        anyhow::bail!("nothing matches `{query}`");
    };
    println!("opening {}", tui::label(&people[i]));
    action::open(&people[i])
}

fn daemon() -> Result<()> {
    let cfg = config::Config::load()?;
    log(&format!(
        "daemon starting, hotkey {}, theme {}",
        cfg.hotkey, cfg.theme
    ));
    gui::daemon(&cfg.hotkey, cfg.theme, log)
}

/// Append a line to ~/.cache/msg/daemon.log, and to stderr when attached.
fn log(line: &str) {
    eprintln!("msg: {line}");
    if let Ok(dir) = index::cache_dir() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("daemon.log"))
        {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
    }
}

fn install() -> Result<()> {
    let path = launchd::install()?;
    println!("installed {}", path.display());
    println!("the daemon is running; press your hotkey to try it");
    Ok(())
}

fn uninstall() -> Result<()> {
    let path = launchd::uninstall()?;
    println!("removed {}", path.display());
    Ok(())
}

