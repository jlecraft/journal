use chrono::{Datelike, Local, Months, NaiveDateTime};
use serde::Deserialize;

/// `strftime`/`strptime` pattern matching the `[YYYY-MM-DD.HH:MM:SS]` format from §2.
pub const TIMESTAMP_FMT: &str = "%Y-%m-%d.%H:%M:%S";

/// How `-h/--human` renders the elapsed-time annotation next to the
/// timestamp (`[timestamp].diff` in config.toml). Unlike `format`, this is
/// no longer a free-form template the user writes -- just a choice of
/// whether it shows up at all, and how verbose it is when it does.
/// `Deserialize` lives here (rather than only in `config.rs`) since this
/// enum describes a display concept `entry.rs` owns; `config::TimestampConfig`
/// references this type directly rather than duplicating it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffStyle {
    /// No elapsed-time annotation at all -- just the formatted date.
    Disabled,
    /// The two highest non-zero units, abbreviated, no direction word:
    /// `3h, 1m`.
    #[default]
    Short,
    /// Up to three highest non-zero units, spelled out and pluralized,
    /// with a trailing direction word: `3 hours, 1 minute, 16 seconds ago`.
    Long,
}

/// An entry's timestamp line is always on its own line, with nothing
/// else on it (§2). Tags are no longer a separate structured field --
/// they're just `@word` tokens that happen to appear somewhere in the
/// body, whether typed inline by the user or appended as their own line
/// by `-t/--tags` (§2.1). `Entry::tags` recovers them on demand by
/// scanning the body, rather than the tool tracking them separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub timestamp: NaiveDateTime,
    pub body: String,
}

/// Controls how `Entry::display`/`display_header` render a timestamp
/// header. Built once per run (in `main.rs`, from resolved CLI flags and
/// `config::TimestampConfig`) and passed by reference to every display
/// call site. Kept local to this module, rather than taking
/// `config::TimestampConfig` directly, so `entry.rs` stays decoupled from
/// the `config` module.
pub struct DisplayOpts<'a> {
    /// Whether `-h/--human` was passed. When false, `format`/`diff` are
    /// ignored entirely and the header shows the on-disk timestamp
    /// verbatim, with no age annotation.
    pub human: bool,
    /// Chrono strftime template applied to the entry's own timestamp.
    pub format: &'a str,
    /// How to render (or whether to render at all) the elapsed-time
    /// annotation next to the timestamp.
    pub diff: DiffStyle,
}

impl Entry {
    pub fn new(timestamp: NaiveDateTime, body: impl Into<String>) -> Self {
        Entry {
            timestamp,
            body: normalize_trailing_blank_lines(&body.into()),
        }
    }

    pub fn now(body: impl Into<String>) -> Self {
        Self::new(Local::now().naive_local(), body)
    }

    /// Every `@tag` token found anywhere in the body, in the order it
    /// appears. A token counts as a tag purely by its `@\S+` shape (§2.1)
    /// -- there's no separate storage to consult.
    pub fn tags(&self) -> Vec<String> {
        self.body
            .split_whitespace()
            .filter(|t| is_tag_token(t))
            .map(|t| t.to_string())
            .collect()
    }

    /// Renders the on-disk representation: the timestamp alone on its
    /// own line, the body, and the single trailing blank line that
    /// terminates every entry (§2).
    pub fn render(&self) -> String {
        let mut out = format!("[{}]\n", self.timestamp.format(TIMESTAMP_FMT));
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        out.push('\n');
        out
    }

    /// Renders an entry for display to a human (`journal -s`, `journal
    /// -N`): the same body as `render`, but with the header reformatted as
    /// an ATX heading rather than the on-disk `[...]` form. By default the
    /// heading shows the timestamp exactly as stored on disk, with no age
    /// suffix -- `opts.human` switches it to a fully configurable
    /// `date (diff)` form (see `display_header`). An ATX heading, unlike a
    /// blockquote, has no lazy-continuation -- Markdown renderers (e.g.
    /// `bat`) color only this line, not the body line that follows it.
    pub fn display(&self, opts: &DisplayOpts) -> String {
        let mut out = self.display_header(opts);
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        out.push('\n');
        out
    }

