# `journal` — CLI Journaling Tool Specification

**Language:** Rust

This document describes `journal`'s current behavior in full and is kept in
sync with the implementation — it is a reference, not a design log. For
installation and quick usage examples, see `README.md`.

## 1. Overview

`journal` is a command-line utility for appending timestamped, taggable
entries to a plain-text journal file, and for searching that file by tag or
keyword. Entries are stored in a flat, human-readable format so the
underlying file remains inspectable and editable without the tool. There are
no subcommands (no `add`/`list`/`show` verbs) — behavior is selected entirely
by which flags are present on a single top-level command.

## 2. On-Disk Entry Format

```
[YYYY-MM-DD HH:MM:SS]
Entry body, line 1
Entry body, line 2 (optional)
@tag1 @tag2 (optional; present only if -t/--tags was used)

```

- **Header line:** a timestamp in `[YYYY-MM-DD HH:MM:SS]` format, and nothing
  else. The timestamp is always alone on its own line. This is the form the
  normal append modes write. Interactive mode may deliberately write one of
  the shorthand forms described in §2.2.
- **Body:** any number of lines, including blank lines. `@tag` tokens may
  appear anywhere in the body — there is no separate structured storage for
  tags.
- **Terminator:** every entry ends with exactly one blank line. If the entry
  text supplied by the user has trailing blank lines, they are collapsed to
  one; if it has none, one is appended.
- **Multiple entries** in a file are simply concatenated; entry boundaries
  are located by scanning for header lines, not blank lines, since a body may
  legitimately contain blank lines of its own.

### 2.1 Tags

A tag is any token matching `@\S+`, recognized by shape wherever it appears
in the body — tags are not a structured field hoisted onto the header line.

| Method | Example | Result |
|---|---|---|
| Typed inline, anywhere in the text | `journal "my @blood_pressure was 117/75/50"` | `@blood_pressure` stays exactly where typed |
| Explicit `-t/--tags` flag | `journal -t "beer @store" "grabbed a six-pack"` | `@beer @store` is appended as its own line at the end of the entry |

No hoisting, extraction, or de-duplication occurs. `-t/--tags` bare words
(without a leading `@`) are automatically prefixed with `@` before being
concatenated onto their own line; tokens already prefixed are left as-is.
Inline tags typed directly in the entry text are left completely untouched —
they're just body text that happens to match the tag shape.

### 2.2 Flexible (hand-typed) timestamps

A header a human hand-types or hand-edits doesn't need to be the full
`YYYY-MM-DD HH:MM:SS` form. A date part and a time part can each be given
or left out independently, and each has its own valid shapes:

| Date shapes | Time shapes |
|---|---|
| `YYYY` | `HH:MM` |
| `MM-DD` | `HH:MM:SS` |
| `YYYY-MM-DD` | |

Any combination of one date shape and one time shape is valid, including
just a date, just a time, or (per §2) the full `YYYY-MM-DD HH:MM:SS` with
both. When both are present, the date comes first, followed by one space
and the time — the same separator as the on-disk format itself.

Whatever isn't given is defaulted:

- **No date part at all** (just a time, or nothing) → the date defaults to
  **today**, resolved fresh every time the entry is displayed (§4), not
  fixed at parse time. This takes priority over the next two rules — a
  bare time like `[08:30]` gets today's actual month/day, not `01-01`.
- **No year, but a date part was given** (the `MM-DD` shape) → the
  **current year** is substituted, resolved fresh at display time, same
  as above.
- **No month/day, but a date part was given** (the `YYYY` shape) →
  `01-01`.
- **No seconds** (the `HH:MM` time shape) → `00`.
- **No time part at all** → `00:00:00`.

| Header | Displays as (§4) |
|---|---|
| `[1972]` | `1972-01-01 00:00:00` |
| `[08-07]` | `⟨current year⟩-08-07 00:00:00` |
| `[1972-06-15]` | `1972-06-15 00:00:00` |
| `[1972 08:30]` | `1972-01-01 08:30:00` |
| `[08:30]` | `⟨today⟩ 08:30:00` |
| `[08-07 08:30]` | `⟨current year⟩-08-07 08:30:00` |

Every combination resolves to a real, complete point in time, so `-h`/
`--human` always computes an elapsed-time annotation for it (§4) — even
when the date behind that annotation is only a stand-in, never one the
human actually wrote down.

