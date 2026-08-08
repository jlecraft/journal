# journal

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

A command-line utility for appending timestamped, taggable entries to a
plain-text journal file, and searching that file by tag or keyword.

Entries are stored in a flat, human-readable format, so the underlying file
stays inspectable and editable by hand -- no database, no lock-in.

```
[2026-07-28 14:03:00]
124/80/55 @bp @health

[2026-07-28 20:41:55]
slept 7 hours
@sleep
```

## Features

- Append timestamped entries from the command line, `$EDITOR`, or stdin
- Tag entries by typing `@tag` anywhere in the text, or via `-t/--tags`
  (bare words are auto-prefixed with `@` and appended as their own line)
- Search by tag (exact match) or keyword (case-insensitive substring), with
  AND/OR term combining, a result limit, and an optional lines-only view
  (`-L`) that prints just the matching lines under each entry's header
- Show the last N entries with `-N` (e.g. `journal -3`)
- Plain, exact timestamps by default; `-h/--human` switches to a
  configurable formatted date with an optional elapsed-time annotation
- Search-term highlighting is opt-in (`--color`/`[color].enabled`), never
  auto-detected -- piped output looks the same as terminal output
- Journal file location resolved via `-f`, `$JOURNAL_FILE`, or the XDG data
  directory, in that order
- Concurrency-safe: locked writes so a cron job and an interactive session
  can't corrupt each other's entries
- `-v/--verbose` prints diagnostics (resolved file, lock status, editor
  invocation) to stderr only, without touching stdout output you might pipe
- Interrupting the editor flow (`Ctrl-C`) cleans up its temporary edit
  buffer instead of leaking it
- Standard Unix CLI conventions: proper exit codes, stdin support (`-`),
  stdout/stderr discipline, `-h/--help`, `-V/--version`, a man page

## Installation

### From source

```sh
git clone https://github.com/jlecraft/journal.git
cd journal
cargo build --release
```

The binary is at `target/release/journal`; put it on your `$PATH`, e.g.:

```sh
install -Dm755 target/release/journal ~/.local/bin/journal
```

### Directly via cargo

```sh
cargo install --git https://github.com/jlecraft/journal.git
```

### Man page

A man page is checked in at `man/journal.1`:

```sh
man ./man/journal.1
# or install it, e.g.:
install -Dm644 man/journal.1 ~/.local/share/man/man1/journal.1
```

Regenerate it after changing `src/cli.rs` with `cargo run --example man`.

## Usage

### Append an entry

```sh
journal "124/80/55 @bp @health"
```

`@tag` tokens can appear anywhere in the text and stay right where you typed
them -- the timestamp line is always alone on its own line. Tags can also be
given via `-t/--tags`, which appends them as their own line at the end of
the entry; bare words are automatically prefixed with `@`:

```sh
journal -t "sleep" "slept 7 hours"      # appends "@sleep" as its own line
journal -t "beer @store" "grabbed a six-pack"  # appends "@beer @store"
```

Pipe entry text in instead of passing it as an argument:

```sh
echo "back from a walk @exercise" | journal -
```

Run `journal` with no arguments to open the entry in `$EDITOR` (falling back
to `vi`), with a new timestamp line pre-inserted and the cursor left on the
blank line right after it, ready to type the body. `-t/--tags` works here
too -- its tags line is pre-seeded at the end of the entry, after the blank
line the cursor starts on:

```sh
journal -t "sleep"
```

The real journal file is only touched if you actually save -- aborting the
editor (`:q!`, or exiting without writing) leaves it untouched. Interrupting
the editor itself (`Ctrl-C`) is handled the same way: the temporary edit
buffer is removed and the real journal file is left untouched.

