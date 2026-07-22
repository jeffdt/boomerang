use crate::diagnostics;
use crate::model::{Issue, IssueState, Label};
use anyhow::{bail, Result};
use serde::Deserialize;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Triage,
    Closed,
    All,
}

impl StateFilter {
    pub fn cycle(self) -> Self {
        match self {
            StateFilter::Open => StateFilter::Triage,
            StateFilter::Triage => StateFilter::Closed,
            StateFilter::Closed => StateFilter::All,
            StateFilter::All => StateFilter::Open,
        }
    }

    pub fn as_gh_arg(self) -> &'static str {
        match self {
            StateFilter::Open => "open",
            StateFilter::Closed => "closed",
            StateFilter::All => "all",
            StateFilter::Triage => "open",
        }
    }
}

pub fn list_args(state: StateFilter) -> Vec<String> {
    vec![
        "issue".into(),
        "list".into(),
        "--state".into(),
        state.as_gh_arg().into(),
        "--json".into(),
        "number,title,body,labels,state,url,createdAt".into(),
        "--limit".into(),
        "500".into(),
    ]
}

pub fn labels_args() -> Vec<String> {
    vec![
        "label".into(),
        "list".into(),
        "--json".into(),
        "name,color".into(),
        "--limit".into(),
        "200".into(),
    ]
}

pub fn repo_name_args() -> Vec<String> {
    vec![
        "repo".into(),
        "view".into(),
        "--json".into(),
        "nameWithOwner".into(),
        "--jq".into(),
        ".nameWithOwner".into(),
    ]
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
    color: String,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u32,
    title: String,
    body: String,
    labels: Vec<RawLabel>,
    state: String,
    url: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

fn parse_state(raw: &str) -> Result<IssueState> {
    match raw.to_ascii_lowercase().as_str() {
        "open" => Ok(IssueState::Open),
        "closed" => Ok(IssueState::Closed),
        other => bail!("unrecognized issue state from gh CLI: {other}"),
    }
}

pub fn parse_issues_json(json: &str) -> Result<Vec<Issue>> {
    let raw: Vec<RawIssue> = serde_json::from_str(json)?;
    raw.into_iter()
        .map(|r| {
            Ok(Issue {
                number: r.number,
                title: r.title,
                body: r.body,
                labels: r
                    .labels
                    .into_iter()
                    .map(|l| Label {
                        name: l.name,
                        color: l.color,
                    })
                    .collect(),
                state: parse_state(&r.state)?,
                url: r.url,
                created_at: r.created_at,
            })
        })
        .collect()
}

pub fn parse_labels_json(json: &str) -> Result<Vec<Label>> {
    let raw: Vec<RawLabel> = serde_json::from_str(json)?;
    Ok(raw
        .into_iter()
        .map(|l| Label {
            name: l.name,
            color: l.color,
        })
        .collect())
}

pub fn parse_repo_name(raw: &str) -> String {
    raw.trim().to_string()
}

/// Parses the newly created issue's number out of `gh issue create`'s
/// stdout, which is just the issue's URL (e.g.
/// `https://github.com/owner/repo/issues/71`).
pub fn parse_created_issue_number(output: &str) -> Result<u32> {
    let url = output.trim();
    match url
        .rsplit('/')
        .next()
        .and_then(|segment| segment.parse().ok())
    {
        Some(number) => Ok(number),
        None => bail!("couldn't parse an issue number from gh's create output: {url:?}"),
    }
}

pub fn create_args(title: &str, body: &str, labels: &[String]) -> Vec<String> {
    let mut args = vec![
        "issue".into(),
        "create".into(),
        "--title".into(),
        title.into(),
        "--body".into(),
        body.into(),
    ];
    for label in labels {
        args.push("--label".into());
        args.push(label.clone());
    }
    args
}

pub fn edit_args(
    number: u32,
    title: &str,
    body: &str,
    add_labels: &[String],
    remove_labels: &[String],
) -> Vec<String> {
    let mut args = vec![
        "issue".into(),
        "edit".into(),
        number.to_string(),
        "--title".into(),
        title.into(),
        "--body".into(),
        body.into(),
    ];
    for label in add_labels {
        args.push("--add-label".into());
        args.push(label.clone());
    }
    for label in remove_labels {
        args.push("--remove-label".into());
        args.push(label.clone());
    }
    args
}

pub fn close_args(number: u32) -> Vec<String> {
    vec!["issue".into(), "close".into(), number.to_string()]
}

pub trait IssueSource: Clone + Send + 'static {
    fn list(&self, state: StateFilter) -> Result<Vec<Issue>>;
    fn labels(&self) -> Result<Vec<Label>>;
    fn repo_name(&self) -> Result<String>;
    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32>;
    fn edit(
        &self,
        number: u32,
        title: &str,
        body: &str,
        add_labels: &[String],
        remove_labels: &[String],
    ) -> Result<()>;
    fn close(&self, number: u32) -> Result<()>;
    /// Retarget every subsequent `gh` call at `repo` (`owner/repo`), or back to
    /// the current directory's repo when `None`. Shared across every clone of
    /// this source, so a switch made from one thread (the UI thread handling
    /// a repo-picker submission) is immediately visible to background worker
    /// threads spawned afterward.
    fn set_repo(&self, repo: Option<String>);
}

