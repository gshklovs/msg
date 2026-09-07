use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub const LABEL: &str = "net.shklovski.msg";

pub fn plist_path() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("no home directory")?
        .join("Library/LaunchAgents");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{LABEL}.plist")))
}

/// A LaunchAgent that keeps `msg daemon` running from login onward.
pub fn plist(binary: &str, log_dir: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>StandardOutPath</key>
    <string>{log_dir}/daemon.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/daemon.log</string>
</dict>
</plist>
"#
    )
}

pub fn install() -> Result<PathBuf> {
    let binary = std::env::current_exe()?;
    let path = plist_path()?;
    let log_dir = crate::index::cache_dir()?;
    std::fs::write(
        &path,
        plist(&binary.to_string_lossy(), &log_dir.to_string_lossy()),
    )?;
    let _ = bootout();
    let uid = unsafe { libc_getuid() };
    let out = Command::new("/bin/launchctl")
        .args(["bootstrap", &format!("gui/{uid}")])
        .arg(&path)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "launchctl bootstrap failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(path)
}

pub fn uninstall() -> Result<PathBuf> {
    let path = plist_path()?;
    let _ = bootout();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(path)
}

fn bootout() -> Result<()> {
    let uid = unsafe { libc_getuid() };
    Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}/{LABEL}")])
        .output()?;
    Ok(())
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_is_well_formed_and_points_at_the_binary() {
        let p = plist("/Users/someone/.cargo/bin/msg", "/Users/someone/.cache/msg");
        assert!(p.starts_with("<?xml"));
        assert!(p.contains("<string>net.shklovski.msg</string>"));
        assert!(p.contains("<string>/Users/someone/.cargo/bin/msg</string>"));
        assert!(p.contains("<string>daemon</string>"));
        assert!(p.contains("<string>/Users/someone/.cache/msg/daemon.log</string>"));
        assert_eq!(p.matches("<key>").count(), p.matches("</key>").count());
    }

    #[test]
    fn plist_lands_in_launch_agents() {
        let p = plist_path().unwrap();
        assert!(p.ends_with("Library/LaunchAgents/net.shklovski.msg.plist"), "{p:?}");
    }
}
