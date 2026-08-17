# AppImage Packaging

AppImage packaging is currently deferred. The maintained installation path
remains Cargo, as documented in `README.md`:

```sh
cargo install --git https://github.com/jlecraft/journal.git
```

The following process documents how AppImage support can be added later.
AppImage is somewhat awkward for a terminal-only utility: users normally invoke
the artifact by its full filename, such as `journal-0.1.0-x86_64.AppImage`, and
it does not automatically install `journal` on `$PATH` or install its man page.
It is therefore best offered as an additional distribution format rather than
as a replacement for Cargo installation or a conventional binary archive.

## 1. Build a release binary

Build and strip a release binary for every architecture that will be
distributed:

```sh
cargo build --release --locked
strip target/release/journal
```

The current GNU/Linux binary is dynamically linked against glibc and
`libgcc_s`. Do not bundle glibc in the AppImage. Build on the oldest Linux
distribution that the release intends to support, because a binary built
against an older glibc generally runs on newer glibc releases, while the
reverse is not guaranteed.

For broader payload portability, consider a statically linked musl build:

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl
strip target/x86_64-unknown-linux-musl/release/journal
```

Build separate AppImages for each supported architecture. At minimum, likely
release targets are `x86_64` and `aarch64`; AppImages are architecture-specific.

## 2. Assemble an AppDir

Create an AppDir with the following structure:

```text
Journal.AppDir/
├── AppRun
├── journal.desktop
├── journal.png
├── .DirIcon -> journal.png
└── usr/
    ├── bin/
    │   └── journal
    └── share/
        ├── doc/journal/
        │   ├── LICENSE
        │   └── README.md
        ├── icons/hicolor/256x256/apps/
        │   └── journal.png
        └── man/man1/
            └── journal.1
```

An AppDir requires an `AppRun` entry point, one desktop file in its root, and
an icon matching the desktop file's `Icon` value. The repository does not
currently contain an application icon, so one must be designed and added
before packaging.

### AppRun

Create `Journal.AppDir/AppRun`:

```sh
#!/bin/sh
APPDIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exec "$APPDIR/usr/bin/journal" "$@"
```

Make it executable:

```sh
chmod 755 Journal.AppDir/AppRun
```

The `"$@"` forwarding is important because it preserves all arguments passed
to the AppImage.

### Desktop entry

Create `Journal.AppDir/journal.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=Journal
Comment=Append and search timestamped plain-text journal entries
Exec=journal
Icon=journal
Terminal=true
Categories=Utility;
```

`Terminal=true` is required because `journal` is a command-line application.
Validate the file when `desktop-file-validate` is available:

```sh
desktop-file-validate Journal.AppDir/journal.desktop
```

### Binary and supporting files

Copy the release binary to `Journal.AppDir/usr/bin/journal` and ensure it is
executable. Copy `LICENSE`, `README.md`, and `man/journal.1` into the paths
shown above. Install the icon both at the AppDir root and under the hicolor
icon hierarchy, then create `.DirIcon` as a symbolic link to the root icon.

## 3. Package the AppDir

Download and verify a pinned release of `appimagetool` from the official
project rather than permanently depending on a moving continuous build. Use
it to convert the AppDir into an AppImage:

```sh
ARCH=x86_64 \
VERSION=0.1.0 \
./appimagetool-x86_64.AppImage \
    Journal.AppDir \
    journal-0.1.0-x86_64.AppImage
```

In a container or another environment where FUSE is unavailable, use
AppImage's extraction mode:

```sh
ARCH=x86_64 \
VERSION=0.1.0 \
APPIMAGE_EXTRACT_AND_RUN=1 \
./appimagetool-x86_64.AppImage \
    Journal.AppDir \
    journal-0.1.0-x86_64.AppImage
```

Current `appimagetool` releases can obtain the appropriate AppImage runtime
automatically. For reproducible or offline builds, download and verify a
pinned runtime separately and pass it with `--runtime-file`.

## 4. Test the artifact

Make the resulting file executable and first check its basic entry points:

```sh
chmod +x journal-0.1.0-x86_64.AppImage
./journal-0.1.0-x86_64.AppImage --version
./journal-0.1.0-x86_64.AppImage --help
```

Then exercise actual journal behavior using a temporary directory:

```sh
tmpdir="$(mktemp -d)"
artifact="$PWD/journal-0.1.0-x86_64.AppImage"

"$artifact" -f "$tmpdir/journal.txt" "first entry @test"
"$artifact" -f "$tmpdir/journal.txt" -s "@test"
"$artifact" -f "$tmpdir/journal.txt" -1
"$artifact" -f "$tmpdir/journal.txt" --all-tags
EDITOR=true "$artifact" -f "$tmpdir/journal.txt"
```

Also verify all of the following:

- Command-line arguments reach `journal` through `AppRun` unchanged.
- Exit codes remain `0`, `1`, and `2` as documented.
- `$EDITOR`, `$JOURNAL_FILE`, and XDG data and configuration paths work.
- Lock sidecars are created beside the selected journal file, not inside the
  mounted AppImage.
- The artifact runs on the oldest supported Linux distribution as well as a
  current distribution.
- Extraction mode works on a system without FUSE, for example with
  `--appimage-extract-and-run --help`.
- `ldd` or an equivalent inspection finds no unexpected shared-library
  dependencies in the embedded binary.
- The repository's normal `cargo test` and
  `cargo clippy --all-targets -- -D warnings` checks still pass before an
  artifact is published.

## 5. Automate packaging

A maintainable implementation should add files similar to:

```text
packaging/appimage/AppRun
packaging/appimage/journal.desktop
assets/journal.png
scripts/build-appimage.sh
.github/workflows/release.yml
```

The build script should:

1. Derive the version from `Cargo.toml` rather than duplicating it.
2. Accept or determine the target architecture explicitly.
3. Build the release binary with `--locked`.
4. Create a fresh AppDir and populate all required files.
5. Validate the desktop entry.
6. Inspect the binary's shared-library dependencies.
7. Invoke a pinned and checksum-verified `appimagetool` and runtime.
8. Smoke-test the completed artifact.
9. Generate a SHA-256 checksum.

The release workflow should run the normal test and lint checks before
packaging, then build and publish architecture-specific artifacts such as:

```text
journal-0.1.0-x86_64.AppImage
journal-0.1.0-x86_64.AppImage.sha256
journal-0.1.0-aarch64.AppImage
journal-0.1.0-aarch64.AppImage.sha256
```

AppImage signing and zsync update metadata can be added afterward if there is
a concrete distribution or update-channel requirement. They are not necessary
for the first AppImage release, but release checksums should be provided from
the beginning.

## References

- [AppDir specification](https://docs.appimage.org/reference/appdir.html)
- [Manual AppImage packaging](https://docs.appimage.org/packaging-guide/manual.html)
- [Packaging native binaries](https://docs.appimage.org/packaging-guide/from-source/native-binaries.html)
- [Official appimagetool repository](https://github.com/AppImage/appimagetool)
- [AppImage FUSE guidance](https://github.com/AppImage/AppImageKit/wiki/FUSE)
