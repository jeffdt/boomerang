use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const LOG_ENV: &str = "BOOMERANG_LOG";
pub const LOG_PATH_ENV: &str = "BOOMERANG_LOG_PATH";

pub fn run_doctor() -> anyhow::Result<()> {
    println!("boomerang {}", env!("CARGO_PKG_VERSION"));
    match env::current_dir() {
        Ok(cwd) => println!("cwd: {}", cwd.display()),
        Err(e) => println!("cwd: error: {e}"),
    }
    println!("GITHUB_TOKEN: {}", env_status("GITHUB_TOKEN"));
    println!("GH_TOKEN: {}", env_status("GH_TOKEN"));
    if env::var_os("GITHUB_TOKEN").is_some() {
        println!("note: GITHUB_TOKEN is set and overrides gh keyring auth; use env -u GITHUB_TOKEN boomerang when you want the keyring account.");
    }
    println!("{}: {}", LOG_ENV, env_status(LOG_ENV));
    if let Some(path) = active_log_path() {
        println!("diagnostic log: {}", path.display());
    } else {
        println!("diagnostic log: disabled; set {LOG_ENV}=1 to enable");
    }

    print_command("gh --version", "gh", &["--version"]);
    print_command("gh auth status", "gh", &["auth", "status"]);
    print_command(
        "gh repo view",
        "gh",
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner,viewerPermission",
            "--jq",
            r#".nameWithOwner + " permission=" + .viewerPermission"#,
        ],
    );
    print_command("git root", "git", &["rev-parse", "--show-toplevel"]);
    print_command("git remote", "git", &["remote", "-v"]);

    Ok(())
}

pub fn log_event(label: &str) {
    append_log(&format!("ts_ms={} event={label}", unix_timestamp_ms()));
}

pub fn log_gh_result(args: &[String], elapsed: Duration, output: &Output) {
    let status = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| output.status.to_string());
    append_log(&format!(
        "ts={} gh args={} elapsed_ms={} status={} success={} stdout_bytes={} stderr={}",
        unix_timestamp(),
        json_args(args),
        elapsed.as_millis(),
        status,
        output.status.success(),
        output.stdout.len(),
        compact_bytes(&output.stderr, 2_000)
    ));
}

pub fn log_gh_spawn_error(args: &[String], elapsed: Duration, error: &std::io::Error) {
    append_log(&format!(
        "ts={} gh args={} elapsed_ms={} spawn_error={}",
        unix_timestamp(),
        json_args(args),
        elapsed.as_millis(),
        compact_text(&error.to_string(), 1_000)
    ));
}

pub fn sanitize_args_for_log(args: &[String]) -> Vec<String> {
    let mut sanitized = Vec::with_capacity(args.len());
    let mut redact_next = false;
    for arg in args {
        if redact_next {
            sanitized.push("[redacted]".to_string());
            redact_next = false;
            continue;
        }
        sanitized.push(redact_token_like(arg));
        if matches!(arg.as_str(), "--title" | "--body") {
            redact_next = true;
        }
    }
    sanitized
}

pub fn log_enabled_value(value: Option<&str>) -> bool {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("1" | "true" | "yes" | "on" | "debug" | "trace") => true,
        Some("" | "0" | "false" | "no" | "off") | None => false,
        Some(_) => true,
    }
}

fn print_command(label: &str, program: &str, args: &[&str]) {
    println!();
    println!("[{label}]");
    match Command::new(program).args(args).output() {
        Ok(output) => {
            println!("status: {}", output.status);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                println!("stdout:\n{}", trim_trailing(&stdout));
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                println!("stderr:\n{}", trim_trailing(&stderr));
            }
        }
        Err(e) => println!("error: {e}"),
    }
}

fn env_status(name: &str) -> &'static str {
    if env::var_os(name).is_some() {
        "set"
    } else {
        "unset"
    }
}

fn active_log_path() -> Option<PathBuf> {
    if !log_enabled_value(env::var(LOG_ENV).ok().as_deref()) {
        return None;
    }
    if let Some(path) = env::var_os(LOG_PATH_ENV).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    Some(default_log_path())
}

fn default_log_path() -> PathBuf {
    if let Some(cache_home) = env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(cache_home).join("boomerang/boomerang.log");
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(home).join(".cache/boomerang/boomerang.log");
    }
    env::temp_dir().join("boomerang.log")
}

fn append_log(line: &str) {
    let Some(path) = active_log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

fn json_args(args: &[String]) -> String {
    serde_json::to_string(&sanitize_args_for_log(args)).unwrap_or_else(|_| "[]".to_string())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn compact_bytes(bytes: &[u8], max_chars: usize) -> String {
    compact_text(&String::from_utf8_lossy(bytes), max_chars)
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let escaped = text.replace('\n', "\\n").replace('\r', "\\r");
    let mut out: String = escaped.chars().take(max_chars).collect();
    if escaped.chars().count() > max_chars {
        out.push_str("...[truncated]");
    }
    redact_token_like(&out)
}

fn trim_trailing(text: &str) -> &str {
    text.trim_end_matches(['\r', '\n'])
}

fn redact_token_like(text: &str) -> String {
    let mut redacted = text.to_string();
    for marker in ["ghp_", "gho_", "github_pat_"] {
        if redacted.contains(marker) {
            redacted = redacted
                .split_whitespace()
                .map(|part| {
                    if part.contains(marker) {
                        "[redacted-token]"
                    } else {
                        part
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn log_enabled_value_recognizes_common_enabled_values() {
        assert!(log_enabled_value(Some("1")));
        assert!(log_enabled_value(Some("debug")));
        assert!(log_enabled_value(Some("true")));
    }

    #[test]
    fn log_enabled_value_recognizes_common_disabled_values() {
        assert!(!log_enabled_value(None));
        assert!(!log_enabled_value(Some("0")));
        assert!(!log_enabled_value(Some("false")));
    }

    #[test]
    fn sanitize_args_redacts_issue_title_and_body_values() {
        let args = strings(&[
            "issue",
            "edit",
            "42",
            "--title",
            "Private title",
            "--body",
            "Private body",
            "--add-label",
            "bug",
        ]);
        assert_eq!(
            sanitize_args_for_log(&args),
            strings(&[
                "issue",
                "edit",
                "42",
                "--title",
                "[redacted]",
                "--body",
                "[redacted]",
                "--add-label",
                "bug",
            ])
        );
    }

    #[test]
    fn sanitize_args_redacts_token_like_arguments() {
        let args = strings(&["auth", "token", "ghp_secretvalue"]);
        assert_eq!(
            sanitize_args_for_log(&args),
            strings(&["auth", "token", "[redacted-token]"])
        );
    }
}
