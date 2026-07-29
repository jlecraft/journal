# journal

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

A command-line utility for appending timestamped, taggable entries to a
plain-text journal file, and searching that file by tag or keyword.

Entries are stored in a flat, human-readable format, so the underlying file
stays inspectable and editable by hand -- no database, no lock-in.

```
[2026-07-28.14:03:00] @bp @health
124/80/55

[2026-07-28.20:41:55] @sleep
slept 7 hours
```

## Features

- Append timestamped entries from the command line, `$EDITOR`, or stdin
- Tag entries inline (`@tag`) or via `-t/--tags`, merged and de-duplicated
- Search by tag (exact match) or keyword (case-insensitive substring), with
  AND/OR term combining and a result limit
- Journal file location resolved via `-f`, `$JOURNAL_FILE`, or the XDG data
  directory, in that order
- Concurrency-safe: locked writes so a cron job and an interactive session
  can't corrupt each other's entries
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

Trailing `@tag` tokens at the end of the entry text are extracted
automatically and hoisted onto the timestamp line. Tags can also be given
explicitly and combined with inline tags:

```sh
journal -t "@sleep" "slept 7 hours"
```

Pipe entry text in instead of passing it as an argument:

```sh
echo "back from a walk @exercise" | journal -
```

Run `journal` with no arguments to open the entry in `$EDITOR` (falling back
to `vi`), with a new timestamp line pre-inserted. The real journal file is
only touched if you actually save -- aborting the editor (`:q!`, or exiting
without writing) leaves it untouched.

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
`@`-prefixed terms are the exception: they're matched only against an
entry's tags, and require a full-word match (`@bp` won't match `@bph`).

### Journal file location

Resolved in this order:

1. `-f/--file <path>`
2. `$JOURNAL_FILE`
3. `$XDG_DATA_HOME/journal/journal.txt`, falling back to
   `~/.local/share/journal/journal.txt`

## Environment variables

| Variable         | Purpose                                             |
|-------------------|------------------------------------------------------|
| `EDITOR`          | Editor launched by no-argument invocation (falls back to `vi`) |
| `JOURNAL_FILE`     | Journal file path (see resolution order above)       |
| `XDG_DATA_HOME`    | Base directory for the default journal file location |

## Exit codes

| Code | Meaning                                              |
|------|-------------------------------------------------------|
| 0    | Success (entry appended/saved, or search found a match) |
| 1    | Runtime error, or a search found no matches           |
| 2    | Usage error (invalid flags/arguments)                 |

## Entry format

```
[YYYY-MM-DD.HH:MM:SS] @tag1 @tag2
Entry body, line 1
Entry body, line 2 (optional)

```

Every entry ends with exactly one blank line, which the tool maintains
automatically regardless of what you type. See
[`journal-cli-spec.md`](journal-cli-spec.md) for the full design spec and
the rationale behind each behavior.

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets
```

## License

Licensed under the [GNU General Public License v3.0 or later](LICENSE).