Positioning the cursor requires an editor-specific command-line flag (vi/vim
and friends use `+N`; GUI editors vary), so there's no single default that
works everywhere. See [Editor cursor positioning](#editor-cursor-positioning)
below to set it up for your `$EDITOR`.

### Search

```sh
journal -s "@bp"              # entries tagged @bp (exact tag match)
journal -s "fm radio"         # entries containing "fm" OR "radio"
journal -s "fm radio" -a      # entries containing "fm" AND "radio"
journal -s "linux+kernel"     # entries containing the phrase "linux kernel"
journal -s "th" --limit 5     # cap the number of results
```

Non-tag search terms are case-insensitive substring matches -- a search for
`"th"` matches inside `"weather"` or `"month"` too. This is broad by design.
`@`-prefixed terms are the exception: they require a full-word match against
a `@tag` token found anywhere in the entry (`@bp` won't match `@bph`). A
bare word also finds its `@`-prefixed form via ordinary substring matching --
`journal -s "blood_pressure"` finds `@blood_pressure` the same as
`journal -s "@blood_pressure"` does.

For a long entry, `-L/--lines-only` prints just the matching lines under
the entry's header instead of the full body:

```sh
journal -s "fm radio" -L        # header, then only lines containing "fm" or "radio"
journal -s "fm radio" -a -L     # header, then only lines containing BOTH "fm" and "radio"
```

Combined with `-a/--all`, `-L` requires both terms on the *same* line to
qualify -- stricter than plain `-a`, which is satisfied if the terms are
spread across different lines anywhere in the entry.

Matched terms can be highlighted (bold red, like `grep --color`), but it's
off by default -- pass `--color` to turn it on for one run:

```sh
journal -s "fm radio" --color        # highlight matches, even if piped
journal -s "fm radio" --color | less -R
journal -s "fm radio" --no-color     # force it off, overriding config
```

`--color`/`--no-color` are mutually exclusive. With neither passed, the
default comes from `[color].enabled` in config.toml (see Configuration
below), itself defaulting to off. `NO_COLOR`, if set, always wins over
both. Unlike some tools, `--color` colors piped output too -- it's an
explicit request, not an auto-detected terminal capability.

### Show the last N entries

```sh
journal -3
```

Prints the 3 most recent entries, oldest to newest (like `tail`). Can't be
combined with entry text, `-t/--tags`, or `-s/--search`.

### Timestamp display

By default, every header shows the entry's fully resolved timestamp, with
no age annotation. For a header written in full already, that's an
unmodified echo of what's on disk; a hand-typed shorthand header (see
below) is always shown expanded, even without `-h`:

```
### 2026-07-28 14:03:00
124/80/55 @bp @health
```

Pass `-h/--human` for a friendlier, configurable rendering instead:

```sh
journal -3 -h
journal -s "@bp" -h
```

```
### 2026-07-28 14:03 (3d, 4h)
124/80/55 @bp @health
```

The date comes from `[timestamp].format` in config.toml (a standard
strftime template -- see Configuration below). The parenthesized part is
controlled by `[timestamp].diff`, one of three preset styles rather than a
free-form template:

| `diff` value | Behavior | Example |
|---|---|---|
| `disabled` | No annotation at all -- just the date | `### 2026-07-28 14:03` |
| `short` (default) | At most the 2 highest-order units from the first non-zero one, abbreviated, no direction word | `3d, 4h` |
| `long` | At most the 3 highest-order units from the first non-zero one, spelled out, with a trailing direction word | `3 days, 4 hours ago` |

Both styles pick units the same way: find the highest non-zero unit
(years/months/days/hours/minutes/seconds) and anchor a fixed-size window
there -- 2 units wide for `short`, 3 for `long`. Units outside that window
never appear, no matter how many of the window's own units turn out to be
zero; units *inside* the window that happen to be zero are just dropped
from the output, without hiding a non-zero unit elsewhere in the same
window. So `4 years, 0 months, 7 days` (window = years/months/days) shows
as `4 years, 7 days`, while `29 days, 0 hours, 0 minutes, 10 seconds`
(window = days/hours/minutes for `long`) shows as `29 days` -- the
non-zero seconds sits past the window's edge and is never reached.

More examples (elapsed time -> `long` -> `short`):

| Elapsed | `long` | `short` |
|---|---|---|
| 30 seconds | `30 seconds ago` | `30s` |
| 15 min, 22 sec | `15 minutes, 22 seconds ago` | `15m, 22s` |
| 3 hr, 1 min, 16 sec | `3 hours, 1 minute, 16 seconds ago` | `3h, 1m` |

See `journal-cli-spec.md` §4 for the full rationale (why exact-by-default,
why `diff` is a preset rather than a template).

Pass `--no-headers` to drop the header line entirely and print just the
body -- useful when you only want the text, e.g. piping into another tool.
It also drops the blank line that normally separates entries, so with
multiple entries printed at once, bodies run directly into each other:

```sh
journal -3 --no-headers
journal -s "@bp" -L --no-headers
```

This only affects what's printed; it has no effect on what's stored on
disk.

### Diagnostics

```sh
journal -v "test entry"       # prints resolved file, lock status, etc. to stderr
journal -v -s foo
```

`-v/--verbose` prints diagnostic information -- the resolved journal file,
lock acquisition/release, and (in editor mode) the resolved `$EDITOR`
command and save/abort outcome -- to stderr only, so it never mixes into
stdout output you might be piping elsewhere. It can be combined with any
other mode.

### Journal file location

Resolved in this order:

1. `-f/--file <path>`
2. `$JOURNAL_FILE`
3. `$XDG_DATA_HOME/journal/journal.txt`, falling back to
   `~/.local/share/journal/journal.txt`

### Configuration

`journal` reads an optional config file at
`$XDG_CONFIG_HOME/journal/config.toml` (falling back to
`~/.config/journal/config.toml`):

```toml
# ~/.config/journal/config.toml
[editor]
args = "+{line}"

[color]
enabled = false

[timestamp]
format = "%Y-%m-%d %H:%M"
diff = "short"
```

A missing file, or one that only sets some of these, is not an error --
anything unspecified falls back to the default shown above.

`[color].enabled` sets the default for search-term highlighting (see
Search above) when neither `--color` nor `--no-color` is passed.

`[timestamp].format` and `[timestamp].diff` control `-h/--human`'s output
(see Timestamp display above) -- `format` is a standard `strftime`/chrono
template applied to the entry's own timestamp; `diff` selects
`disabled`/`short`/`long`, the elapsed-time annotation's verbosity.

#### Editor cursor positioning

`journal`'s no-argument mode seeds a blank line for the cursor to land on,
but *moving* the cursor there requires a command-line flag specific to your
`$EDITOR` -- there's no flag that works across every editor, so it's opt-in
via `[editor].args` above. `args` is parsed with the same shell-word splitting as `$EDITOR` itself
(quoting works the same way), and `{line}` is replaced with the 1-indexed
line number of the blank line where you should start typing. The resulting
arguments are inserted right before the file path, ahead of anything else in
`$EDITOR`.

**vim/vi/nvim** understand `+N` as "open with the cursor on line N", which is
all you need:

```toml
[editor]
args = "+{line}"
```

If you'd rather land straight in insert mode instead of normal mode, chain a
`-c` command (vim runs `+`/`-c` arguments in the order given):

```toml
[editor]
args = "+{line} -c startinsert"
```

**nano** uses the same `+LINE` convention, so `args = "+{line}"` works there
too. Other editors vary -- e.g. GUI editors that take a `file:line` argument
instead -- so check your editor's documentation for its equivalent flag.

## Environment variables

| Variable         | Purpose                                             |
|-------------------|------------------------------------------------------|
| `EDITOR`          | Editor launched by no-argument invocation (falls back to `vi`) |
| `JOURNAL_FILE`     | Journal file path (see resolution order above)       |
| `XDG_DATA_HOME`    | Base directory for the default journal file location |
| `XDG_CONFIG_HOME`  | Base directory for the config file (see Configuration) |
| `NO_COLOR`         | Disables search-term highlighting, overriding `--color` and `[color].enabled` too |

## Exit codes

| Code | Meaning                                              |
|------|-------------------------------------------------------|
| 0    | Success (entry appended/saved, or search found a match) |
| 1    | Runtime error, or a search found no matches           |
| 2    | Usage error (invalid flags/arguments)                 |
| 130  | Editor flow interrupted (`Ctrl-C`/`SIGINT`/`SIGTERM`); the temporary edit buffer is cleaned up before exiting |

## Entry format

```
[YYYY-MM-DD HH:MM:SS]
Entry body, line 1
Entry body, line 2 (optional)
@tag1 @tag2 (optional, only present if -t/--tags was used)

```

The timestamp is always alone on its own line. `@tag` tokens are otherwise
just ordinary text -- they can appear anywhere in the body, wherever they
were typed. Every entry ends with exactly one blank line, which the tool
maintains automatically regardless of what you type. See
[`journal-cli-spec.md`](journal-cli-spec.md) for the full design spec and
the rationale behind each behavior.

A header you hand-type or hand-edit doesn't need full precision -- a date
part (`YYYY`, `MM-DD`, or `YYYY-MM-DD`) and a time part (`HH:MM` or
`HH:MM:SS`) can each be given or left out independently. `[1972]` displays
as `1972-01-01 00:00:00` (missing month/day default to `01-01`), and
`[1972 08:30]` as `1972-01-01 08:30:00` (missing seconds default to `00`).
A header with a date part but no year -- `[08-07]` (month-day only) --
displays with the *current year* filled in instead, e.g.
`2026-08-07 00:00:00`. A header with **no date part at all** -- just a
time, like `[08:30]` -- displays with **today's actual date** filled in,
e.g. `2026-08-07 08:30:00`, not just the current year. Since that's always
a real, concrete point in time, `-h/--human` computes an elapsed-time
annotation for it same as any other entry, even though the date behind it
wasn't actually given.

This expansion only ever affects what's printed, never what's stored --
`journal` doesn't rewrite a header just because it read the file. `[1972]`
and `[08:30]` stay exactly that on disk forever, even after an editor
session that touches the entry they belong to; only a brand-new entry
(`journal "..."`, stdin, or an editor timestamp you left untouched) is
ever written in full. See §2.2 of the spec for the full grammar.

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets
```

## License

Licensed under the [GNU General Public License v3.0 or later](LICENSE).
