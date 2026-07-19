use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Most-recent-first list of `owner/repo` targets offered by the repo picker.
const MAX_RECENT_REPOS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub exit_on_copy_yank: bool,
    pub zebra_striping: bool,
    pub shortcuts_on_demand: bool,
    pub recent_repos: Vec<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            exit_on_copy_yank: false,
            zebra_striping: true,
            shortcuts_on_demand: false,
            recent_repos: Vec::new(),
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

    /// Move `repo` to the front of the recent-repos list, deduplicating any
    /// existing entry and capping the list at `MAX_RECENT_REPOS`.
    pub fn remember_repo(&mut self, repo: &str) {
        self.recent_repos.retain(|existing| existing != repo);
        self.recent_repos.insert(0, repo.to_string());
        self.recent_repos.truncate(MAX_RECENT_REPOS);
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
        assert!(!config.shortcuts_on_demand);
        assert!(config.recent_repos.is_empty());
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
            shortcuts_on_demand: true,
            recent_repos: vec!["jeffdt/boomerang".to_string(), "jeffdt/rolomux".to_string()],
        };
        config.save_to(&path).unwrap();
        let loaded = Config::load_from(&path);
        assert_eq!(loaded, config);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn remember_repo_inserts_at_front() {
        let mut config = Config::default();
        config.remember_repo("jeffdt/boomerang");
        config.remember_repo("jeffdt/rolomux");
        assert_eq!(
            config.recent_repos,
            vec!["jeffdt/rolomux".to_string(), "jeffdt/boomerang".to_string()]
        );
    }

    #[test]
    fn remember_repo_moves_existing_entry_to_front_without_duplicating() {
        let mut config = Config::default();
        config.remember_repo("jeffdt/boomerang");
        config.remember_repo("jeffdt/rolomux");
        config.remember_repo("jeffdt/boomerang");
        assert_eq!(
            config.recent_repos,
            vec!["jeffdt/boomerang".to_string(), "jeffdt/rolomux".to_string()]
        );
    }

    #[test]
    fn remember_repo_caps_the_list_at_max_recent_repos() {
        let mut config = Config::default();
        for i in 0..(MAX_RECENT_REPOS + 3) {
            config.remember_repo(&format!("jeffdt/repo-{i}"));
        }
        assert_eq!(config.recent_repos.len(), MAX_RECENT_REPOS);
        assert_eq!(
            config.recent_repos.first(),
            Some(&format!("jeffdt/repo-{}", MAX_RECENT_REPOS + 2))
        );
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
