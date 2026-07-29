use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Journal's optional config file, resolved from
/// `$XDG_CONFIG_HOME/journal/config.toml` (falling back to
/// `~/.config/journal/config.toml` per the XDG Base Directory spec, same
/// as the data file default in §4). A missing file is not an error --
/// it just means no overrides are in effect.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub editor: EditorConfig,
}

/// Editor invocation overrides for the no-argument compose flow (§5.1).
/// Needed because positioning the cursor after opening a fresh entry
/// requires an editor-specific command-line argument (e.g. `+N` for
/// vi/vim/nano, `+N` for emacs too, but GUI editors vary) -- there's no
/// single flag that works everywhere, so it's left to the user to
/// configure for their own `$EDITOR`.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct EditorConfig {
    /// Extra arguments passed to $EDITOR, inserted just before the file
    /// path, with `{line}` replaced by the 1-indexed line number of the
    /// blank line where the user should start typing (right after the
    /// newly-seeded timestamp, or between it and any `-t/--tags` line).
    /// Parsed with the same shell-word splitting as $EDITOR itself, so
    /// quoting works the same way, e.g. `+{line} -c "startinsert"`.
    pub args: Option<String>,
}

/// Loads the config file if one exists, or `Config::default()` if not.
pub fn load() -> Result<Config> {
    load_from(config_path())
}

fn load_from(path: Option<PathBuf>) -> Result<Config> {
    let Some(path) = path else {
        return Ok(Config::default());
    };
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file at {}", path.display()))?;
    toml::from_str(&text)
        .with_context(|| format!("failed to parse config file at {}", path.display()))
}

fn config_path() -> Option<PathBuf> {
    xdg::BaseDirectories::with_prefix("journal").find_config_file("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(Some(dir.path().join("no-such-config.toml"))).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn no_config_path_yields_defaults() {
        let config = load_from(None).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn parses_editor_args_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[editor]\nargs = \"+{line}\"\n").unwrap();
        let config = load_from(Some(path)).unwrap();
        assert_eq!(config.editor.args, Some("+{line}".to_string()));
    }

    #[test]
    fn empty_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let config = load_from(Some(path)).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn malformed_toml_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not valid toml [[[").unwrap();
        assert!(load_from(Some(path)).is_err());
    }
}
