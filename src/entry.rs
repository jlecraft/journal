use chrono::{Datelike, Local, Months, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::Deserialize;

/// `strftime`/`strptime` pattern matching the `[YYYY-MM-DD HH:MM:SS]` format
/// from §2. This is what the tool itself always writes; a human hand-typing
/// or hand-editing a header may write a less precise form instead (see
/// `Timestamp` and `parse_timestamp_text`).
pub const TIMESTAMP_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// The timestamp shape to store for a newly created interactive entry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TimestampShape {
    #[default]
    Full,
    Year,
    MonthDay,
    Time,
}

impl TimestampShape {
    fn format(self, timestamp: NaiveDateTime) -> String {
        let format = match self {
            Self::Full => TIMESTAMP_FMT,
            Self::Year => "%Y",
            Self::MonthDay => "%m-%d",
            Self::Time => "%H:%M:%S",
        };
        timestamp.format(format).to_string()
    }
}

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

/// The date half of a `Timestamp` (`timestamps.md`), covering the four
/// distinct shapes a header's date part can take -- three explicit shapes,
/// plus "no date part at all," each with its own defaulting rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateSpec {
    /// No date part in the header at all -- unlike every other case,
    /// this doesn't default to a fixed value: it tracks *today's date*,
    /// resolved fresh every time `Timestamp::resolved` is called.
    Today,
    /// Just a year (`YYYY`) -- month/day default to `1`/`1`.
    YearOnly(i32),
    /// Just a month and day (`MM-DD`), no year -- the year tracks
    /// whatever year it currently is, resolved fresh every call.
    MonthDayOnly { month: u32, day: u32 },
    /// A full date (`YYYY-MM-DD`) -- nothing to default.
    Full(NaiveDate),
}

/// A header's parsed timestamp (§2.2, and `timestamps.md`). This tool only
/// ever *writes* a timestamp with every component given (`From<NaiveDateTime>`
/// below), but a human hand-typing or hand-editing a header may write any
/// combination of a date part (`DateSpec`) and a time part (`HH:MM` or
/// `HH:MM:SS`, seconds defaulting to `0`), including just one of the two,
/// or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    date: DateSpec,
    hour: u32,
    minute: u32,
    second: u32,
}

impl Timestamp {
    /// The point in time to use for display and `-h` elapsed-time math.
    /// `DateSpec::Today` and `MonthDayOnly`'s year both substitute a
    /// "current" value -- resolved fresh every call, since neither was
    /// ever truly anchored to any particular day/year; nothing about the
    /// entry itself changes, only what "today"/"the current year" happens
    /// to be when this runs. On the one date this can actually affect
    /// validity (`MonthDayOnly`'s `02-29`, only valid in a leap year),
    /// falls back to `02-28` for a substituted year that turns out not to
    /// be a leap year -- `month`/`day` were already validated as a
    /// possible combination at parse time (`parse_date_tok`), just not
    /// against this specific year.
    pub fn resolved(&self) -> NaiveDateTime {
        let date = match self.date {
            DateSpec::Today => Local::now().date_naive(),
            DateSpec::YearOnly(year) => {
                NaiveDate::from_ymd_opt(year, 1, 1).expect("validated at parse time")
            }
            DateSpec::MonthDayOnly { month, day } => {
                let year = Local::now().year();
                NaiveDate::from_ymd_opt(year, month, day)
                    .or_else(|| NaiveDate::from_ymd_opt(year, month, day - 1))
                    .expect("month/day validated against a leap-year reference at parse time")
            }
            DateSpec::Full(date) => date,
        };
        date.and_hms_opt(self.hour, self.minute, self.second)
            .expect("hour/minute/second validated at parse time")
    }

    /// The on-disk (and default non-`-h` display) rendering: full
    /// `YYYY-MM-DD HH:MM:SS`, resolving a missing date/year the same way
    /// `resolved()` does.
    pub fn render(&self) -> String {
        self.resolved().format(TIMESTAMP_FMT).to_string()
    }
}

impl From<NaiveDateTime> for Timestamp {
    fn from(dt: NaiveDateTime) -> Self {
        Timestamp {
            date: DateSpec::Full(dt.date()),
            hour: dt.hour(),
            minute: dt.minute(),
            second: dt.second(),
        }
    }
}