**This flexibility is a display-time affordance only — what's on disk is
never rewritten because of it.** The tool itself always writes the full
`YYYY-MM-DD HH:MM:SS` form (§2) for a genuinely new entry appended via
`journal "..."` or stdin. Interactive mode (§3.2) is the exception: it can
store a selected shorthand shape. A header that already exists on disk in a
shorter hand-typed form — including one a human types directly into the
journal file via editor mode (§3.3), since nothing is pre-seeded there —
keeps that exact text forever, byte-for-byte. `journal` only ever
*expands* a header for the reader (§4); it doesn't "fix" what was
intentionally written by hand.

## 3. Modes of Operation

### 3.1 Append an entry

```sh
journal "124/80/55 @bp @health"
journal -t "sleep" "slept 7 hours"       # appends "@sleep" as its own line
echo "back from a walk @exercise" | journal -
```

Positional `TEXT` is appended as a new entry with the current timestamp.
`-t/--tags` (if given) is normalized per §2.1 and concatenated onto its own
trailing line. A literal `-` as the text argument reads the entry body from
stdin instead (the standard Unix filter-tool convention). `-t/--tags` and
positional `TEXT` both conflict with `-s/--search` at the argument-parsing
level (exit code 2 if combined).

### 3.2 Interactive append

`-i/--interactive` runs a prompted append workflow without requiring a TTY.
All prompts and validation messages go to stderr and each prompt is flushed:

```text
Tags (space-separated, optional):
Timestamp [F/full, y/year, d/month-day, t/time] (default F):
Entry (press Ctrl-D on an empty line to submit):
```

Tags are optional and use the same normalization as `-t/--tags`. Timestamp
choices are case-insensitive: empty, `F`, or `full` stores
`YYYY-MM-DD HH:MM:SS`; `y` or `year` stores `YYYY`; `d`, `date`, or
`month-day` stores `MM-DD`; and `t` or `time` stores `HH:MM:SS`. Invalid
choices print an explanation and repeat the timestamp prompt.

The body is multiline and EOF submits it; empty bodies are valid. EOF while
reading tags or choosing the timestamp aborts with exit code 1 and writes
nothing. The current local time is captured only after body submission, and
the resulting entry uses the normal append lock and verbose diagnostics.
Interactive mode can be combined only with `-f/--file` and `-v/--verbose`;
all positional text, tags, search/list modes, and display flags are usage
errors (exit code 2).

### 3.3 Compose via `$EDITOR` (no arguments)

Running `journal` with no positional text opens the resolved journal file
directly in `$EDITOR` (falling back to `vi`) — see §6 for the full
mechanics. Nothing is pre-inserted: no timestamp, no blank line, no tags
line. The user types a new header and body themselves (per §2's on-disk
format) to compose a new entry, or edits anything else already in the file
— there's no distinction between "compose" and "edit an existing entry"
modes, since both are just editing the same file directly. `-t/--tags` has
nothing to attach a tags line to here, so combining it with the
no-argument form is a usage error (exit code 2) rather than being silently
ignored.

### 3.3 Show the last N entries (`-N`)

```sh
journal -3
```

Prints the last `N` entries in chronological order (oldest → newest, like
`tail`), using the human-facing display format (§4). Fewer than `N` entries
in the journal is not an error — everything available is printed. An empty
journal prints nothing and exits `0`.

`-N` is a short flag whose name is a variable-length run of digits, which
`clap`'s derive API cannot express as a declared flag; it's extracted from
`argv` by hand before the rest of the arguments reach `clap`. It cannot be
combined with `-s/--search`, positional `TEXT`, or `-t/--tags`, and `-0` is
rejected (all three: exit code 2).

### 3.4 Search (`-s/--search`)

```sh
journal -s "@bp"                # entries tagged @bp (exact tag match)
journal -s "fm radio"           # entries containing "fm" OR "radio"
journal -s "fm radio" -a        # entries containing "fm" AND "radio"
journal -s "linux+kernel"       # entries containing the literal phrase "linux kernel"
journal -s "th" --limit 5       # cap the number of results
journal -s "fm radio" -L        # print only the matching lines, not full entries
journal -s "fm radio" -r        # newest match first instead of oldest
```

`-a/--all`, `--limit`, `-L/--lines-only`, and `-r/--reverse-sort` are usage
errors (exit code 2) unless `-s/--search` is also given. `-v/--verbose`
(§12) has no such restriction — it's orthogonal to every mode in this
section.

#### 3.4.1 Term parsing

