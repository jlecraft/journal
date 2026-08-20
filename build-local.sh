#!/bin/sh
set -eu

cd "$(dirname "$0")"

cargo build --release
cargo run --example man

man_dir="${XDG_DATA_HOME:-$HOME/.local/share}/man/man1"
install -Dm644 man/journal.1 "$man_dir/journal.1"

printf 'Release binary: %s\nMan page: %s\n' \
    "$PWD/target/release/journal" "$man_dir/journal.1"
