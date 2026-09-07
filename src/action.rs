use anyhow::{bail, Result};
use std::process::{Command, Stdio};

use crate::model::{Kind, Person};

/// Open a conversation in Messages and bring the app forward.
///
/// People open through the `imessage://` URL scheme. Groups are harder:
/// Messages has no scripting command to show a chat, and `imessage://chatNNN`
/// starts a new message to nobody. An unnamed group resolves when a new
/// message is addressed to exactly its participants. A named group does not,
/// so it is opened by driving the Messages search field (needs Accessibility).
pub fn open(person: &Person) -> Result<()> {
    if person.kind == Kind::Group {
        return open_group(person);
    }
    let Some(handle) = person.target_handle() else {
        bail!("{} has no phone number or email", person.name);
    };
    open_url(&format!("imessage://{handle}"))?;
    activate_messages()
}

fn open_group(group: &Person) -> Result<()> {
    if !group.named && !group.members.is_empty() {
        open_url(&format!(
            "imessage://open?addresses={}",
            group.members.join(",")
        ))?;
        return activate_messages();
    }
    open_group_by_search(&group.name)
}

/// Focus the Messages search field, type the name, pick the first suggestion.
fn open_group_by_search(name: &str) -> Result<()> {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    osascript(&format!(
        r#"tell application "Messages" to activate
delay 0.2
tell application "System Events" to tell process "Messages"
    key code 53
    keystroke "f" using command down
    delay 0.15
    keystroke "a" using command down
    keystroke "{escaped}"
    delay 0.7
    key code 125
    delay 0.1
    key code 36
end tell"#
    ))
    .map_err(|e| anyhow::anyhow!("{e}; opening a named group needs Accessibility access for msg"))
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
