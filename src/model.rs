use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Person,
    Group,
}

/// One row in the picker: a contact, or a group chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    /// Phone numbers / emails (people), or the chat_identifier (groups).
    pub handles: Vec<String>,
    /// Apple-epoch-derived unix seconds of the most recent message. 0 = never.
    pub last_message_ts: i64,
    pub kind: Kind,
    /// chat.guid, only set for groups (used by the osascript fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// Handle with the most recent chat activity, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_handle: Option<String>,
}

impl Person {
    /// The handle to hand to `imessage://`.
    ///
    /// Person: most recently used handle, else the first phone number, else the
    /// first handle at all. Group: the chat identifier.
    pub fn target_handle(&self) -> Option<&str> {
        if let Some(h) = self.best_handle.as_deref() {
            if self.handles.iter().any(|x| x == h) {
                return Some(h);
            }
        }
        if let Some(p) = self.handles.iter().find(|h| is_phone(h)) {
            return Some(p);
        }
        self.handles.first().map(|s| s.as_str())
    }
}

pub fn is_phone(handle: &str) -> bool {
    !handle.contains('@')
}

/// Apple's Core Data / Messages epoch is 2001-01-01; `message.date` is
/// nanoseconds since then on modern macOS and seconds on older ones.
pub const APPLE_EPOCH_OFFSET: i64 = 978_307_200;

pub fn apple_time_to_unix(raw: i64) -> i64 {
    if raw == 0 {
        return 0;
    }
    let secs = if raw.abs() > 1_000_000_000_000 {
        raw / 1_000_000_000
    } else {
        raw
    };
    secs + APPLE_EPOCH_OFFSET
}

/// Reduce a phone number or email to a stable key so that contacts and Messages
/// handles for the same person land on each other.
///
/// Emails lowercase. Phone numbers keep digits only, then get a `+1` country
/// code when they look like a bare North American number.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.contains('@') {
        return Some(raw.to_ascii_lowercase());
    }
    let had_plus = raw.trim_start().starts_with('+');
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let normalized = if had_plus {
        format!("+{digits}")
    } else {
        match digits.len() {
            10 => format!("+1{digits}"),
            11 if digits.starts_with('1') => format!("+{digits}"),
            _ => format!("+{digits}"),
        }
    };
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_us_numbers_to_e164() {
        for raw in [
            "555 010 1234",
            "(555) 010-1234",
            "555-010-1234",
            "+1 555 010 1234",
            "15550101234",
        ] {
            assert_eq!(normalize_handle(raw).as_deref(), Some("+15550101234"), "{raw}");
        }
    }

    #[test]
    fn keeps_international_numbers_intact() {
        assert_eq!(
            normalize_handle("+44 20 7946 0000").as_deref(),
            Some("+442079460000")
        );
    }

    #[test]
    fn lowercases_emails_and_rejects_junk() {
        assert_eq!(
            normalize_handle(" Ada@Example.COM ").as_deref(),
            Some("ada@example.com")
        );
        assert_eq!(normalize_handle(""), None);
        assert_eq!(normalize_handle("---"), None);
    }

    #[test]
    fn converts_nanosecond_and_second_apple_times() {
        assert_eq!(apple_time_to_unix(0), 0);
        assert_eq!(apple_time_to_unix(1), APPLE_EPOCH_OFFSET + 1);
        assert_eq!(
            apple_time_to_unix(700_000_000_000_000_000),
            APPLE_EPOCH_OFFSET + 700_000_000
        );
    }

    #[test]
    fn target_handle_prefers_recent_then_phone() {
        let mut p = Person {
            name: "Ada".into(),
            handles: vec!["ada@example.com".into(), "+15550101234".into()],
            last_message_ts: 5,
            kind: Kind::Person,
            guid: None,
            best_handle: Some("ada@example.com".into()),
        };
        assert_eq!(p.target_handle(), Some("ada@example.com"));
        p.best_handle = None;
        assert_eq!(p.target_handle(), Some("+15550101234"));
        p.handles = vec!["ada@example.com".into()];
        assert_eq!(p.target_handle(), Some("ada@example.com"));
    }
}