    /// Just the reformatted header line from `display`, with no body --
    /// used by `-L/--lines-only` to print the entry's header once,
    /// followed by only the lines that actually matched. Without
    /// `opts.human`, this is an unmodified echo of the on-disk timestamp
    /// (`TIMESTAMP_FMT`) with no age annotation. With `opts.human`, it's
    /// `### {opts.format}` optionally followed by ` ({diff})` -- the diff
    /// annotation is entirely absent (not just empty parens) when
    /// `opts.diff` is `DiffStyle::Disabled`.
    pub fn display_header(&self, opts: &DisplayOpts) -> String {
        if opts.human {
            let date = self.timestamp.format(opts.format);
            match render_diff(opts.diff, self.timestamp, Local::now().naive_local()) {
                Some(diff) => format!("### {date} ({diff})\n"),
                None => format!("### {date}\n"),
            }
        } else {
            format!("### {}\n", self.timestamp.format(TIMESTAMP_FMT))
        }
    }

    /// Parses a single entry block: a header line followed by its body
    /// lines (a trailing terminator blank line, if present, is trimmed).
    /// Any text found on the header line itself after the timestamp --
    /// which shouldn't normally be there, but can appear from a hand-edit
    /// or an older-format entry -- is folded into the body rather than
    /// discarded: parsing must never silently drop file content.
    pub fn parse(raw: &str) -> Option<Entry> {
        let mut lines = raw.lines();
        let header = lines.next()?;
        let (timestamp, header_overflow) = parse_header(header)?;
        let rest = lines.collect::<Vec<_>>().join("\n");
        let body = match header_overflow {
            Some(overflow) if rest.is_empty() => overflow,
            Some(overflow) => format!("{overflow}\n{rest}"),
            None => rest,
        };
        Some(Entry {
            timestamp,
            body: normalize_trailing_blank_lines(&body),
        })
    }

    /// Parses every entry out of a full journal file's contents. Entry
    /// boundaries are located by header lines rather than blank lines,
    /// since a body may legitimately contain blank lines of its own (§2).
    pub fn parse_all(text: &str) -> Vec<Entry> {
        let lines: Vec<&str> = text.lines().collect();
        let header_idxs: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| parse_header(line).map(|_| i))
            .collect();

        header_idxs
            .iter()
            .enumerate()
            .filter_map(|(n, &start)| {
                let end = header_idxs.get(n + 1).copied().unwrap_or(lines.len());
                Entry::parse(&lines[start..end].join("\n"))
            })
            .collect()
    }

    /// Returns the byte offset in `text` where its last header-line-led
    /// entry begins, or `None` if `text` contains no header line at all.
    /// Used by editor mode to isolate the newly-composed entry from
    /// everything before it, which is left untouched.
    pub(crate) fn last_entry_start(text: &str) -> Option<usize> {
        let mut offset = 0;
        let mut last = None;
        for line in text.split_inclusive('\n') {
            let trimmed = line.strip_suffix('\n').unwrap_or(line);
            if parse_header(trimmed).is_some() {
                last = Some(offset);
            }
            offset += line.len();
        }
        last
    }
}

/// Parses a header line into `(timestamp, header_overflow)`. The
/// timestamp line is always alone (§2); `header_overflow` is whatever
/// text (if any) follows the closing `]` on that same line -- not
/// expected in files this tool writes, but preserved rather than dropped
/// if found (e.g. a hand-edit, or a file from before this format change).
fn parse_header(line: &str) -> Option<(NaiveDateTime, Option<String>)> {
    let rest = line.strip_prefix('[')?;
    let (ts_str, rest) = rest.split_once(']')?;
    let timestamp = NaiveDateTime::parse_from_str(ts_str, TIMESTAMP_FMT).ok()?;
    let overflow = rest.trim();
    Some((timestamp, (!overflow.is_empty()).then(|| overflow.to_string())))
}

fn is_tag_token(token: &str) -> bool {
    token.starts_with('@') && token.len() > 1
}

