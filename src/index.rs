use crate::dictionary::MDictKeywordEntry;
use std::cmp::Ordering;

pub fn normalize_comparable_key(input: &str) -> String {
    input
        .chars()
        .filter(|ch| {
            !ch.is_ascii_whitespace()
                && !matches!(
                    ch,
                    ':' | '.' | ',' | '-' | '_' | '\'' | '"' | '(' | ')' | '#' | '<' | '>' | '!'
                )
        })
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub entry: MDictKeywordEntry,
    pub score: u32,
    pub source: String,
}

impl Ord for SearchHit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| self.entry.lookup_key().cmp(other.entry.lookup_key()))
    }
}

impl PartialOrd for SearchHit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn fuzzy_score(query: &str, key: &str) -> Option<(u32, String)> {
    let query = query.trim().to_lowercase();
    let key_l = key.trim().to_lowercase();
    if query.is_empty() || key_l.is_empty() {
        return None;
    }
    if key_l == query {
        return Some((100, "exact".to_string()));
    }
    if key_l.starts_with(&query) {
        return Some((80, "prefix".to_string()));
    }
    if key_l.contains(&query) {
        return Some((60, "substring".to_string()));
    }
    let mut pos = 0usize;
    for ch in query.chars() {
        let found = key_l[pos..].find(ch)?;
        pos += found + ch.len_utf8();
    }
    Some((30, "subsequence".to_string()))
}