/// An entry's timestamp line is always on its own line, with nothing
/// else on it (§2). Tags are no longer a separate structured field --
/// they're just `@word` tokens that happen to appear somewhere in the
/// body, whether typed inline by the user or appended as their own line
/// by `-t/--tags` (§2.1). `Entry::tags` recovers them on demand by
/// scanning the body, rather than the tool tracking them separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub timestamp: Timestamp,
    pub body: String,
    /// The exact bracket-interior text this entry was parsed from, if it
    /// was parsed from existing file content at all (`None` for an entry
    /// built fresh via `new`/`now`, which has no prior text to preserve).
    /// `render()` writes this back out verbatim rather than reformatting
    /// it from `timestamp` -- a human who hand-typed or hand-edited a
    /// header into a shorthand form (§2.2) gets to keep it that way on
    /// disk; only *display* (`display_header`) expands it dynamically.
    raw_header: Option<String>,
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
    /// Whether to show the header line at all (`--no-headers` sets this
    /// false). When false, `display_header` returns an empty string,
    /// `display` prints just the body, and the blank line that would
    /// otherwise separate entries is dropped too -- `--no-headers` output
    /// is just each entry's body, back-to-back.
    pub header: bool,
}

impl Entry {
    pub fn new(timestamp: impl Into<Timestamp>, body: impl Into<String>) -> Self {
        Entry {
            timestamp: timestamp.into(),
            body: normalize_trailing_blank_lines(&body.into()),
            raw_header: None,
        }
    }

    pub fn now(body: impl Into<String>) -> Self {
        Self::new(Local::now().naive_local(), body)
    }

