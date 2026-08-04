# journal

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

A command-line utility for appending timestamped, taggable entries to a
plain-text journal file, and searching that file by tag or keyword.

Entries are stored in a flat, human-readable format, so the underlying file
stays inspectable and editable by hand -- no database, no lock-in.

```
[2026-07-28.14:03:00]
124/80/55 @bp @health

[2026-07-28.20:41:55]
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
editor (`:q!`, or exiting without writing) leaves it untouched.

Positioning the cursor requires an editor-specific command-line flag (vi/vim
and friends use `+N`; GUI editors vary), so there's no single default that
works everywhere. See [Editor configuration](#editor-configuration) below to
set it up for your `$EDITOR`.

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

Matched terms are highlighted (bold red, like `grep --color`) whenever
stdout is an interactive terminal; piped output (a file, `less`, a Markdown
renderer like `bat`) stays plain text, and `NO_COLOR` disables it too.

### Show the last N entries

```sh
journal -3
```

Prints the 3 most recent entries, oldest to newest (like `tail`). Can't be
combined with entry text, `-t/--tags`, or `-s/--search`.

### Journal file location

Resolved in this order:

1. `-f/--file <path>`
2. `$JOURNAL_FILE`
3. `$XDG_DATA_HOME/journal/journal.txt`, falling back to
   `~/.local/share/journal/journal.txt`

### Editor configuration

`journal`'s no-argument mode seeds a blank line for the cursor to land on,
but *moving* the cursor there requires a command-line flag specific to your
`$EDITOR` -- there's no flag that works across every editor, so it's opt-in
via a config file at `$XDG_CONFIG_HOME/journal/config.toml` (falling back to
`~/.config/journal/config.toml`):

```toml
# ~/.config/journal/config.toml
[editor]
args = "+{line}"
```

`args` is parsed with the same shell-word splitting as `$EDITOR` itself
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
| `XDG_CONFIG_HOME`  | Base directory for the config file (see Editor configuration) |

## Exit codes

| Code | Meaning                                              |
|------|-------------------------------------------------------|
| 0    | Success (entry appended/saved, or search found a match) |
| 1    | Runtime error, or a search found no matches           |
| 2    | Usage error (invalid flags/arguments)                 |

## Entry format

```
[YYYY-MM-DD.HH:MM:SS]
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

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets
```

## License

Licensed under the [GNU General Public License v3.0 or later](LICENSE).
