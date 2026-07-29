use crate::entry::Entry;

pub struct SearchOptions {
    pub all: bool,
    pub limit: Option<usize>,
}

/// A parsed search term (§3): either an `@tag` requiring a full-word
/// match against the entry's tag list, or a plain term matched as a
/// case-insensitive substring anywhere in the entry (timestamp + body).
enum Term {
    Tag(String),
    Substring(String),
}

fn parse_terms(query: &str) -> Vec<Term> {
    query
        .split_whitespace()
        .map(|tok| tok.replace('+', " "))
        .map(|term| {
            if term.starts_with('@') && term.len() > 1 {
                Term::Tag(term.to_lowercase())
            } else {
                Term::Substring(term.to_lowercase())
            }
        })
        .collect()
}

fn term_matches(term: &Term, entry: &Entry, haystack: &str) -> bool {
    match term {
        // Tags are no longer stored separately (§2.1) -- `entry.tags()`
        // recovers them by scanning the body for `@\S+` tokens, wherever
        // they happen to appear.
        Term::Tag(t) => entry.tags().iter().any(|tag| tag.to_lowercase() == *t),
        Term::Substring(s) => haystack.contains(s.as_str()),
    }
}

/// Returns entries matching `query` under the given options, in the
/// order they appear in `entries`, truncated to `opts.limit` if set.
pub fn search<'a>(entries: &'a [Entry], query: &str, opts: &SearchOptions) -> Vec<&'a Entry> {
    let terms = parse_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<&Entry> = entries
        .iter()
        .filter(|e| {
            let haystack = e.render().to_lowercase();
            if opts.all {
                terms.iter().all(|t| term_matches(t, e, &haystack))
            } else {
                terms.iter().any(|t| term_matches(t, e, &haystack))
            }
        })
        .collect();

    if let Some(limit) = opts.limit {
        matches.truncate(limit);
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ts(y: i32, mo: u32, d: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    /// Builds a test entry with `tags` appended as a trailing line, the
    /// same shape `-t/--tags` produces (§2.1). Tags aren't a separate
    /// field anymore -- they're just `@word` tokens in the body.
    fn e(tags: &[&str], body: &str) -> Entry {
        let full_body = if tags.is_empty() {
            body.to_string()
        } else {
            format!("{body}\n{}", tags.join(" "))
        };
        Entry::new(ts(2026, 1, 1), full_body)
    }

    fn opts(all: bool, limit: Option<usize>) -> SearchOptions {
        SearchOptions { all, limit }
    }

    #[test]
    fn or_mode_matches_any_term() {
        let entries = vec![e(&[], "fm radio"), e(&[], "linux kernel"), e(&[], "neither")];
        let results = search(&entries, "radio kernel", &opts(false, None));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn and_mode_requires_all_terms() {
        let entries = vec![
            e(&[], "fm radio broadcast"),
            e(&[], "fm radio"),
            e(&[], "radio broadcast"),
        ];
        let results = search(&entries, "fm broadcast", &opts(true, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].body, "fm radio broadcast");
    }

    #[test]
    fn plus_joins_words_into_one_substring_term() {
        let entries = vec![e(&[], "reading about linux kernel internals"), e(&[], "unrelated")];
        let results = search(&entries, "linux+kernel", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn substring_matching_is_broad_by_design() {
        let entries = vec![e(&[], "the weather this month")];
        let results = search(&entries, "th", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn tag_term_requires_full_word_match_not_substring() {
        let entries = vec![e(&["@bp"], "reading"), e(&["@bph"], "other")];
        let results = search(&entries, "@bp", &opts(false, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tags(), vec!["@bp"]);
    }

    #[test]
    fn tag_term_match_is_case_insensitive() {
        let entries = vec![e(&["@BP"], "reading")];
        let results = search(&entries, "@bp", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn plain_term_matches_inside_tag_text_via_substring() {
        // Non-@ terms are ordinary substrings over the whole rendered
        // entry (timestamp line + body), which includes the tag line.
        let entries = vec![e(&["@bp"], "reading")];
        let results = search(&entries, "bp", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn limit_caps_number_of_results() {
        let entries = vec![e(&[], "match one"), e(&[], "match two"), e(&[], "match three")];
        let results = search(&entries, "match", &opts(false, Some(2)));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_query_matches_nothing() {
        let entries = vec![e(&[], "anything")];
        let results = search(&entries, "   ", &opts(false, None));
        assert!(results.is_empty());
        let results = search(&entries, "   ", &opts(true, None));
        assert!(results.is_empty());
    }
}