The query is split on whitespace into terms. Within a term, `+` is replaced
with a space, joining words into one multi-word substring term (e.g.
`linux+kernel` matches the literal substring `linux kernel`). A term starting
with `@` (and longer than just `@`) is a **tag term**; anything else is a
**substring term**. All terms are matched case-insensitively.

- **Substring term:** matches if it appears anywhere in the match scope (see
  §3.4.2 / §3.4.3 for what that scope is) as a plain substring. This is
  broad by design — a search for `"th"` matches inside `"weather"` or
  `"month"` too, so short or common terms can return many results.
- **Tag term:** requires a **full-word match** against a `@tag` token found
  in the match scope — a search for `@bp` matches the tag `@bp` but not
  `@bph`. A bare word without `@` still finds a tag via ordinary substring
  matching (`blood_pressure` matches `@blood_pressure` as a substring of the
  whole entry).

#### 3.4.2 Whole-entry mode (default)

An entry matches if its terms are satisfied *anywhere* in the entry — the
on-disk header line plus the full body, treated as one string. Default is OR
(any term matches); `-a/--all` requires every term to match somewhere in the
entry, not necessarily on the same line. On match, the entry is printed in
full using the display format (§4). `--limit N` caps the number of matching
entries printed (not the number of matched lines/terms). Multiple matching
entries are separated by a blank line.

#### 3.4.3 Lines-only mode (`-L`/`--lines-only`)

Instead of the full entry body, prints the entry's header (§4) once, followed
by only the body lines that themselves satisfy the term condition:

- Default (OR): a line qualifies if it contains *any* term.
- `-a/--all` combined with `-L`: a line qualifies only if it contains *every*
  term **on that same line** — stricter than whole-entry `-a`, where terms
  may be spread across different lines of the entry.

An entry is included in the output only if it has at least one qualifying
line; `--limit N` still caps the number of entries (not lines) printed.

#### 3.4.4 Highlighting

Matched terms are wrapped in ANSI bold-red (`grep`'s default match style)
wherever they appear in the printed output, in both whole-entry and
lines-only mode — but only when highlighting is turned on. Highlighting is
strictly opt-in, matching this tool's plain-by-default posture: piped output
is colored exactly the same as terminal output, there's no auto-detection.
Resolution order, highest precedence first:

1. `NO_COLOR` environment variable set → highlighting is always off,
   regardless of `--color`/config (https://no-color.org).
2. `--no-color` → off.
3. `--color` → on, even when stdout isn't a terminal (like `grep
   --color=always`) — piping `journal -s foo --color | less -R` colors the
   pager's input on purpose.
4. Neither flag → `[color].enabled` from config.toml (§7), default `false`.

#### 3.4.5 Exit status

A search that finds no matches (in either mode) exits `1` with no output, the
same convention as `grep`. A search that finds at least one match exits `0`.

#### 3.4.6 Sort order (`-r`/`--reverse-sort`)

Matching entries (whole-entry mode, §3.4.2) or matching `(entry, lines)`
pairs (lines-only mode, §3.4.3) are sorted by the entry's resolved
timestamp (§2.2) before anything else happens to them — oldest first by
default, regardless of where an entry happens to fall in the file (a
hand-edited or backdated entry need not be in file order). `-r/--reverse-sort`
reverses that order to newest first; like `-a`/`--limit`/`-L`, it's a usage
error (exit code 2) without `-s/--search`.

Sorting happens before `--limit` truncates the result list, so `--limit N`
always caps from the end you're sorted toward: the `N` oldest matches by
default, or the `N` newest with `-r`.

## 4. Human-Facing Display Format

Search results (§3.4) and `-N` output (§3.3) use a different rendering than
the on-disk format (§2): the header is reformatted as a Markdown ATX heading
rather than the on-disk `[...]` bracket form. An ATX heading (`###`), rather
than a `>` blockquote, has no Markdown lazy continuation, so a renderer like
`bat` colors only the header line and not the body line that follows it.

**By default**, the heading shows the entry's fully resolved timestamp in
§2's `TIMESTAMP_FMT`, with no age annotation:

```
### 2026-07-28 14:03:00
124/80/55 @bp @health

```

For a header written exactly this way, that's an unmodified echo of what's
on disk. But display always shows the *resolved* value (§2.2) — a
shorthand header like `[2025]` displays as `### 2025-01-01 00:00:00`, and a
header with no date part at all, like `[08:30]`, displays with today's
date filled in, e.g. `### 2026-08-07 08:30:00` — even though what's
actually sitting in the file is still just `[2025]` or `[08:30]`. Display
is a read-only view; only §2.2's rules about how a header is *written*
control what's ever on disk. This is a deliberate design choice: default
output should be a plain, undecorated view of the entry, consistent with
this tool's plain-by-default posture (§3.4.4 makes the same choice for
color) — useful for scripting, diffing, or just reading at a glance,
without needing `-h` for anything as basic as "what date is this."

**With `-h`/`--human`**, the heading instead shows a configured date,
optionally followed by an elapsed-time annotation:

```
### 2026-07-28 14:03 (3d, 4h)
124/80/55 @bp @health

```

A header missing its date, year, or both (§2.2) still gets the
parenthesized annotation: it resolves to today's date (or, for `MM-DD`
with no year, the current year), which is a concrete point in time to
measure against even though it isn't the entry's actual date (one was
never given).