    /// Creates an entry at `timestamp` while retaining only the selected
    /// timestamp components in its on-disk header.
    pub fn new_with_shape(
        timestamp: NaiveDateTime,
        body: impl Into<String>,
        shape: TimestampShape,
    ) -> Self {
        let mut entry = Self::new(timestamp, body);
        entry.raw_header = Some(shape.format(timestamp));
        entry
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
    /// terminates every entry (§2). The header line is `raw_header`
    /// verbatim when this entry was parsed from existing text -- a
    /// hand-typed shorthand timestamp (§2.2) is never "fixed" back to
    /// full precision just because the entry passed through the tool
    /// again (e.g. editor mode re-rendering the entry it just seeded,
    /// per §3.3). Only an entry with no prior text at all (freshly built
    /// via `new`/`now`, i.e. a genuinely new append) falls back to
    /// `timestamp`'s canonical full-precision form.
    pub fn render(&self) -> String {
        let canonical;
        let header_text: &str = match &self.raw_header {
            Some(raw) => raw,
            None => {
                canonical = self.timestamp.render();
                &canonical
            }
        };
        let mut out = format!("[{header_text}]\n");
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
    /// The trailing blank line that separates entries when several are
    /// printed back-to-back is itself part of that heading-based
    /// separation, so it's only added when `opts.header` is true --
    /// `--no-headers` drops both, leaving nothing but each entry's body.
    pub fn display(&self, opts: &DisplayOpts) -> String {
        let mut out = self.display_header(opts);
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        if opts.header {
            out.push('\n');
        }
        out
    }

    /// Just the reformatted header line from `display`, with no body --
    /// used by `-L/--lines-only` to print the entry's header once,
    /// followed by only the lines that actually matched. Display always
    /// shows `timestamp.resolved()` -- the fully expanded date and time,
    /// with any components a human left out (§2.2) dynamically filled in
    /// -- regardless of what's actually sitting on disk (`render`, unlike
    /// this, preserves shorthand verbatim). Without `opts.human`, that's
    /// just the resolved value in the on-disk `TIMESTAMP_FMT` layout, no
    /// age annotation. With `opts.human`, it's `### {opts.format}`
    /// optionally followed by ` ({diff})` -- the diff annotation is
    /// entirely absent (not just empty parens) only when `opts.diff` is
    /// `DiffStyle::Disabled`. A malformed (no-year) timestamp (§2.2) still
    /// gets one: it resolves to *some* date (today's), so there's always a
    /// fixed point to measure the gap from, even though that date wasn't
    /// actually given. Returns an empty string when `opts.header` is false
    /// (`--no-headers`), so callers building on top of this (`display`,
    /// `-L/--lines-only`'s per-entry block) don't need their own check.
    pub fn display_header(&self, opts: &DisplayOpts) -> String {
        if !opts.header {
            return String::new();
        }
        let resolved = self.timestamp.resolved();
        if opts.human {
            let date = resolved.format(opts.format);
            match render_diff(opts.diff, resolved, Local::now().naive_local()) {
                Some(diff) => format!("### {date} ({diff})\n"),
                None => format!("### {date}\n"),
            }
        } else {
            format!("### {}\n", resolved.format(TIMESTAMP_FMT))
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
        let (timestamp, raw_header, header_overflow) = parse_header(header)?;
        let rest = lines.collect::<Vec<_>>().join("\n");
        let body = match header_overflow {
            Some(overflow) if rest.is_empty() => overflow,
            Some(overflow) => format!("{overflow}\n{rest}"),
            None => rest,
        };
        Some(Entry {
            timestamp,
            body: normalize_trailing_blank_lines(&body),
            raw_header: Some(raw_header),
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

}

/// Parses a header line into `(timestamp, raw_header, header_overflow)`.
/// The timestamp line is always alone (§2); `raw_header` is the exact
/// bracket-interior text, verbatim, for `Entry::render` to preserve
/// (§2.2 -- a hand-typed shorthand timestamp is never expanded back out
/// on disk); `header_overflow` is whatever text (if any) follows the
/// closing `]` on that same line -- not expected in files this tool
/// writes, but preserved rather than dropped if found (e.g. a hand-edit,
/// or a file from before this format change).
fn parse_header(line: &str) -> Option<(Timestamp, String, Option<String>)> {
    let rest = line.strip_prefix('[')?;
    let (ts_str, rest) = rest.split_once(']')?;
    let timestamp = parse_timestamp_text(ts_str)?;
    let overflow = rest.trim();
    Some((timestamp, ts_str.to_string(), (!overflow.is_empty()).then(|| overflow.to_string())))
}

/// Parses the text between a header's `[` and `]` into a `Timestamp`
/// (`timestamps.md`). A date part (`YYYY`, `MM-DD`, or `YYYY-MM-DD`) and a
/// time part (`HH:MM` or `HH:MM:SS`) can each be given or omitted
/// independently -- any combination is valid, including just one of the
/// two, or (single-token case) neither, if that token parses as a time and
/// there's simply no date part at all (`DateSpec::Today`). One space
/// separates the two parts when both are present, matching the on-disk
/// separator between date and time (§2).
fn parse_timestamp_text(s: &str) -> Option<Timestamp> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let (date, hour, minute, second) = match tokens.as_slice() {
        [date_tok, time_tok] => {
            let date = parse_date_tok(date_tok)?;
            let (hour, minute, second) = parse_time_tok(time_tok)?;
            (date, hour, minute, second)
        }
        [tok] => match parse_date_tok(tok) {
            Some(date) => (date, 0, 0, 0),
            None => {
                let (hour, minute, second) = parse_time_tok(tok)?;
                (DateSpec::Today, hour, minute, second)
            }
        },
        _ => return None,
    };
    Some(Timestamp { date, hour, minute, second })
}

/// Parses one of the three valid date shapes (`timestamps.md`) into a
/// `DateSpec`:
/// - `YYYY` -- year only; month/day default to `1`/`1`.
/// - `MM-DD` -- month and day, no year at all -- the year substitutes
///   whatever year it currently is (`Timestamp::resolved`).
/// - `YYYY-MM-DD` -- every component given.
///
/// `MM-DD`'s validity is checked against a leap-year reference (year `4`)
/// since the real year isn't known yet -- `02-29` is accepted here and
/// only possibly downgraded to `02-28` later, at `resolved()` time,
/// depending on what year actually gets substituted in.
fn parse_date_tok(s: &str) -> Option<DateSpec> {
    let parts: Vec<&str> = s.split('-').collect();
    match parts.as_slice() {
        [y] => {
            let year = parse_digits(y)? as i32;
            NaiveDate::from_ymd_opt(year, 1, 1)?;
            Some(DateSpec::YearOnly(year))
        }
        [m, d] => {
            let month = parse_digits(m)?;
            let day = parse_digits(d)?;
            NaiveDate::from_ymd_opt(4, month, day)?;
            Some(DateSpec::MonthDayOnly { month, day })
        }
        [y, m, d] => {
            let year = parse_digits(y)? as i32;
            let month = parse_digits(m)?;
            let day = parse_digits(d)?;
            let date = NaiveDate::from_ymd_opt(year, month, day)?;
            Some(DateSpec::Full(date))
        }
        _ => None,
    }
}

/// Parses `HH:MM` or `HH:MM:SS` into `(hour, minute, second)`, defaulting
/// an omitted seconds field to `0` (`timestamps.md`).
fn parse_time_tok(s: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m] => (parse_digits(h)?, parse_digits(m)?, 0),
        [h, m, s] => (parse_digits(h)?, parse_digits(m)?, parse_digits(s)?),
        _ => return None,
    };
    NaiveTime::from_hms_opt(h, m, sec)?;
    Some((h, m, sec))
}

/// A plain non-negative integer, with no sign and no separators -- rejects
/// anything (like a stray `:` that leaked in from the other half of the
/// timestamp) that isn't purely ASCII digits.
fn parse_digits(s: &str) -> Option<u32> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
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
            "[2026-07-28 14:03:00]\n124/80/55 @bp @health\n\n"
        );
    }