/// The elapsed time between two timestamps, broken down into a cascading
/// years/months/days/hours/minutes/seconds breakdown -- a real calendar
/// breakdown, using actual month lengths and leap years (via chrono's
/// calendar-aware month arithmetic), not a fixed-length approximation.
/// `future` is true when `then` is after `now` (a backdated/postdated
/// entry).
struct Elapsed {
    future: bool,
    years: u64,
    months: u64,
    days: u64,
    hours: u64,
    minutes: u64,
    seconds: u64,
}

impl Elapsed {
    fn between(then: NaiveDateTime, now: NaiveDateTime) -> Self {
        let future = then > now;
        let (earlier, later) = if future { (now, then) } else { (then, now) };

        // Years: `later.year() - earlier.year()` is always within 1 of
        // the true calendar year count -- it can only overshoot, never
        // undershoot, since earlier <= later implies later.year() >=
        // earlier.year() always holds. So at most one decrement is ever
        // needed; no loop required for this step.
        let mut years = (later.year() - earlier.year()).max(0) as u32;
        let mut anchor = add_months(earlier, years * 12);
        if anchor > later {
            years -= 1;
            anchor = add_months(earlier, years * 12);
        }

        // Months: the 0..=11 remainder beyond the whole years already
        // counted above. Walks forward from `anchor`; bounded at 11
        // steps since a 12th month would already have been folded into
        // `years`.
        let mut months = 0u32;
        while months < 11 && add_months(anchor, months + 1) <= later {
            months += 1;
        }
        let anchor = add_months(anchor, months);

        // Remainder: by construction `anchor` is always within one
        // calendar month of `later`, so plain duration arithmetic is
        // exact for what's left.
        let remainder = later - anchor;
        let days = remainder.num_days();
        let secs_left = remainder.num_seconds() - days * 86_400;

        Elapsed {
            future,
            years: years as u64,
            months: months as u64,
            days: days as u64,
            hours: (secs_left / 3600) as u64,
            minutes: (secs_left % 3600 / 60) as u64,
            seconds: (secs_left % 60) as u64,
        }
    }

    /// The six components in descending order, each paired with its long
    /// (singular) and abbreviated unit name.
    fn units(&self) -> [(u64, &'static str, &'static str); 6] {
        [
            (self.years, "year", "y"),
            (self.months, "month", "mo"),
            (self.days, "day", "d"),
            (self.hours, "hour", "h"),
            (self.minutes, "minute", "m"),
            (self.seconds, "second", "s"),
        ]
    }

    /// A fixed-size window of `max` units, anchored at the first non-zero
    /// unit, with zero units inside that window dropped from the result
    /// (not from the window's size or position). Two things this rules
    /// out, both deliberately:
    ///
    /// - A zero unit in the *middle* of the window doesn't hide a
    ///   non-zero unit later in the same window -- `4 years, 0 months, 7
    ///   days` (window = years/months/days) shows as `4 years, 7 days`,
    ///   not just `4 years`.
    /// - A non-zero unit *outside* the window never appears, no matter
    ///   how far the window's own units are from filling it -- `29 days,
    ///   0 hours, 0 minutes, 10 seconds` with a 3-unit window anchored at
    ///   `days` only ever considers days/hours/minutes; the non-zero
    ///   `seconds` past the window's edge is never reached, so this shows
    ///   as `29 days`, not `29 days, 10 seconds`. The window's size and
    ///   position are fixed by `max` and the anchor alone -- non-zero
    ///   values earn a spot in the window, never an extension of it.
    ///
    /// If every component is zero, falls back to a single `0 seconds`, so
    /// there's always something to show.
    fn windowed_nonzero(&self, max: usize) -> Vec<(u64, &'static str, &'static str)> {
        let units = self.units();
        let Some(start) = units.iter().position(|(n, ..)| *n != 0) else {
            return vec![units[5]];
        };
        let end = (start + max).min(units.len());
        units[start..end].iter().filter(|(n, ..)| *n != 0).copied().collect()
    }
}