The date comes from `[timestamp].format` in config.toml (§7): a standard
`chrono`/`strftime` template applied directly to the entry's own timestamp
— any of the usual `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `%A`, `%B`, etc.
directives work. Validated at config-load time: an unrecognized directive
is a config error, not a runtime crash. Default: `%Y-%m-%d %H:%M`.

The parenthesized annotation is controlled by `[timestamp].diff`, one of
three keywords — not a user-authored template. It's a preset choice of
*whether* an annotation shows up at all, and how verbose it is, not
free-form formatting:

| `diff` value | Behavior |
|---|---|
| `disabled` | No annotation at all — just the date, no parens. |
| `short` (default) | Every non-zero calendar unit followed by a zero-padded clock and direction: `3d 04:05:06 ago`. |
| `long` | At most the three highest-order units starting from the first non-zero one, spelled out and pluralized, with a trailing direction word: `3 days, 4 hours ago`. |

Both styles are built from the same underlying elapsed-time breakdown: the
total elapsed time between the entry and now, cascaded into years, months,
days, hours, minutes, and seconds using real calendar arithmetic -- actual
month lengths and leap years, not a fixed-length approximation (e.g. an
entry from exactly one calendar year and one day ago always shows `1 year,
1 day`, regardless of whether that year happened to be a leap year). The
styles render that breakdown differently.

`short` prepends each non-zero calendar component as `Ny`, `Nm`, and `Nd`,
in that order. Zero years, months, or days are omitted. Hours, minutes, and
seconds are always present as a zero-padded `HH:MM:SS` clock, including when
all three are zero. A direction suffix is always present: `ago` for a past
or equal timestamp and `from now` for a future timestamp. Thus a full value
may look like `52y 10m 13d 14:30:58 ago`, while 30 seconds is
`00:00:30 ago`.

`long` uses this unit-selection rule:

1. Find the highest-order unit that's non-zero (skipping any leading zero
   units — e.g. `years`/`months` being zero for a recent entry). This
   anchors a fixed-size window 3 units wide,
   starting at that unit and running downward (e.g. an anchor of `days`
   with a 3-unit window covers `days`/`hours`/`minutes`, and nothing else
   — never `seconds`).
2. Within that window, zero units are dropped from the output — but a zero
   unit in the *middle* of the window doesn't hide a non-zero unit later
   in the *same* window. `4 years, 0 months, 7 days` (window =
   years/months/days) shows as `4 years, 7 days`, not just `4 years`.
3. A non-zero unit *outside* the window never appears, no matter how few
   of the window's own units turn out non-zero. `29 days, 0 hours, 0
   minutes, 10 seconds` with a 3-unit window anchored at `days` only ever
   considers `days`/`hours`/`minutes`; the non-zero `seconds` past the
   window's edge is never reached, so this displays as `29 days`, not `29
   days, 10 seconds`. Put another way: a non-zero value earns a spot
   *inside* the window, never an extension of it.
4. If every unit is zero, fall back to `0 seconds`, so there's always
   something to show.

`long` also ends with `ago` for a past timestamp or `from now` for a future
one. Examples (elapsed time → `long` → `short`):

| Elapsed | `long` | `short` |
|---|---|---|
| 30 seconds | `30 seconds ago` | `00:00:30 ago` |
| 15 min, 22 sec | `15 minutes, 22 seconds ago` | `00:15:22 ago` |
| 3 hr, 1 min, 16 sec | `3 hours, 1 minute, 16 seconds ago` | `03:01:16 ago` |
| 24 days, 13 hours, 1 min | `24 days, 13 hours, 1 minute ago` | `24d 13:01:00 ago` |
| 29 days, 10 sec | `29 days ago` | `29d 00:00:10 ago` |

The last row illustrates the difference: `long` omits seconds beyond its
three-unit window, while `short` always preserves them in the clock.

### 4.1 Suppressing the header (`--no-headers`)

`--no-headers` drops the `###` heading line entirely from `-N` and search
output (in `-L`/`--lines-only` mode, this is the per-entry header line
printed once above that entry's matched lines), leaving just the body. It
also drops the blank line normally used to separate consecutive entries —
that separator is considered part of the heading-based presentation, not
an independent thing, so suppressing the heading suppresses it too. With a
single entry the output is just its body:

```
124/80/55 @bp @health
```

and with several printed back-to-back (e.g. `journal -3 --no-headers`),
each entry's body runs directly into the next with no blank line between
them — whatever separation exists is whatever blank lines happen to
already be inside the bodies themselves.

`--no-header` (singular) also works, as a `clap` `alias` rather than a
second documented flag — it's hidden from `--help`/the man page on
purpose, existing purely so a plausible typo of the real flag still does
the right thing instead of erroring out. Nothing else in this codebase
treats it as a separate flag; it maps onto the exact same `Cli::no_headers`
field and code path as `--no-headers` itself.

It composes with `-h/--human` (which becomes moot — there's no heading
left to format) and with `--color`/highlighting (only body text is ever
highlighted, so this doesn't interact with it). Like `-h`, this is a
display-only flag — it has no effect on `Entry::render`'s on-disk form
(§2).

## 5. File Location Resolution

Resolved in this precedence order:

1. `-f/--file <path>` command-line argument
2. `$JOURNAL_FILE` environment variable (an empty/whitespace-only value is
   treated as unset)
3. XDG default: `$XDG_DATA_HOME/journal/journal.txt`, falling back to
   `~/.local/share/journal/journal.txt` if `$XDG_DATA_HOME` is unset, per the
   XDG Base Directory spec. Parent directories are created automatically for
   this default; an explicit `-f`/`$JOURNAL_FILE` path is expected to already
   have an existing parent directory (`touch`-like semantics).

## 6. Editor Integration

`$EDITOR` is used for the no-argument compose flow (§3.3); if unset, falls
back to `vi`. `$EDITOR`'s value is shell-word split (so quoted paths with
spaces work), giving a program name and its own leading arguments.

`$EDITOR` is launched directly on the real journal file — no temporary
buffer, no seeded content, no diffing, no atomic replace. The file is
created empty first (§5) if it doesn't already exist, so the editor always
has something to open. Whatever the editor writes to the file by the time
it exits is exactly what ends up on disk; quitting without saving simply
means the file was never written, so it's unchanged as a natural
consequence of editing in place, not anything `journal` detects or
special-cases.

A non-zero editor exit status is reported as an error (exit code 1). Since
there's no temp file or atomic replace involved, this says nothing about
whether the file was modified before the editor exited — whatever the
editor had already written to disk stays written.

## 7. Config File

An optional TOML file at `$XDG_CONFIG_HOME/journal/config.toml` (falling
back to `~/.config/journal/config.toml`, same resolution as §5) configures
default highlighting and `-h/--human` display:

```toml
[color]
enabled = false

[timestamp]
format = "%Y-%m-%d %H:%M"
diff = "short"
```

`[color].enabled` sets the default for search-term highlighting (§3.4.4)
when neither `--color` nor `--no-color` is passed. Default `false`: color
is opt-in, not assumed, matching this tool's plain-by-default posture — a
user who wants highlighting everywhere flips this once, rather than typing
`--color` on every invocation.

`[timestamp].format` and `[timestamp].diff` control `-h/--human`'s display
(§4) — `format` is a `strftime` template for the date, `diff` is one of
`disabled`/`short`/`long` selecting the elapsed-time annotation's verbosity
(not a user-authored template — see §4 for why). Defaulting to §2's plain
`TIMESTAMP_FMT` with no age annotation (§4), rather than some baked-in
"friendly" format, means the config's defaults only ever take effect when
the user has explicitly opted into human-formatted output — nothing
changes for scripts or pipelines that never pass `-h`.

A missing config file is not an error, nor is a config file that only sets
some of these tables/keys — everything not specified falls back to its
documented default, table by table and key by key.

## 8. Environment Variables

| Variable | Purpose |
|---|---|
| `EDITOR` | Editor launched by no-argument invocation (§3.3, §6); falls back to `vi` |
| `JOURNAL_FILE` | Journal file path override (§5) |
| `XDG_DATA_HOME` | Base directory for the default journal file location (§5) |
| `XDG_CONFIG_HOME` | Base directory for the config file (§7) |
| `NO_COLOR` | Disables search-term highlighting when set, overriding `--color` and `[color].enabled` too (§3.4.4) |

## 9. Exit Codes

| Code | Meaning |
|---|---|
| 0 | Success — entry appended/saved, `-N` printed (or the journal was empty), or a search found at least one match |
| 1 | Runtime error (I/O failure, editor exited non-zero, etc.), early EOF in interactive setup, or a search found no matches |
| 2 | Usage error — an invalid flag combination, caught either by `clap` itself (e.g. `-s` combined with `-t`) or by explicit validation (e.g. `-a` without `-s`, `-N` combined with `-s`, `-t/--tags` without entry text) |

## 10. Concurrency / Write Safety

Since the journal file may be touched by multiple invocations concurrently
(e.g. a cron job and an interactive session), writes are serialized with an
exclusive lock (`flock`) taken on a stable sidecar file, `<path>.lock` —
not the journal file itself. Both the non-editor append path and the
editor-mode session (§6) take this same lock; the editor holds it for the
entire session, so a concurrent `journal` append blocks until the editor
exits rather than racing it, and the two can't corrupt each other's
writes. This protection only covers `journal` invocations themselves — if
something else (a different program, or the same journal file opened
directly in an editor outside of `journal`) writes to the file without
going through this lock, that write isn't serialized against it. The
write itself is also opened in append (`O_APPEND`) mode as defense in
depth.

## 11. Output Stream Discipline

Journal entries and search results are written to `stdout`; errors, warnings,
and diagnostics go to `stderr`. This allows `journal -s foo > results.txt` to
work predictably, and keeps `journal`'s output composable in shell
pipelines.

## 12. Diagnostics (`-v/--verbose`)

`-v/--verbose` prints diagnostic lines to stderr — never stdout, per §11 —
and can be combined with any mode. Without it, a successful run is
completely silent on stderr, the usual Unix convention. Diagnostics
currently emitted:

| When | Diagnostic |
|---|---|
| After resolving the journal file (§5) | `using journal file <path>` |
| Around the sidecar lock (§10) | `acquiring lock at <path>.lock`, then `lock released` |
| After appending an entry | `appended entry at <timestamp>` |
| Editor mode: launching `$EDITOR` | `launching editor: <program> <args>` |
| Editor mode: outcome | `editor exited` |
| `-N` | `<total> entries in journal, printing last <n>` |
| `-s/--search` (either mode) | `<n> entries matched` |

## 13. Man Page / `--help`

`--help` and `-V/--version` are provided by `clap`. Note that `-h` is *not*
`--help` here — it's repurposed for `-h/--human` (§4), so `clap`'s usual
automatic `-h` short flag for help is disabled and `--help` only has a long
form. A man page is checked in at `man/journal.1`, generated from the same
flag definitions via `cargo run --example man` (not regenerated
automatically at build time — see `CLAUDE.md` for the regeneration
workflow). The end of `--help` also prints the currently resolved journal
file path, honoring the usual `-f` > `$JOURNAL_FILE` > XDG precedence.

## 14. Possible Future Work

Not currently implemented; listed here as known gaps rather than mixed into
the normative sections above:

1. A way to list/filter entries by date range.
2. Config-driven defaults for more flags, generalizing the precedent
   `[color].enabled` (§7) already sets. Of the three standard Unix ways to
   make a command always run with certain flags — a shell alias, a
   program-parsed environment variable (e.g. `less`'s `LESS`, POSIX
   `MAKEFLAGS`), or a per-user rc/config file (e.g. `~/.curlrc`,
   `~/.wgetrc`) — the config-file approach is the right fit here, since
   `journal` already has one, it's inherited by non-interactive contexts
   (scripts, cron) unlike a shell alias, and it avoids the shell-quoting
   hazards an env-var-of-flags approach is prone to (see GNU grep's now
   removed `GREP_OPTIONS`). Candidates if pursued: `[defaults].human`
   (default `-h/--human` on), `[defaults].all` (default `-a/--all` on for
   searches), `[search].limit` (a default `--limit`). Not implemented
   speculatively — add only if user testing surfaces an actual need for a
   given flag to be always-on, following the same `[table].key` shape and
   CLI-flag-overrides-config precedence `[color].enabled` already
   establishes.
