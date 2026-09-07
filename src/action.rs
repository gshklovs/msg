use anyhow::{bail, Result};
use std::process::{Command, Stdio};

use crate::model::{Kind, Person};

/// Open a conversation in Messages and bring the app forward.
pub fn open(person: &Person) -> Result<()> {
    let Some(handle) = person.target_handle() else {
        bail!("{} has no phone number or email", person.name);
    };
    let opened = open_url(&format!("imessage://{handle}"));
    match opened {
        Ok(()) => {}
        Err(e) if person.kind == Kind::Group => {
            if let Some(guid) = person.guid.as_deref() {
                show_chat_by_guid(guid)?;
            } else {
                return Err(e);
            }
        }
        Err(e) => return Err(e),
    }
    activate_messages()
}

fn open_url(url: &str) -> Result<()> {
    let status = Command::new("/usr/bin/open")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("`open {url}` exited with {status}");
    }
    Ok(())
}

fn show_chat_by_guid(guid: &str) -> Result<()> {
    osascript(&format!(
        "tell application \"Messages\" to show chat id \"{}\"",
        guid.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

pub fn activate_messages() -> Result<()> {
    osascript("tell application \"Messages\" to activate")
}

fn osascript(script: &str) -> Result<()> {
    let out = Command::new("/usr/bin/osascript").arg("-e").arg(script).output()?;
    if !out.status.success() {
        bail!("osascript failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}