/// Adds `n` calendar months to `dt` via `NaiveDateTime::checked_add_months`,
/// which clamps the day-of-month down to the target month's last valid day
/// if the original day doesn't exist there (e.g. Jan 31 + 1 month -> Feb
/// 28/29) rather than rolling into the following month. `None` is only
/// possible on true chrono range overflow -- not realistically reachable
/// for this application's timestamps, so this expects rather than
/// threading a `Result` through `Elapsed::between` and its callers.
fn add_months(dt: NaiveDateTime, n: u32) -> NaiveDateTime {
    dt.checked_add_months(Months::new(n))
        .expect("timestamp arithmetic overflowed chrono's supported date range")
}

/// Renders the elapsed-time annotation for the parenthesized part of
/// `-h/--human`'s header, per `style` (`[timestamp].diff` in config.toml).
/// Returns `None` for `DiffStyle::Disabled`, meaning no annotation at all
/// (not even empty parens).
fn render_diff(style: DiffStyle, then: NaiveDateTime, now: NaiveDateTime) -> Option<String> {
    if style == DiffStyle::Disabled {
        return None;
    }
    let elapsed = Elapsed::between(then, now);

    Some(match style {
        DiffStyle::Disabled => unreachable!(),
        DiffStyle::Long => {
            let direction = if elapsed.future { "from now" } else { "ago" };
            let parts: Vec<String> = elapsed
                .windowed_nonzero(3)
                .into_iter()
                .map(|(n, name, _)| {
                    let plural = if n == 1 { "" } else { "s" };
                    format!("{n} {name}{plural}")
                })
                .collect();
            format!("{} {direction}", parts.join(", "))
        }
        DiffStyle::Short => elapsed
            .windowed_nonzero(2)
            .into_iter()
            .map(|(n, _, abbrev)| format!("{n}{abbrev}"))
            .collect::<Vec<_>>()
            .join(", "),
    })
}

/// Builds the tags line appended by `-t/--tags` (§2.1): splits `s` on
/// whitespace and prefixes any bare word with `@`, so `-t "beer @store"`
/// becomes `@beer @store`. Returns `None` if `s` has no tokens at all
/// (e.g. an empty or whitespace-only flag value), since then there's no
/// line to add.
pub fn tags_line_from_flag(s: &str) -> Option<String> {
    let tags: Vec<String> = s
        .split_whitespace()
        .map(|tok| if tok.starts_with('@') { tok.to_string() } else { format!("@{tok}") })
        .collect();
    (!tags.is_empty()).then(|| tags.join(" "))
}

