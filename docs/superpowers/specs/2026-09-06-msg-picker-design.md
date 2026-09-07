# msg: fzf-style iMessage people picker

## Goal
Replace the slow Messages.app "New Message" contact search with an instant fuzzy picker.
One thing: type a name, hit enter, land in that Messages conversation. Nothing else.

## Modes (one binary, `msg`)
- `msg` (tty): ratatui fuzzy picker. Typing filters; top match is always selected; Enter opens it; Esc quits.
- `msg daemon`: resident background process. Global hotkey (default Cmd+Shift+M) shows a small
  centered borderless egui window with the same picker. Enter opens and hides window; Esc hides.
  Process stays alive so popup is instant. `msg install` writes a launchd LaunchAgent plist
  (~/Library/LaunchAgents/net.shklovski.msg.plist) so daemon starts at login; `msg uninstall` removes it.
- `msg index`: rebuild the people cache. Also runs automatically in either mode when the mtime of
  chat.db or the AddressBook db is newer than the cache.

## Data
- Contacts: `~/Library/Application Support/AddressBook/Sources/*/AddressBook-v22.abcddb`
  (also the top-level `~/Library/Application Support/AddressBook/AddressBook-v22.abcddb` if present).
  Tables: ZABCDRECORD (first/last/org name), ZABCDPHONENUMBER (ZFULLNUMBER), ZABCDEMAILADDRESS (ZADDRESS).
  Open read-only with `?mode=ro` URI, copy to temp if locked.
- Chats: `~/Library/Messages/chat.db` read-only. `handle.id` = phone/email; `chat.chat_identifier`,
  `chat.display_name`, `chat_handle_join`, `chat_message_join`, `message.date` (Apple epoch, ns since 2001).
- Normalize phone numbers to E.164 digits for matching contacts <-> handles (strip spaces/punct, add +1 if 10 digits).
- Person entry: display name, handles (Vec), last_message_ts (max across handles), kind = Person | Group.
- Group chats: name = display_name if set, else joined member names (resolved via contacts), handle = chat_identifier.
- Contacts with no chat history are included with last_message_ts = 0.
- Sort by last_message_ts desc, then name. Cache to `~/.cache/msg/people.json` with source mtimes.

## Matching
- `nucleo` crate (fzf-style). Query matches against name and handles. Ranked by nucleo score,
  ties broken by recency. Empty query shows recency order.

## Action on Enter
- Person: pick handle with most recent chat, else first phone, else first email.
  Run `open "imessage://<handle>"` then activate Messages (osascript `tell application "Messages" to activate`).
- Group: `open "imessage://<chat_identifier>"`; if that fails to open the group, fall back to
  osascript: `tell application "Messages" to show chat id "<guid>"`-style lookup via chat.guid.
- Log errors to stderr (tty) or ~/.cache/msg/daemon.log.

## Config
- `~/.config/msg/config.toml`: `hotkey = "cmd+shift+m"`. That's the only key. Missing file = defaults.

## Crates
nucleo, ratatui + crossterm, eframe/egui, global-hotkey, rusqlite (bundled), serde/serde_json, toml, dirs, anyhow.

## Performance targets
- Index build < 200 ms on ~700 chats / few thousand contacts.
- Keystroke-to-render < 16 ms.
- Hotkey-to-window-visible < 50 ms (daemon resident, window pre-created and hidden).

## Testing
- Unit tests: phone normalization, contact<->handle join, ranking (recency + score), config parsing.
- Index test against a fixture sqlite db built in-test (do not depend on real chat.db in tests).
- Manual: `cargo run -- index` then `cargo run` in tty, then `cargo run -- daemon` and press hotkey.

## Not doing
Reading messages, composing/sending, any Messages UI beyond opening a conversation, Hammerspoon.
