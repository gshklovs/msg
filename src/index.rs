use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::{apple_time_to_unix, normalize_handle, Kind, Person};

/// A contact as read out of the AddressBook database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub name: String,
    /// Normalized handles.
    pub handles: Vec<String>,
}

/// A conversation as read out of chat.db.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub guid: String,
    pub chat_identifier: String,
    pub display_name: Option<String>,
    /// 43 = group, 45 = one-to-one.
    pub style: i64,
    /// Normalized member handles.
    pub members: Vec<String>,
    /// Unix seconds of the most recent message in this chat.
    pub last_ts: i64,
}

impl Chat {
    pub fn is_group(&self) -> bool {
        self.style == 43 || self.members.len() > 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    /// mtime (unix seconds) of each source db, keyed by path.
    pub sources: HashMap<String, i64>,
    pub people: Vec<Person>,
}

// ---------------------------------------------------------------- paths

pub fn cache_dir() -> Result<PathBuf> {
    let d = dirs::home_dir()
        .context("no home directory")?
        .join(".cache/msg");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// `MSG_CACHE` overrides the people list location (used for demos and tests).
pub fn cache_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("MSG_CACHE") {
        return Ok(PathBuf::from(p));
    }
    Ok(cache_dir()?.join("people.json"))
}

pub fn chat_db_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("no home directory")?
        .join("Library/Messages/chat.db"))
}

/// Every AddressBook database we can find: the per-source ones and the
/// top-level one, when they exist.
pub fn address_book_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let root = home.join("Library/Application Support/AddressBook");
    let mut out = Vec::new();
    let top = root.join("AddressBook-v22.abcddb");
    if top.exists() {
        out.push(top);
    }
    if let Ok(entries) = std::fs::read_dir(root.join("Sources")) {
        for e in entries.flatten() {
            let db = e.path().join("AddressBook-v22.abcddb");
            if db.exists() {
                out.push(db);
            }
        }
    }
    out
}

fn source_paths() -> Vec<PathBuf> {
    let mut v = address_book_paths();
    if let Ok(p) = chat_db_path() {
        if p.exists() {
            v.push(p);
        }
    }
    v
}

fn mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn current_sources() -> HashMap<String, i64> {
    source_paths()
        .into_iter()
        .map(|p| (p.to_string_lossy().into_owned(), mtime(&p)))
        .collect()
}

// ---------------------------------------------------------------- sqlite

/// Open read-only. macOS keeps these databases live, so fall back to a copy in
/// the cache directory when SQLite refuses the original.
fn open_ro(path: &Path) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let uri = format!("file:{}?mode=ro", path.to_string_lossy());
    match Connection::open_with_flags(&uri, flags) {
        Ok(c) => Ok(c),
        Err(_) => {
            let tmp = cache_dir()?.join(format!(
                "copy-{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            std::fs::copy(path, &tmp)
                .with_context(|| format!("copying {} for read-only access", path.display()))?;
            let uri = format!("file:{}?mode=ro", tmp.to_string_lossy());
            Ok(Connection::open_with_flags(&uri, flags)?)
        }
    }
}

/// Contacts from one AddressBook database.
pub fn read_contacts(conn: &Connection) -> Result<Vec<Contact>> {
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT Z_PK, ZFIRSTNAME, ZLASTNAME, ZNICKNAME, ZORGANIZATION FROM ZABCDRECORD",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    for row in rows {
        let (pk, first, last, nick, org) = row?;
        if let Some(name) = compose_name(first, last, nick, org) {
            names.insert(pk, name);
        }
    }

    let mut handles: HashMap<i64, Vec<String>> = HashMap::new();
    for (sql, col) in [
        ("SELECT ZOWNER, ZFULLNUMBER FROM ZABCDPHONENUMBER", "phone"),
        ("SELECT ZOWNER, ZADDRESS FROM ZABCDEMAILADDRESS", "email"),
    ] {
        let _ = col;
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (owner, value) = row?;
            let (Some(owner), Some(value)) = (owner, value) else {
                continue;
            };
            if let Some(h) = normalize_handle(&value) {
                let e = handles.entry(owner).or_default();
                if !e.contains(&h) {
                    e.push(h);
                }
            }
        }
    }

    Ok(names
        .into_iter()
        .map(|(pk, name)| Contact {
            name,
            handles: handles.remove(&pk).unwrap_or_default(),
        })
        .filter(|c| !c.handles.is_empty())
        .collect())
}