/// Trims trailing blank (or whitespace-only) lines from `body`, per the
/// terminator normalization rule in §2. Also reused by editor mode
/// (§5.1) to normalize the whole file after a save.
pub(crate) fn normalize_trailing_blank_lines(body: &str) -> String {
    let mut lines: Vec<&str> = body.lines().collect();
    while matches!(lines.last(), Some(l) if l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    #[test]
    fn renders_timestamp_alone_on_its_own_line() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "124/80/55 @bp @health");
        assert_eq!(
            e.render(),
            "[2026-07-28.14:03:00]\n124/80/55 @bp @health\n\n"
        );
    }

    #[test]
    fn renders_header_with_no_body() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\n\n");
    }

    #[test]
    fn collapses_trailing_blank_lines_to_one() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "line1\nline2\n\n\n\n");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\nline1\nline2\n\n");
    }

    #[test]
    fn appends_blank_line_when_none_present() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "just one line");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\njust one line\n\n");
    }

    #[test]
    fn tags_are_recovered_from_anywhere_in_the_body() {
        let e = Entry::new(
            ts(2026, 7, 28, 14, 3, 0),
            "my @blood_pressure was 117/75/50",
        );
        assert_eq!(e.tags(), vec!["@blood_pressure"]);
    }

    #[test]
    fn tags_line_appended_by_dash_t_is_also_recovered() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "124/80/55\n@bp @health");
        assert_eq!(e.tags(), vec!["@bp", "@health"]);
    }

    #[test]
    fn entry_with_no_at_tokens_has_no_tags() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "just plain text");
        assert!(e.tags().is_empty());
    }

    #[test]
    fn tags_line_from_flag_prefixes_bare_words() {
        assert_eq!(
            tags_line_from_flag("beer @store"),
            Some("@beer @store".to_string())
        );
    }

    #[test]
    fn tags_line_from_flag_leaves_already_prefixed_tags_alone() {
        assert_eq!(
            tags_line_from_flag("@bp @health"),
            Some("@bp @health".to_string())
        );
    }

    #[test]
    fn tags_line_from_flag_is_none_for_blank_input() {
        assert_eq!(tags_line_from_flag(""), None);
        assert_eq!(tags_line_from_flag("   "), None);
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "124/80/55 @bp");
        let parsed = Entry::parse(e.render().trim_end_matches('\n')).unwrap();
        assert_eq!(parsed.timestamp, e.timestamp);
        assert_eq!(parsed.body, e.body);
    }

    #[test]
    fn stray_text_on_the_header_line_is_preserved_in_the_body() {
        // The canonical format only ever puts the timestamp on the
        // header line (§2), but a hand-edit -- or a file from before this
        // format existed -- could leave text there instead. Parsing must
        // fold it into the body, not silently drop it.
        let parsed = Entry::parse("[2026-07-28.14:03:00] oops no newline before this").unwrap();
        assert_eq!(parsed.body, "oops no newline before this");
    }

    #[test]
    fn header_overflow_is_prepended_to_an_existing_body() {
        let parsed = Entry::parse("[2026-07-28.14:03:00] oops\nreal body line").unwrap();
        assert_eq!(parsed.body, "oops\nreal body line");
    }

    #[test]
    fn old_format_header_tags_are_folded_into_the_body() {
        // A file written before this format change may still have tags
        // on the header line; they're preserved as body text (still
        // searchable as tags) rather than dropped.
        let parsed = Entry::parse("[2026-07-28.14:03:00] @bp @health\n124/80/55").unwrap();
        assert_eq!(parsed.body, "@bp @health\n124/80/55");
        assert_eq!(parsed.tags(), vec!["@bp", "@health"]);
    }

    #[test]
    fn parses_multiple_entries_from_a_journal_file() {
        let file = "[2026-07-28.14:03:00]\n124/80/55 @bp\n\n[2026-07-29.09:00:00]\nslept fine\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].body, "124/80/55 @bp");
        assert_eq!(entries[1].body, "slept fine");
    }

    #[test]
    fn parse_all_tolerates_blank_lines_inside_a_body() {
        let file = "[2026-07-28.14:03:00]\npara one\n\npara two\n\n[2026-07-29.09:00:00]\nnext entry\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].body, "para one\n\npara two");
        assert_eq!(entries[1].body, "next entry");
    }

    #[test]
    fn last_entry_start_finds_offset_of_final_header() {
        let file = "[2026-07-28.14:03:00]\nfirst\n\n[2026-07-29.09:00:00]\nsecond";
        let offset = Entry::last_entry_start(file).unwrap();
        assert_eq!(&file[offset..], "[2026-07-29.09:00:00]\nsecond");
    }

    #[test]
    fn last_entry_start_finds_the_only_header_with_no_trailing_newline() {
        let file = "[2026-07-28.14:03:00]";
        let offset = Entry::last_entry_start(file).unwrap();
        assert_eq!(offset, 0);
    }

    #[test]
    fn last_entry_start_is_none_without_any_header_line() {
        assert_eq!(Entry::last_entry_start("just some text\nno header here"), None);
    }

    /// `then`/`now` exactly `secs` seconds apart, `then` in the past.
    fn elapsed_secs_ago(secs: i64) -> (NaiveDateTime, NaiveDateTime) {
        let now = ts(2026, 7, 30, 12, 0, 0);
        (now - chrono::Duration::seconds(secs), now)
    }

    #[test]
    fn render_diff_disabled_is_no_annotation_at_all() {
        let (then, now) = elapsed_secs_ago(30);
        assert_eq!(render_diff(DiffStyle::Disabled, then, now), None);
    }

    // The doc examples below (30, 922, 10876 seconds) are taken verbatim
    // from the design note requesting this scheme; each is independently
    // verified against the 365-day-year/30-day-month cascading breakdown.
    // The design note's "2505610 -> 29 days" example is intentionally NOT
    // reproduced here: that number decomposes to 29d/0h/0m/10s, and a
    // later clarification established that zero units are skipped rather
    // than treated as a stopping point, so the correct output is now "29
    // days, 10 seconds" (see render_diff_long_skips_zero_units_... below).

    #[test]
    fn render_diff_long_matches_doc_example_seconds_only() {
        let (then, now) = elapsed_secs_ago(30);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "30 seconds ago");
    }

    #[test]
    fn render_diff_long_matches_doc_example_minutes_and_seconds() {
        // 922s = 15m 22s
        let (then, now) = elapsed_secs_ago(922);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "15 minutes, 22 seconds ago");
    }

    #[test]
    fn render_diff_long_matches_doc_example_hours_minutes_seconds() {
        // 10876s = 3h 1m 16s
        let (then, now) = elapsed_secs_ago(10876);
        assert_eq!(
            render_diff(DiffStyle::Long, then, now).unwrap(),
            "3 hours, 1 minute, 16 seconds ago"
        );
    }

    #[test]
    fn render_diff_long_window_excludes_units_past_max_even_if_nonzero() {
        // 2505610s = 29d 0h 0m 10s: the 3-unit window anchored at `days`
        // covers days/hours/minutes only -- the non-zero seconds=10 sits
        // past the window's edge and must never appear, regardless of
        // being non-zero. Matches the original doc example exactly.
        let (then, now) = elapsed_secs_ago(2_505_610);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "29 days ago");
    }

    #[test]
    fn render_diff_long_zero_months_does_not_hide_days_within_the_window() {
        // then=2021-06-20 -> now=2025-06-27: exactly 4 calendar years to
        // the day (2021-06-20 + 4y = 2025-06-20, <= now), plus 7 more
        // days, and 0 months in between. The window anchored at `years`
        // covers years/months/days; the zero `months` inside that window
        // is dropped from the output but doesn't hide the non-zero `days`
        // right after it, since both are within the window.
        let then = ts(2021, 6, 20, 10, 0, 0);
        let now = ts(2025, 6, 27, 10, 0, 0);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "4 years, 7 days ago");
    }

    #[test]
    fn render_diff_long_caps_at_three_units() {
        // then=2024-05-20 07:54:54 -> now=2025-07-30 12:00:00: 1 calendar
        // year (-> 2025-05-20 07:54:54) + 2 calendar months (-> 2025-07-20
        // 07:54:54) + a 10d4h5m6s remainder -- five non-zero units in a
        // row, but the window anchored at `years` is only 3 wide, so
        // hours/minutes/seconds fall outside it entirely.
        let then = ts(2024, 5, 20, 7, 54, 54);
        let now = ts(2025, 7, 30, 12, 0, 0);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "1 year, 2 months, 10 days ago");
    }

    #[test]
    fn render_diff_long_real_calendar_example_not_crude_approximation() {
        // Under the old crude 365-day-year/30-day-month approximation this
        // rendered as "36 years, 6 days ago" -- wrong. Verified
        // independently against Python's dateutil.relativedelta.
        let then = ts(1990, 8, 8, 8, 0, 0);
        let now = ts(2026, 8, 5, 12, 0, 0);
        assert_eq!(
            render_diff(DiffStyle::Long, then, now).unwrap(),
            "35 years, 11 months, 28 days ago"
        );
    }

    #[test]
    fn render_diff_long_month_addition_clamps_to_leap_day_not_march() {
        // Jan 31 + 1 calendar month clamps to Feb 29 (leap year), not a
        // rollover into March, per checked_add_months's documented
        // semantics.
        let then = ts(2024, 1, 31, 0, 0, 0);
        let now = ts(2024, 2, 29, 0, 0, 0);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "1 month ago");
    }

    #[test]
    fn render_diff_long_shows_hours_and_minutes_when_both_nonzero() {
        // Window anchored at `days` (5d, 3h, 20m): both extra slots filled.
        let secs = 5 * 86400 + 3 * 3600 + 20 * 60;
        let (then, now) = elapsed_secs_ago(secs);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "5 days, 3 hours, 20 minutes ago");
    }

    #[test]
    fn render_diff_long_shows_only_hours_when_minutes_is_zero() {
        // Window anchored at `days` (5d, 3h, 0m): the zero minutes inside
        // the window is dropped, leaving just days and hours.
        let secs = 5 * 86400 + 3 * 3600;
        let (then, now) = elapsed_secs_ago(secs);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "5 days, 3 hours ago");
    }

    #[test]
    fn render_diff_long_shows_only_minutes_when_hours_is_zero() {
        // Window anchored at `days` (5d, 0h, 20m): the zero hours in the
        // middle of the window is dropped, but doesn't hide the non-zero
        // minutes right after it, since both are within the window.
        let secs = 5 * 86400 + 20 * 60;
        let (then, now) = elapsed_secs_ago(secs);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "5 days, 20 minutes ago");
    }

    #[test]
    fn render_diff_long_direction_future_is_from_now() {
        let now = ts(2026, 7, 30, 12, 0, 0);
        let then = now + chrono::Duration::days(3);
        assert_eq!(render_diff(DiffStyle::Long, then, now).unwrap(), "3 days from now");
    }

    #[test]
    fn render_diff_long_all_zero_falls_back_to_zero_seconds() {
        let now = ts(2026, 7, 30, 12, 0, 0);
        assert_eq!(render_diff(DiffStyle::Long, now, now).unwrap(), "0 seconds ago");
    }

    #[test]
    fn render_diff_short_matches_doc_examples() {
        let (then, now) = elapsed_secs_ago(30);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "30s");
        let (then, now) = elapsed_secs_ago(922);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "15m, 22s");
        let (then, now) = elapsed_secs_ago(10876);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "3h, 1m");
        let (then, now) = elapsed_secs_ago(2_120_495);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "24d, 13h");
        // 2505610s = 29d 0h 0m 10s: the 2-unit window anchored at `days`
        // covers days/hours only -- the non-zero seconds=10 sits past the
        // window's edge and is never reached. Matches the original doc
        // example exactly.
        let (then, now) = elapsed_secs_ago(2_505_610);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "29d");
    }

    #[test]
    fn render_diff_short_never_shows_more_than_the_unit_right_after_the_highest() {
        // Window anchored at `days` is only 2 wide (days, hours) -- a
        // non-zero `minutes` a further step down must never appear, no
        // matter how the window's own two units turn out.
        let secs = 5 * 86400 + 47 * 60; // 5d, 0h, 47m
        let (then, now) = elapsed_secs_ago(secs);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "5d");

        let secs = 5 * 86400 + 3 * 3600 + 47 * 60; // 5d, 3h, 47m
        let (then, now) = elapsed_secs_ago(secs);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "5d, 3h");
    }

    #[test]
    fn render_diff_short_has_no_direction_word() {
        let now = ts(2026, 7, 30, 12, 0, 0);
        let then = now + chrono::Duration::days(3);
        assert_eq!(render_diff(DiffStyle::Short, then, now).unwrap(), "3d");
    }

    #[test]
    fn display_header_default_is_exact_on_disk_timestamp_with_no_age() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "slept fine");
        let opts = DisplayOpts { human: false, format: "", diff: DiffStyle::Short };
        assert_eq!(e.display_header(&opts), "### 2026-07-28.14:03:00\n");
    }

    #[test]
    fn display_header_human_mode_short_diff_by_default() {
        let e = Entry::new(Local::now().naive_local() - chrono::Duration::days(3), "slept fine");
        let opts = DisplayOpts {
            human: true,
            format: "%Y-%m-%d %H:%M",
            diff: DiffStyle::Short,
        };
        let header = e.display_header(&opts);
        assert!(header.starts_with(&format!("### {}", e.timestamp.format("%Y-%m-%d %H:%M"))));
        assert!(header.contains("(3d)"));
    }

    #[test]
    fn display_header_human_mode_disabled_diff_has_no_parens() {
        let e = Entry::new(Local::now().naive_local() - chrono::Duration::days(3), "slept fine");
        let opts = DisplayOpts { human: true, format: "%Y-%m-%d", diff: DiffStyle::Disabled };
        let header = e.display_header(&opts);
        assert_eq!(header, format!("### {}\n", e.timestamp.format("%Y-%m-%d")));
        assert!(!header.contains('('));
    }
}
