use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use crate::model::Person;

/// fzf-style ranking over a fixed people list.
pub struct Ranker {
    matcher: Matcher,
}

impl Default for Ranker {
    fn default() -> Self {
        Self::new()
    }
}

impl Ranker {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Indices into `people`, best match first.
    ///
    /// An empty query keeps the caller's order, which is recency. Otherwise the
    /// best nucleo score over the name and every handle wins, with recency
    /// breaking ties.
    pub fn rank(&mut self, people: &[Person], query: &str) -> Vec<usize> {
        if query.trim().is_empty() {
            return (0..people.len()).collect();
        }
        let pattern = Pattern::parse(query.trim(), CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();
        let mut scored: Vec<(u32, usize, i64, usize)> = Vec::new();
        for (i, p) in people.iter().enumerate() {
            let mut best = None;
            for hay in std::iter::once(&p.name).chain(p.handles.iter()) {
                buf.clear();
                let utf32 = nucleo_matcher::Utf32Str::new(hay, &mut buf);
                if let Some(score) = pattern.score(utf32, &mut self.matcher) {
                    best = Some(best.map_or(score, |b: u32| b.max(score)));
                }
            }
            if let Some(score) = best {
                scored.push((score, p.name.chars().count(), p.last_message_ts, i));
            }
        }
        // Equal scores: the shorter name is the tighter match (fzf's rule), so
        // "Grace Hopper" beats the group that merely contains her. Then recency.
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| b.2.cmp(&a.2))
                .then(a.3.cmp(&b.3))
        });
        scored.into_iter().map(|(_, _, _, i)| i).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    fn person(name: &str, handles: &[&str], ts: i64) -> Person {
        Person {
            name: name.into(),
            handles: handles.iter().map(|s| s.to_string()).collect(),
            last_message_ts: ts,
            kind: Kind::Person,
            guid: None,
            best_handle: None,
            members: Vec::new(),
            named: false,
        }
    }

    fn names<'a>(people: &'a [Person], idx: &[usize]) -> Vec<&'a str> {
        idx.iter().map(|i| people[*i].name.as_str()).collect()
    }

    #[test]
    fn empty_query_keeps_recency_order() {
        let people = vec![person("Ada", &[], 10), person("Grace", &[], 5)];
        let idx = Ranker::new().rank(&people, "  ");
        assert_eq!(idx, vec![0, 1]);
    }

    #[test]
    fn fuzzy_subsequence_matches_and_nonmatches_drop_out() {
        let people = vec![
            person("Ada Lovelace", &[], 1),
            person("Grace Hopper", &[], 2),
            person("Alan Turing", &[], 3),
        ];
        let idx = Ranker::new().rank(&people, "adlv");
        assert_eq!(names(&people, &idx), vec!["Ada Lovelace"]);
    }

    #[test]
    fn recency_breaks_score_ties() {
        let people = vec![
            person("Ada Lovelace", &[], 1),
            person("Ada Lovelace", &[], 99),
        ];
        let idx = Ranker::new().rank(&people, "ada");
        assert_eq!(idx[0], 1, "the more recent identical match ranks first");
    }

    #[test]
    fn handles_are_searchable() {
        let people = vec![
            person("Ada Lovelace", &["+15550101234"], 1),
            person("Grace Hopper", &["+15550109999"], 2),
        ];
        let idx = Ranker::new().rank(&people, "1234");
        assert_eq!(names(&people, &idx), vec!["Ada Lovelace"]);
    }

    #[test]
    fn exact_prefix_outranks_scattered_match() {
        let people = vec![
            person("Alexandra Bell Carter", &[], 1),
            person("Abe", &[], 1),
        ];
        let idx = Ranker::new().rank(&people, "abe");
        assert_eq!(names(&people, &idx)[0], "Abe");
    }

    #[test]
    fn shorter_name_wins_ties_over_recency() {
        let people = vec![
            person("Ada Lovelace, Grace Hopper, Alan Turing", &["chat1"], 900),
            person("Grace Hopper", &["+15550100001"], 100),
        ];
        let ranked = Ranker::new().rank(&people, "grac");
        assert_eq!(ranked[0], 1);
    }
}
