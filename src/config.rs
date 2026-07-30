use crate::copy;
use crate::model::{DEFAULT_ACCENT_COLOR, DEFAULT_REPO_ACCENT_COLOR};
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
    pub accent_color: String,
    pub repo_accent_color: String,
    pub yank_template_primary: String,
    pub yank_template_secondary: String,
    pub yank_template_tertiary: String,
    pub yank_multi_delimiter: String,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            exit_on_copy_yank: false,
            zebra_striping: true,
            shortcuts_on_demand: false,
            recent_repos: Vec::new(),
            accent_color: DEFAULT_ACCENT_COLOR.to_string(),
            repo_accent_color: DEFAULT_REPO_ACCENT_COLOR.to_string(),
            yank_template_primary: copy::DEFAULT_TEMPLATE_PRIMARY.to_string(),
            yank_template_secondary: copy::DEFAULT_TEMPLATE_SECONDARY.to_string(),
            yank_template_tertiary: copy::DEFAULT_TEMPLATE_TERTIARY.to_string(),
            yank_multi_delimiter: copy::DEFAULT_MULTI_DELIMITER.to_string(),
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

    /// Validates the three yank templates, reverting any invalid one to
    /// its slot's compiled-in default. Returns one warning message per
    /// reverted template, for surfacing to the user at startup.
    pub fn repair_yank_templates(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Err(e) = copy::validate_template(&self.yank_template_primary) {
            warnings.push(format!("invalid yank_template_primary, reverted to default: {e}"));
            self.yank_template_primary = copy::DEFAULT_TEMPLATE_PRIMARY.to_string();
        }
        if let Err(e) = copy::validate_template(&self.yank_template_secondary) {
            warnings.push(format!("invalid yank_template_secondary, reverted to default: {e}"));
            self.yank_template_secondary = copy::DEFAULT_TEMPLATE_SECONDARY.to_string();
        }
        if let Err(e) = copy::validate_template(&self.yank_template_tertiary) {
            warnings.push(format!("invalid yank_template_tertiary, reverted to default: {e}"));
            self.yank_template_tertiary = copy::DEFAULT_TEMPLATE_TERTIARY.to_string();
        }
        warnings
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
        assert_eq!(config.accent_color, "Blue");
        assert_eq!(config.repo_accent_color, "Green");
        assert_eq!(config.yank_template_primary, "#{number}");
        assert_eq!(config.yank_template_secondary, "[#{number}: {title}]({url})");
        assert_eq!(config.yank_template_tertiary, "{url}");
        assert_eq!(config.yank_multi_delimiter, ", ");
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
            accent_color: "Magenta".to_string(),
            repo_accent_color: "Cyan".to_string(),
            yank_template_primary: "#{number}: {title}".to_string(),
            yank_template_secondary: "{url}".to_string(),
            yank_template_tertiary: "{body_short:40}".to_string(),
            yank_multi_delimiter: " | ".to_string(),
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

    #[test]
    fn repair_yank_templates_reverts_invalid_template_and_returns_a_warning() {
        let mut config = Config {
            yank_template_secondary: "{not_a_real_variable}".to_string(),
            ..Config::default()
        };
        let warnings = config.repair_yank_templates();
        assert_eq!(config.yank_template_secondary, Config::default().yank_template_secondary);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("yank_template_secondary"));
        assert!(warnings[0].contains("unknown variable: {not_a_real_variable}"));
    }

    #[test]
    fn load_from_old_schema_file_without_yank_fields_fills_in_defaults() {
        let path = temp_path("old-schema");
        fs::write(&path, "exit_on_copy_yank = true\nzebra_striping = false\n").unwrap();
        let config = Config::load_from(&path);
        assert!(config.exit_on_copy_yank);
        assert!(!config.zebra_striping);
        assert_eq!(config.yank_template_primary, copy::DEFAULT_TEMPLATE_PRIMARY);
        assert_eq!(config.yank_template_secondary, copy::DEFAULT_TEMPLATE_SECONDARY);
        assert_eq!(config.yank_template_tertiary, copy::DEFAULT_TEMPLATE_TERTIARY);
        assert_eq!(config.yank_multi_delimiter, copy::DEFAULT_MULTI_DELIMITER);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn repair_yank_templates_leaves_valid_templates_untouched() {
        let mut config = Config {
            yank_template_primary: "{number}: {title}".to_string(),
            ..Config::default()
        };
        let warnings = config.repair_yank_templates();
        assert_eq!(config.yank_template_primary, "{number}: {title}");
        assert!(warnings.is_empty());
    }
}
