use crate::diagnostics;
use crate::model::{Issue, IssueState, Label};
use anyhow::{bail, Result};
use serde::Deserialize;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
    All,
}

impl StateFilter {
    pub fn cycle(self) -> Self {
        match self {
            StateFilter::Open => StateFilter::Closed,
            StateFilter::Closed => StateFilter::All,
            StateFilter::All => StateFilter::Open,
        }
    }

    pub fn as_gh_arg(self) -> &'static str {
        match self {
            StateFilter::Open => "open",
            StateFilter::Closed => "closed",
            StateFilter::All => "all",
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
        "200".into(),
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
    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<()>;
    fn edit(
        &self,
        number: u32,
        title: &str,
        body: &str,
        add_labels: &[String],
        remove_labels: &[String],
    ) -> Result<()>;
    fn close(&self, number: u32) -> Result<()>;
}

#[derive(Clone, Copy)]
pub struct GhCliSource;

impl GhCliSource {
    pub fn new() -> Self {
        GhCliSource
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let started = Instant::now();
        let output = match Command::new("gh").args(args).output() {
            Ok(output) => output,
            Err(e) => {
                diagnostics::log_gh_spawn_error(args, started.elapsed(), &e);
                return Err(e.into());
            }
        };
        diagnostics::log_gh_result(args, started.elapsed(), &output);
        if !output.status.success() {
            bail!(
                "gh {} failed: {}",
                args.join(" "),
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

    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<()> {
        self.run(&create_args(title, body, labels)).map(|_| ())
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
    fn state_filter_cycles_open_closed_all() {
        assert_eq!(StateFilter::Open.cycle(), StateFilter::Closed);
        assert_eq!(StateFilter::Closed.cycle(), StateFilter::All);
        assert_eq!(StateFilter::All.cycle(), StateFilter::Open);
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
}