fn compose_name(
    first: Option<String>,
    last: Option<String>,
    nick: Option<String>,
    org: Option<String>,
) -> Option<String> {
    let parts: Vec<String> = [first, last]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !parts.is_empty() {
        return Some(parts.join(" "));
    }
    for fallback in [nick, org].into_iter().flatten() {
        let s = fallback.trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Chats, their members, and their most recent message.
pub fn read_chats(conn: &Connection) -> Result<Vec<Chat>> {
    let mut last: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT chat_id, MAX(message_date) FROM chat_message_join GROUP BY chat_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (id, ts) = row?;
            last.insert(id, apple_time_to_unix(ts));
        }
    }

    let mut members: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT chj.chat_id, h.id FROM chat_handle_join chj JOIN handle h ON h.ROWID = chj.handle_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, raw) = row?;
            if let Some(h) = normalize_handle(&raw) {
                let e = members.entry(id).or_default();
                if !e.contains(&h) {
                    e.push(h);
                }
            }
        }
    }

    let mut stmt =
        conn.prepare("SELECT ROWID, guid, chat_identifier, display_name, style FROM chat")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, guid, chat_identifier, display_name, style) = row?;
        out.push(Chat {
            guid,
            chat_identifier: chat_identifier.unwrap_or_default(),
            display_name: display_name.filter(|s| !s.trim().is_empty()),
            style: style.unwrap_or(45),
            members: members.remove(&id).unwrap_or_default(),
            last_ts: last.get(&id).copied().unwrap_or(0),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- assembly

/// Join contacts and chats into the picker's rows.
///
/// A person is one contact, carrying the recency of the most recent chat on any
/// of their handles. Handles that never matched a contact become their own
/// entries so unknown numbers are still reachable. Groups are one entry each.
pub fn assemble(contacts: Vec<Contact>, chats: &[Chat]) -> Vec<Person> {
    // handle -> most recent activity across all chats it appears in
    let mut activity: HashMap<&str, i64> = HashMap::new();
    for c in chats {
        for m in &c.members {
            let e = activity.entry(m.as_str()).or_insert(0);
            *e = (*e).max(c.last_ts);
        }
    }

    let mut people = Vec::new();

    for c in contacts {
        let mut last = 0i64;
        let mut best: Option<(i64, String)> = None;
        for h in &c.handles {
            let ts = activity.get(h.as_str()).copied().unwrap_or(0);
            last = last.max(ts);
            if ts > 0 && best.as_ref().is_none_or(|(b, _)| ts > *b) {
                best = Some((ts, h.clone()));
            }
        }
        people.push(Person {
            name: c.name,
            handles: c.handles,
            last_message_ts: last,
            kind: Kind::Person,
            guid: None,
            best_handle: best.map(|(_, h)| h),
            members: Vec::new(),
            named: false,
        });
    }

    // Contacts we do not have: one entry per one-to-one chat handle.
    let known: std::collections::HashSet<String> = people
        .iter()
        .flat_map(|p| p.handles.iter().cloned())
        .collect();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for c in chats {
        if c.is_group() {
            continue;
        }
        for m in &c.members {
            if known.contains(m) || !seen.insert(m.as_str()) {
                continue;
            }
            people.push(Person {
                name: m.clone(),
                handles: vec![m.clone()],
                last_message_ts: activity.get(m.as_str()).copied().unwrap_or(0),
                kind: Kind::Person,
                guid: None,
                best_handle: Some(m.clone()),
                members: Vec::new(),
                named: false,
            });
        }
    }

    // Groups.
    let by_handle: HashMap<String, String> = people
        .iter()
        .flat_map(|p| p.handles.iter().map(|h| (h.clone(), p.name.clone())))
        .collect();
    for c in chats {
        if !c.is_group() {
            continue;
        }
        let name = match &c.display_name {
            Some(d) => d.clone(),
            None => {
                let mut names: Vec<&str> = c
                    .members
                    .iter()
                    .map(|m| by_handle.get(m).map_or(m.as_str(), |n| n.as_str()))
                    .collect();
                names.sort_unstable();
                names.dedup();
                if names.is_empty() {
                    c.chat_identifier.clone()
                } else {
                    names.join(", ")
                }
            }
        };
        people.push(Person {
            name,
            handles: vec![c.chat_identifier.clone()],
            last_message_ts: c.last_ts,
            kind: Kind::Group,
            guid: Some(c.guid.clone()),
            best_handle: Some(c.chat_identifier.clone()),
            members: c.members.clone(),
            named: c.display_name.is_some(),
        });
    }

    // A group chat can appear under several rows in `chat` (one per service);
    // keep the most recent of each name+identifier pair.
    people.sort_by(|a, b| {
        b.last_message_ts
            .cmp(&a.last_message_ts)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let mut seen_key: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    people.retain(|p| {
        seen_key.insert((
            p.name.to_lowercase(),
            p.target_handle().unwrap_or_default().to_string(),
        ))
    });
    people
}

// ---------------------------------------------------------------- top level

/// Read both databases and build the people list from scratch.
pub fn build() -> Result<Vec<Person>> {
    let mut contacts = Vec::new();
    for db in address_book_paths() {
        match open_ro(&db).and_then(|c| read_contacts(&c)) {
            Ok(mut v) => contacts.append(&mut v),
            Err(e) => eprintln!("msg: skipping {}: {e}", db.display()),
        }
    }
    // The same person can live in several address book sources.
    contacts.sort_by(|a, b| a.name.cmp(&b.name));
    contacts.dedup_by(|a, b| a.name == b.name && a.handles == b.handles);

    let chat_db = chat_db_path()?;
    let mut chats = Vec::new();
    if chat_db.exists() {
        match open_ro(&chat_db).and_then(|c| read_chats(&c)) {
            Ok(v) => chats = v,
            // Usually Full Disk Access. Contacts alone are still useful.
            Err(e) => eprintln!("msg: cannot read {}: {e}", chat_db.display()),
        }
    }

    Ok(assemble(contacts, &chats))
}

/// Rebuild and write the cache.
pub fn rebuild() -> Result<Vec<Person>> {
    let people = build()?;
    if people.is_empty() {
        // Do not replace a good cache with nothing.
        return Ok(people);
    }
    let cache = Cache {
        sources: current_sources(),
        people: people.clone(),
    };
    let path = cache_path()?;
    std::fs::write(&path, serde_json::to_vec(&cache)?)?;
    Ok(people)
}

/// The cached people list, rebuilt first when a source database has changed.
///
/// A rebuild that fails or comes back empty falls back to whatever is cached,
/// so losing read access to the databases degrades to a stale list rather than
/// to nothing.
pub fn load() -> Result<Vec<Person>> {
    let cached = read_cache();
    if let Some(cache) = &cached {
        let pinned = std::env::var_os("MSG_CACHE").is_some();
        if (pinned || cache.sources == current_sources()) && !cache.people.is_empty() {
            return Ok(cache.people.clone());
        }
    }
    match rebuild() {
        Ok(people) if !people.is_empty() => Ok(people),
        result => match cached {
            Some(cache) if !cache.people.is_empty() => {
                eprintln!("msg: using the cached people list; a rebuild found nothing");
                Ok(cache.people)
            }
            _ => result,
        },
    }
}

fn read_cache() -> Option<Cache> {
    let bytes = std::fs::read(cache_path().ok()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(name: &str, handles: &[&str]) -> Contact {
        Contact {
            name: name.into(),
            handles: handles.iter().filter_map(|h| normalize_handle(h)).collect(),
        }
    }

    fn chat(id: &str, style: i64, members: &[&str], last_ts: i64) -> Chat {
        Chat {
            guid: format!("iMessage;-;{id}"),
            chat_identifier: id.into(),
            display_name: None,
            style,
            members: members.iter().filter_map(|h| normalize_handle(h)).collect(),
            last_ts,
        }
    }

    #[test]
    fn joins_contacts_to_handles_across_formats() {
        let people = assemble(
            vec![contact("Ada Lovelace", &["(555) 010-1234"])],
            &[chat("+15550101234", 45, &["+1 555 010 1234"], 900)],
        );
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].name, "Ada Lovelace");
        assert_eq!(people[0].last_message_ts, 900);
        assert_eq!(people[0].target_handle(), Some("+15550101234"));
    }

    #[test]
    fn contacts_without_history_are_kept_at_zero() {
        let people = assemble(vec![contact("Nobody", &["+15550009999"])], &[]);
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].last_message_ts, 0);
    }

    #[test]
    fn unknown_handles_become_their_own_entries() {
        let people = assemble(vec![], &[chat("+15550107777", 45, &["+15550107777"], 500)]);
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].name, "+15550107777");
        assert_eq!(people[0].kind, Kind::Person);
    }

    #[test]
    fn groups_are_named_from_members_when_unnamed() {
        let people = assemble(
            vec![
                contact("Ada Lovelace", &["+15550101234"]),
                contact("Grace Hopper", &["+15550105678"]),
            ],
            &[chat(
                "chat9001",
                43,
                &["+15550101234", "+15550105678"],
                1_000,
            )],
        );
        let group = people.iter().find(|p| p.kind == Kind::Group).unwrap();
        assert_eq!(group.name, "Ada Lovelace, Grace Hopper");
        assert_eq!(group.target_handle(), Some("chat9001"));
        assert_eq!(group.guid.as_deref(), Some("iMessage;-;chat9001"));
        assert!(!group.named);
        assert_eq!(group.members.len(), 2);
    }

    #[test]
    fn display_name_wins_over_member_names() {
        let mut c = chat("chat42", 43, &["+15550101234", "+15550105678"], 10);
        c.display_name = Some("Team Analytical".into());
        let people = assemble(vec![], &[c]);
        assert_eq!(people[0].name, "Team Analytical");
    }

    #[test]
    fn sorted_by_recency_then_name() {
        let people = assemble(
            vec![
                contact("Zoe", &["+15550100001"]),
                contact("Ada", &["+15550100002"]),
                contact("Old Friend", &["+15550100003"]),
            ],
            &[
                chat("+15550100001", 45, &["+15550100001"], 50),
                chat("+15550100002", 45, &["+15550100002"], 50),
                chat("+15550100003", 45, &["+15550100003"], 10),
            ],
        );
        let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Ada", "Zoe", "Old Friend"]);
    }

    // ---- fixture databases ------------------------------------------------

    fn fixture_address_book() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE ZABCDRECORD (Z_PK INTEGER PRIMARY KEY, ZFIRSTNAME TEXT, ZLASTNAME TEXT, ZNICKNAME TEXT, ZORGANIZATION TEXT);
             CREATE TABLE ZABCDPHONENUMBER (Z_PK INTEGER PRIMARY KEY, ZOWNER INTEGER, ZFULLNUMBER TEXT);
             CREATE TABLE ZABCDEMAILADDRESS (Z_PK INTEGER PRIMARY KEY, ZOWNER INTEGER, ZADDRESS TEXT);
             INSERT INTO ZABCDRECORD VALUES (1,'Ada','Lovelace',NULL,NULL),
                                            (2,NULL,NULL,NULL,'Analytical Engines Ltd'),
                                            (3,'Ghost',NULL,NULL,NULL);
             INSERT INTO ZABCDPHONENUMBER VALUES (1,1,'(555) 010-1234'), (2,2,'+1 555 010 5678');
             INSERT INTO ZABCDEMAILADDRESS VALUES (1,1,'Ada@Example.COM');",
        )
        .unwrap();
        c
    }

    #[test]
    fn reads_contacts_from_fixture_db() {
        let mut got = read_contacts(&fixture_address_book()).unwrap();
        got.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(got.len(), 2, "contacts with no handle are dropped");
        assert_eq!(got[0].name, "Ada Lovelace");
        assert!(got[0].handles.contains(&"+15550101234".to_string()));
        assert!(got[0].handles.contains(&"ada@example.com".to_string()));
        assert_eq!(got[1].name, "Analytical Engines Ltd");
    }

    fn fixture_chat_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE chat (ROWID INTEGER PRIMARY KEY, guid TEXT, chat_identifier TEXT, display_name TEXT, style INTEGER);
             CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
             CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
             CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER, message_date INTEGER);
             INSERT INTO chat VALUES (1,'iMessage;-;+15550101234','+15550101234','',45),
                                     (2,'iMessage;+;chat9001','chat9001','Book Club',43);
             INSERT INTO handle VALUES (1,'+15550101234'), (2,'+15550105678');
             INSERT INTO chat_handle_join VALUES (1,1),(2,1),(2,2);
             INSERT INTO chat_message_join VALUES (1,1,100000000000000000),(2,2,200000000000000000);",
        )
        .unwrap();
        c
    }

    #[test]
    fn reads_chats_from_fixture_db() {
        let mut chats = read_chats(&fixture_chat_db()).unwrap();
        chats.sort_by_key(|c| c.style);
        assert_eq!(chats[0].style, 43);
        assert_eq!(chats[0].display_name.as_deref(), Some("Book Club"));
        assert_eq!(chats[0].members.len(), 2);
        assert_eq!(chats[0].last_ts, apple_time_to_unix(200000000000000000));
        assert_eq!(chats[1].style, 45);
        assert!(!chats[1].is_group());
        assert_eq!(chats[1].display_name, None, "blank display_name is dropped");
    }

    #[test]
    fn end_to_end_over_fixture_databases() {
        let contacts = read_contacts(&fixture_address_book()).unwrap();
        let chats = read_chats(&fixture_chat_db()).unwrap();
        let people = assemble(contacts, &chats);
        let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Book Club"));
        assert!(names.contains(&"Ada Lovelace"));
        // Both contacts are in the group, so all three rows carry the group's
        // recency and sort alphabetically against each other.
        assert_eq!(
            names,
            ["Ada Lovelace", "Analytical Engines Ltd", "Book Club"]
        );
        let ts = apple_time_to_unix(200000000000000000);
        assert!(people.iter().all(|p| p.last_message_ts == ts));
    }
}
