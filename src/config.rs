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
    pub color: ColorConfig,
    #[serde(default)]
    pub timestamp: TimestampConfig,
}

/// Whether search-term highlighting (§3.4.4) is on by default when neither
/// `--color` nor `--no-color` is passed on the command line. Off by
/// default, matching this tool's plain-by-default posture -- color is
/// opt-in, not auto-detected.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ColorConfig {
    pub enabled: bool,
}

/// Controls how `-h/--human` renders a timestamp header. `format` is a
/// standard chrono strftime template applied to the entry's own
/// timestamp. `diff` selects whether/how an elapsed-time annotation is
/// shown next to it -- `entry::DiffStyle` owns the actual rendering, since
/// it's a display concept that belongs with the rest of `entry.rs`'s
/// timestamp-formatting logic. Without `-h/--human`, neither of these is
/// consulted -- the default display is an unmodified echo of the on-disk
/// timestamp.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TimestampConfig {
    pub format: String,
    pub diff: crate::entry::DiffStyle,
}

impl Default for TimestampConfig {
    fn default() -> Self {
        TimestampConfig {
            format: "%Y-%m-%d %H:%M".to_string(),
            diff: crate::entry::DiffStyle::default(),
        }
    }
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
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("failed to parse config file at {}", path.display()))?;
    validate_timestamp_format(&config.timestamp.format)?;
    Ok(config)
}

/// Validates `[timestamp].format` at load time so a bad strftime
/// directive surfaces as a clean config error here, rather than as a
/// panic inside chrono's `Display` impl deep in `entry::display_header`
/// (chrono's `.format()` doesn't validate the template itself -- an
/// unrecognized directive only fails when the resulting `DelayedFormat`
/// is later displayed). `[timestamp].diff` needs no equivalent check: it
/// deserializes straight into `entry::DiffStyle`, so an invalid value
/// (anything other than `disabled`/`short`/`long`) is already rejected by
/// `toml::from_str` itself, before this function even runs.
fn validate_timestamp_format(fmt: &str) -> Result<()> {
    use chrono::format::{Item, StrftimeItems};
    if StrftimeItems::new(fmt).any(|item| item == Item::Error) {
        anyhow::bail!("invalid [timestamp] format string: {fmt:?}");
    }
    Ok(())
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

    #[test]
    fn color_defaults_to_disabled() {
        let config = load_from(None).unwrap();
        assert!(!config.color.enabled);
    }

    #[test]
    fn parses_color_enabled_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[color]\nenabled = true\n").unwrap();
        let config = load_from(Some(path)).unwrap();
        assert!(config.color.enabled);
    }

    #[test]
    fn timestamp_defaults_when_table_absent() {
        let config = load_from(None).unwrap();
        assert_eq!(config.timestamp, TimestampConfig::default());
    }

    #[test]
    fn parses_timestamp_format_and_diff_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[timestamp]\nformat = \"%H:%M\"\ndiff = \"long\"\n").unwrap();
        let config = load_from(Some(path)).unwrap();
        assert_eq!(config.timestamp.format, "%H:%M");
        assert_eq!(config.timestamp.diff, crate::entry::DiffStyle::Long);
    }

    #[test]
    fn timestamp_partial_table_fills_missing_key_with_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[timestamp]\nformat = \"%H:%M\"\n").unwrap();
        let config = load_from(Some(path)).unwrap();
        assert_eq!(config.timestamp.format, "%H:%M");
        assert_eq!(config.timestamp.diff, TimestampConfig::default().diff);
    }

    #[test]
    fn invalid_timestamp_format_string_is_a_load_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[timestamp]\nformat = \"%Q\"\n").unwrap();
        assert!(load_from(Some(path)).is_err());
    }

    #[test]
    fn invalid_timestamp_diff_value_is_a_load_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[timestamp]\ndiff = \"verbose\"\n").unwrap();
        assert!(load_from(Some(path)).is_err());
    }

    #[test]
    fn parses_each_diff_style_keyword() {
        for (toml_value, expected) in [
            ("disabled", crate::entry::DiffStyle::Disabled),
            ("short", crate::entry::DiffStyle::Short),
            ("long", crate::entry::DiffStyle::Long),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.toml");
            std::fs::write(&path, format!("[timestamp]\ndiff = \"{toml_value}\"\n")).unwrap();
            let config = load_from(Some(path)).unwrap();
            assert_eq!(config.timestamp.diff, expected);
        }
    }
}