#[derive(Clone)]
pub struct GhCliSource {
    repo: Arc<Mutex<Option<String>>>,
}

impl GhCliSource {
    pub fn new() -> Self {
        GhCliSource {
            repo: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_repo(repo: String) -> Self {
        GhCliSource {
            repo: Arc::new(Mutex::new(Some(repo))),
        }
    }

    fn current_repo(&self) -> Option<String> {
        self.repo.lock().unwrap().clone()
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let mut full_args = Vec::with_capacity(args.len() + 2);
        if let Some(repo) = self.current_repo() {
            full_args.push("-R".to_string());
            full_args.push(repo);
        }
        full_args.extend_from_slice(args);

        let started = Instant::now();
        let output = match Command::new("gh").args(&full_args).output() {
            Ok(output) => output,
            Err(e) => {
                diagnostics::log_gh_spawn_error(&full_args, started.elapsed(), &e);
                if e.kind() == std::io::ErrorKind::NotFound {
                    bail!("`gh` CLI not found on PATH. Install it from https://cli.github.com and run `gh auth login`.");
                }
                return Err(e.into());
            }
        };
        diagnostics::log_gh_result(&full_args, started.elapsed(), &output);
        if !output.status.success() {
            bail!(
                "gh {} failed: {}",
                full_args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl IssueSource for GhCliSource {
    fn list(&self, state: StateFilter) -> Result<Vec<Issue>> {
        parse_issues_json(&self.run(&list_args(state))?)
    }

    fn labels(&self) -> Result<Vec<Label>> {
        parse_labels_json(&self.run(&labels_args())?)
    }

    fn repo_name(&self) -> Result<String> {
        Ok(parse_repo_name(&self.run(&repo_name_args())?))
    }

    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32> {
        let output = self.run(&create_args(title, body, labels))?;
        parse_created_issue_number(&output)
    }

    fn edit(
        &self,
        number: u32,
        title: &str,
        body: &str,
        add_labels: &[String],
        remove_labels: &[String],
    ) -> Result<()> {
        self.run(&edit_args(number, title, body, add_labels, remove_labels))
            .map(|_| ())
    }

    fn close(&self, number: u32) -> Result<()> {
        self.run(&close_args(number)).map(|_| ())
    }

    fn set_repo(&self, repo: Option<String>) {
        *self.repo.lock().unwrap() = repo;
    }
}

/// Parse user-typed text (from the repo picker or a CLI arg) into a
/// `gh -R`-compatible `owner/repo` spec. Accepts a bare `owner/repo`, a
/// `https://github.com/owner/repo[...]` URL (extra path segments like
/// `/issues/20` are dropped), or a `git@github.com:owner/repo.git` SSH URL.
/// Returns `None` for anything else, including non-github.com hosts.
pub fn parse_repo_spec(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        return owner_repo_from_path(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return owner_repo_from_path(rest);
    }
    owner_repo_from_path(trimmed)
}

fn owner_repo_from_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    if !owner.chars().all(is_repo_char) || !repo.chars().all(is_repo_char) {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn is_repo_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_issues_json_into_issues() {
        let json = r#"[
            {"number": 42, "title": "Fix login bug", "body": "Steps to repro...",
             "labels": [{"name": "bug", "color": "d73a4a"}], "state": "OPEN",
             "url": "https://github.com/owner/repo/issues/42",
             "createdAt": "2026-06-01T12:00:00Z"}
        ]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 42);
        assert_eq!(issues[0].title, "Fix login bug");
        assert_eq!(issues[0].state, crate::model::IssueState::Open);
        assert_eq!(issues[0].created_at, "2026-06-01T12:00:00Z");
        assert_eq!(
            issues[0].labels,
            vec![crate::model::Label {
                name: "bug".into(),
                color: "d73a4a".into()
            }]
        );
    }

    #[test]
    fn parses_closed_state_case_insensitively() {
        let json = r#"[{"number": 1, "title": "t", "body": "", "labels": [], "state": "closed", "url": "u", "createdAt": "2026-01-01T00:00:00Z"}]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(issues[0].state, crate::model::IssueState::Closed);
    }

    #[test]
    fn rejects_unrecognized_state() {
        let json = r#"[{"number": 1, "title": "t", "body": "", "labels": [], "state": "weird", "url": "u", "createdAt": "2026-01-01T00:00:00Z"}]"#;
        assert!(parse_issues_json(json).is_err());
    }

    #[test]
    fn parses_labels_json() {
        let json = r#"[{"name": "bug", "color": "d73a4a"}, {"name": "docs", "color": "0075ca"}]"#;
        let labels = parse_labels_json(json).unwrap();
        assert_eq!(
            labels,
            vec![
                crate::model::Label {
                    name: "bug".into(),
                    color: "d73a4a".into()
                },
                crate::model::Label {
                    name: "docs".into(),
                    color: "0075ca".into()
                },
            ]
        );
    }

    #[test]
    fn list_args_includes_state_filter() {
        let args = list_args(StateFilter::Closed);
        assert!(args.contains(&"--state".to_string()));
        assert!(args.contains(&"closed".to_string()));
        assert!(
            args.iter().any(|a| a.contains("createdAt")),
            "must request createdAt for the description pane"
        );
    }

    #[test]
    fn list_args_requests_a_500_item_limit() {
        let args = list_args(StateFilter::Open);
        let limit_index = args
            .iter()
            .position(|a| a == "--limit")
            .expect("list_args must pass --limit");
        assert_eq!(args[limit_index + 1], "500");
    }

    #[test]
    fn state_filter_cycles_open_triage_closed_all() {
        assert_eq!(StateFilter::Open.cycle(), StateFilter::Triage);
        assert_eq!(StateFilter::Triage.cycle(), StateFilter::Closed);
        assert_eq!(StateFilter::Closed.cycle(), StateFilter::All);
        assert_eq!(StateFilter::All.cycle(), StateFilter::Open);
    }

    #[test]
    fn list_args_treats_triage_as_open() {
        let args = list_args(StateFilter::Triage);
        assert!(args.contains(&"--state".to_string()));
        assert!(args.contains(&"open".to_string()));
    }

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn create_args_includes_title_body_and_labels() {
        let args = create_args(
            "Fix bug",
            "Steps...",
            &["bug".to_string(), "urgent".to_string()],
        );
        assert_eq!(
            args,
            strs(&[
                "issue", "create", "--title", "Fix bug", "--body", "Steps...", "--label", "bug",
                "--label", "urgent"
            ])
        );
    }

    #[test]
    fn parse_created_issue_number_reads_the_trailing_url_segment() {
        assert_eq!(
            parse_created_issue_number("https://github.com/owner/repo/issues/71\n").unwrap(),
            71
        );
    }

    #[test]
    fn parse_created_issue_number_errors_on_unparseable_output() {
        assert!(parse_created_issue_number("not a url").is_err());
    }

    #[test]
    fn edit_args_includes_add_and_remove_labels() {
        let args = edit_args(
            42,
            "New title",
            "New body",
            &["bug".to_string()],
            &["wontfix".to_string()],
        );
        assert_eq!(
            args,
            strs(&[
                "issue",
                "edit",
                "42",
                "--title",
                "New title",
                "--body",
                "New body",
                "--add-label",
                "bug",
                "--remove-label",
                "wontfix"
            ])
        );
    }

    #[test]
    fn close_args_targets_issue_number() {
        assert_eq!(close_args(7), strs(&["issue", "close", "7"]));
    }

    #[test]
    fn repo_name_args_requests_name_with_owner() {
        let args = repo_name_args();
        assert_eq!(
            args,
            strs(&[
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "--jq",
                ".nameWithOwner"
            ])
        );
    }

    #[test]
    fn parse_repo_name_trims_trailing_newline() {
        assert_eq!(parse_repo_name("jeffdt/boomerang\n"), "jeffdt/boomerang");
    }

    #[test]
    fn parse_repo_spec_accepts_bare_owner_slash_repo() {
        assert_eq!(
            parse_repo_spec("jeffdt/rolomux"),
            Some("jeffdt/rolomux".to_string())
        );
    }

    #[test]
    fn parse_repo_spec_trims_surrounding_whitespace() {
        assert_eq!(
            parse_repo_spec("  jeffdt/rolomux  "),
            Some("jeffdt/rolomux".to_string())
        );
    }

    #[test]
    fn parse_repo_spec_accepts_https_github_url() {
        assert_eq!(
            parse_repo_spec("https://github.com/jeffdt/rolomux"),
            Some("jeffdt/rolomux".to_string())
        );
    }

    #[test]
    fn parse_repo_spec_accepts_https_github_url_with_trailing_slash() {
        assert_eq!(
            parse_repo_spec("https://github.com/jeffdt/rolomux/"),
            Some("jeffdt/rolomux".to_string())
        );
    }

    #[test]
    fn parse_repo_spec_drops_extra_path_segments_from_a_url() {
        assert_eq!(
            parse_repo_spec("https://github.com/jeffdt/rolomux/issues/20"),
            Some("jeffdt/rolomux".to_string())
        );
    }

    #[test]
    fn parse_repo_spec_accepts_ssh_style_url() {
        assert_eq!(
            parse_repo_spec("git@github.com:jeffdt/rolomux.git"),
            Some("jeffdt/rolomux".to_string())
        );
    }

    #[test]
    fn parse_repo_spec_rejects_owner_without_repo() {
        assert_eq!(parse_repo_spec("jeffdt"), None);
    }

    #[test]
    fn parse_repo_spec_rejects_blank_input() {
        assert_eq!(parse_repo_spec("   "), None);
    }

    #[test]
    fn parse_repo_spec_rejects_non_github_host() {
        assert_eq!(parse_repo_spec("https://gitlab.com/jeffdt/rolomux"), None);
    }

    #[test]
    fn set_repo_prefixes_subsequent_calls_with_dash_r() {
        let source = GhCliSource::new();
        assert_eq!(source.current_repo(), None);
        source.set_repo(Some("jeffdt/rolomux".to_string()));
        assert_eq!(source.current_repo(), Some("jeffdt/rolomux".to_string()));
    }

    #[test]
    fn with_repo_starts_pre_targeted() {
        let source = GhCliSource::with_repo("jeffdt/rolomux".to_string());
        assert_eq!(source.current_repo(), Some("jeffdt/rolomux".to_string()));
    }

    #[test]
    fn set_repo_is_visible_across_clones() {
        // Every clone shares the same underlying Arc<Mutex<_>>, so a switch
        // made through one handle (the UI thread) must be visible to another
        // handle already captured by a spawned worker thread.
        let source = GhCliSource::new();
        let clone = source.clone();
        clone.set_repo(Some("jeffdt/rolomux".to_string()));
        assert_eq!(source.current_repo(), Some("jeffdt/rolomux".to_string()));
    }
}
