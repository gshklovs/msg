# msg

An fzf-style picker for iMessage conversations. Type a name, press Enter, land in
that Messages thread.

## Use

    msg              fuzzy picker in the terminal
    msg index        rebuild the people cache
    msg daemon       stay resident, popup on the global hotkey
    msg install      start the daemon at login (launchd)
    msg uninstall    stop starting the daemon at login

Typing filters; the top match is always selected. Enter opens the conversation,
Esc quits. Arrow keys (or ctrl-n / ctrl-p) move the selection.

## Where the data comes from

Contacts come from the AddressBook databases under
`~/Library/Application Support/AddressBook`, conversations from
`~/Library/Messages/chat.db`. Both are opened read-only. Phone numbers are
normalized to E.164 so a contact and a Messages handle for the same person line
up. The result is cached in `~/.cache/msg/people.json` and rebuilt whenever a
source database changes.

Nothing is written to either database and no message content is read.

## Configuration

`~/.config/msg/config.toml`, one key:

    hotkey = "cmd+shift+m"

A missing file means the default above.

## macOS permissions

Reading `chat.db` and the AddressBook databases needs Full Disk Access for
whatever runs `msg`. In a terminal that is your terminal app. Under the
LaunchAgent it is the `msg` binary itself, so add `~/.cargo/bin/msg` in System
Settings -> Privacy & Security -> Full Disk Access before running `msg install`.

Without it the daemon still starts and serves the last cached people list; it
just cannot refresh it.

The daemon's global hotkey uses a Carbon hotkey registration, which needs no
special permission, but macOS will not deliver it if another app has already
claimed the same combination.

The daemon logs to `~/.cache/msg/daemon.log`.