    #[test]
    fn renders_header_with_no_body() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "");
        assert_eq!(e.render(), "[2026-07-28 14:03:00]\n\n");
    }

    #[test]
    fn collapses_trailing_blank_lines_to_one() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "line1\nline2\n\n\n\n");
        assert_eq!(e.render(), "[2026-07-28 14:03:00]\nline1\nline2\n\n");
    }

    #[test]
    fn appends_blank_line_when_none_present() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "just one line");
        assert_eq!(e.render(), "[2026-07-28 14:03:00]\njust one line\n\n");
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
        let parsed = Entry::parse("[2026-07-28 14:03:00] oops no newline before this").unwrap();
        assert_eq!(parsed.body, "oops no newline before this");
    }

    #[test]
    fn header_overflow_is_prepended_to_an_existing_body() {
        let parsed = Entry::parse("[2026-07-28 14:03:00] oops\nreal body line").unwrap();
        assert_eq!(parsed.body, "oops\nreal body line");
    }

    #[test]
    fn old_format_header_tags_are_folded_into_the_body() {
        // A file written before this format change may still have tags
        // on the header line; they're preserved as body text (still
        // searchable as tags) rather than dropped.
        let parsed = Entry::parse("[2026-07-28 14:03:00] @bp @health\n124/80/55").unwrap();
        assert_eq!(parsed.body, "@bp @health\n124/80/55");
        assert_eq!(parsed.tags(), vec!["@bp", "@health"]);
    }

    #[test]
    fn parses_multiple_entries_from_a_journal_file() {
        let file = "[2026-07-28 14:03:00]\n124/80/55 @bp\n\n[2026-07-29 09:00:00]\nslept fine\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].body, "124/80/55 @bp");
        assert_eq!(entries[1].body, "slept fine");
    }

    #[test]
    fn parse_all_tolerates_blank_lines_inside_a_body() {
        let file = "[2026-07-28 14:03:00]\npara one\n\npara two\n\n[2026-07-29 09:00:00]\nnext entry\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].body, "para one\n\npara two");
        assert_eq!(entries[1].body, "next entry");
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
        let opts = DisplayOpts { human: false, format: "", diff: DiffStyle::Short, header: true };
        assert_eq!(e.display_header(&opts), "### 2026-07-28 14:03:00\n");
    }

    #[test]
    fn display_header_human_mode_short_diff_by_default() {
        let e = Entry::new(Local::now().naive_local() - chrono::Duration::days(3), "slept fine");
        let opts = DisplayOpts {
            human: true,
            format: "%Y-%m-%d %H:%M",
            diff: DiffStyle::Short,
            header: true,
        };
        let header = e.display_header(&opts);
        assert!(header.starts_with(&format!("### {}", e.timestamp.resolved().format("%Y-%m-%d %H:%M"))));
        assert!(header.contains("(3d)"));
    }

    #[test]
    fn display_header_human_mode_disabled_diff_has_no_parens() {
        let e = Entry::new(Local::now().naive_local() - chrono::Duration::days(3), "slept fine");
        let opts = DisplayOpts { human: true, format: "%Y-%m-%d", diff: DiffStyle::Disabled, header: true };
        let header = e.display_header(&opts);
        assert_eq!(header, format!("### {}\n", e.timestamp.resolved().format("%Y-%m-%d")));
        assert!(!header.contains('('));
    }

    /// Constructs a `Timestamp` with every component given explicitly --
    /// a shorthand for the fully-resolved case, mirroring `ts()` above.
    fn dated(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Timestamp {
        Timestamp {
            date: DateSpec::Full(NaiveDate::from_ymd_opt(y, mo, d).unwrap()),
            hour: h,
            minute: mi,
            second: s,
        }
    }

    /// Constructs a `Timestamp` with just a year given -- month/day
    /// default to `1`/`1`.
    fn year_only(y: i32, h: u32, mi: u32, s: u32) -> Timestamp {
        Timestamp { date: DateSpec::YearOnly(y), hour: h, minute: mi, second: s }
    }

    /// Constructs a `Timestamp` with a month/day but no year -- the year
    /// tracks whatever it currently is at display time (`timestamps.md`).
    fn month_day_only(mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Timestamp {
        Timestamp { date: DateSpec::MonthDayOnly { month: mo, day: d }, hour: h, minute: mi, second: s }
    }

    /// Constructs a `Timestamp` with no date part at all -- it tracks
    /// today's date at display time (`timestamps.md`).
    fn today_date(h: u32, mi: u32, s: u32) -> Timestamp {
        Timestamp { date: DateSpec::Today, hour: h, minute: mi, second: s }
    }

    #[test]
    fn flexible_timestamp_year_only_defaults_to_jan_first_midnight() {
        let e = Entry::parse("[1972]\nold entry").unwrap();
        assert_eq!(e.timestamp, year_only(1972, 0, 0, 0));
    }

    #[test]
    fn flexible_timestamp_month_day_with_no_year_tracks_the_current_year() {
        let e = Entry::parse("[08-07]\nold entry").unwrap();
        assert_eq!(e.timestamp, month_day_only(8, 7, 0, 0, 0));
    }

    #[test]
    fn flexible_timestamp_year_and_time_defaults_month_and_day() {
        let e = Entry::parse("[1972 08:30]\nold entry").unwrap();
        assert_eq!(e.timestamp, year_only(1972, 8, 30, 0));
    }

    #[test]
    fn flexible_timestamp_month_day_and_time_with_no_year() {
        let e = Entry::parse("[08-07 08:30]\nold entry").unwrap();
        assert_eq!(e.timestamp, month_day_only(8, 7, 8, 30, 0));
    }

    #[test]
    fn flexible_timestamp_year_month_day_and_time_defaults_seconds() {
        let e = Entry::parse("[1972-06-15 08:30]\nold entry").unwrap();
        assert_eq!(e.timestamp, dated(1972, 6, 15, 8, 30, 0));
    }

    #[test]
    fn flexible_timestamp_with_no_date_at_all_tracks_todays_date() {
        let e = Entry::parse("[08:30]\nno date given").unwrap();
        assert_eq!(e.timestamp, today_date(8, 30, 0));
    }

    #[test]
    fn a_two_part_dash_date_is_month_day_not_year_month() {
        // "1972-06" used to mean "year 1972, month 6" under an earlier
        // scheme; per timestamps.md, a two-part dashed date is always
        // MM-DD. "1972" isn't a valid month, so this whole header fails
        // to parse -- it's not recognized as a header at all.
        assert_eq!(parse_header("[1972-06]"), None);
    }

    #[test]
    fn empty_or_invalid_month_day_combinations_do_not_parse() {
        assert_eq!(parse_header("[13-01]"), None); // no month 13
        assert_eq!(parse_header("[02-30]"), None); // no Feb 30, in any year
    }

    #[test]
    fn timestamp_with_no_year_resolves_against_the_current_year() {
        let e = Entry::parse("[08-07 08:30]\nno year given").unwrap();
        let current_year = Local::now().year();
        assert_eq!(e.timestamp.resolved(), ts(current_year, 8, 7, 8, 30, 0));
    }

    #[test]
    fn timestamp_with_no_date_at_all_resolves_to_today() {
        let e = Entry::parse("[08:30]\nno date given").unwrap();
        let today = Local::now().date_naive();
        assert_eq!(e.timestamp.resolved().date(), today);
        assert_eq!(
            e.timestamp.resolved().time(),
            NaiveTime::from_hms_opt(8, 30, 0).unwrap()
        );
    }

    #[test]
    fn month_day_of_feb_29_resolves_to_feb_28_in_a_non_leap_current_year() {
        let e = Entry::parse("[02-29]\nleap day, maybe").unwrap();
        let resolved = e.timestamp.resolved();
        let is_leap_year = NaiveDate::from_ymd_opt(resolved.year(), 2, 29).is_some();
        assert_eq!(resolved.month(), 2);
        assert_eq!(resolved.day(), if is_leap_year { 29 } else { 28 });
    }

    #[test]
    fn timestamp_with_no_date_at_all_still_gets_a_diff_annotation() {
        // Since a missing date always resolves to *some* concrete day
        // (today), there's always a fixed point to measure against -- it
        // gets a diff annotation just like a fully-dated entry. This
        // holds no matter which direction the diff points, so this only
        // checks that an annotation shows up at all, not its exact value.
        let then = Local::now().naive_local() - chrono::Duration::minutes(5);
        let e = Entry::parse(&format!("[{}]\nno date given", then.format("%H:%M:%S"))).unwrap();
        let opts = DisplayOpts { human: true, format: "%H:%M", diff: DiffStyle::Short, header: true };
        let header = e.display_header(&opts);
        assert!(header.contains('('), "expected an elapsed-time annotation, got: {header}");
    }

    #[test]
    fn timestamp_with_no_date_at_all_renders_on_disk_exactly_as_hand_typed() {
        let e = Entry::parse("[08:30]\nno date given").unwrap();
        assert_eq!(e.render(), "[08:30]\nno date given\n\n");
    }

    #[test]
    fn shorthand_dated_timestamp_renders_on_disk_exactly_as_hand_typed() {
        // "[2025]" is never "fixed" back to "2025-01-01 00:00:00" on disk
        // just because the entry round-tripped through parse/render --
        // only a freshly-appended entry (no prior text) gets the full
        // canonical form (see round_trips_through_render_and_parse).
        let e = Entry::parse("[2025]\nsome body").unwrap();
        assert_eq!(e.render(), "[2025]\nsome body\n\n");
    }

    #[test]
    fn shorthand_dated_timestamp_is_expanded_for_display_even_without_human_flag() {
        let e = Entry::parse("[2025]\nsome body").unwrap();
        let opts = DisplayOpts { human: false, format: "", diff: DiffStyle::Short, header: true };
        assert_eq!(e.display_header(&opts), "### 2025-01-01 00:00:00\n");
    }

    #[test]
    fn timestamp_with_no_date_at_all_is_expanded_to_todays_date_for_display_even_without_human_flag() {
        let e = Entry::parse("[08:30]\nno date given").unwrap();
        let today = Local::now().date_naive();
        let opts = DisplayOpts { human: false, format: "", diff: DiffStyle::Short, header: true };
        assert_eq!(
            e.display_header(&opts),
            format!("### {} 08:30:00\n", today.format("%Y-%m-%d"))
        );
    }

    #[test]
    fn freshly_appended_entry_has_no_raw_header_and_renders_canonically() {
        let e = Entry::now("just typed this");
        assert_eq!(e.raw_header, None);
    }

    #[test]
    fn empty_header_brackets_do_not_parse_as_a_timestamp() {
        assert_eq!(parse_header("[]"), None);
    }

    #[test]
    fn garbage_header_text_does_not_parse_as_a_timestamp() {
        assert_eq!(parse_header("[not a date]"), None);
    }
}
