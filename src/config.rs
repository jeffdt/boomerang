use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub exit_on_copy_yank: bool,
    pub zebra_striping: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            exit_on_copy_yank: false,
            zebra_striping: true,
        }
    }
}

impl Config {
    pub fn load_from(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string(self).map_err(io::Error::other)?;
        std::fs::write(path, body)
    }
}

/// `$XDG_CONFIG_HOME/boomerang/config.toml`, falling back to
/// `~/.config/boomerang/config.toml` when unset or empty.
pub fn config_path() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("boomerang").join("config.toml");
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".config")
        .join("boomerang")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "boomerang-config-test-{name}-{}.toml",
            std::process::id()
        ))
    }

    #[test]
    fn default_config_has_exit_on_copy_yank_off_and_zebra_striping_on() {
        let config = Config::default();
        assert!(!config.exit_on_copy_yank);
        assert!(config.zebra_striping);
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let path = temp_path("missing");
        let _ = fs::remove_file(&path);
        let config = Config::load_from(&path);
        assert_eq!(config, Config::default());
    }

    #[test]
    fn load_from_corrupt_file_returns_defaults() {
        let path = temp_path("corrupt");
        fs::write(&path, "not valid toml {{{").unwrap();
        let config = Config::load_from(&path);
        assert_eq!(config, Config::default());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_then_load_round_trips_values() {
        let path = temp_path("roundtrip");
        let config = Config {
            exit_on_copy_yank: true,
            zebra_striping: false,
        };
        config.save_to(&path).unwrap();
        let loaded = Config::load_from(&path);
        assert_eq!(loaded, config);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let dir =
            std::env::temp_dir().join(format!("boomerang-config-test-dir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("config.toml");
        Config::default().save_to(&path).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
