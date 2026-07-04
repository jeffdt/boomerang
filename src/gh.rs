use crate::model::{Issue, IssueState, Label};
use anyhow::{bail, Result};
use serde::Deserialize;

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
        "number,title,body,labels,state,url".into(),
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
                    .map(|l| Label { name: l.name, color: l.color })
                    .collect(),
                state: parse_state(&r.state)?,
                url: r.url,
            })
        })
        .collect()
}

pub fn parse_labels_json(json: &str) -> Result<Vec<Label>> {
    let raw: Vec<RawLabel> = serde_json::from_str(json)?;
    Ok(raw.into_iter().map(|l| Label { name: l.name, color: l.color }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_issues_json_into_issues() {
        let json = r#"[
            {"number": 42, "title": "Fix login bug", "body": "Steps to repro...",
             "labels": [{"name": "bug", "color": "d73a4a"}], "state": "OPEN",
             "url": "https://github.com/owner/repo/issues/42"}
        ]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 42);
        assert_eq!(issues[0].title, "Fix login bug");
        assert_eq!(issues[0].state, crate::model::IssueState::Open);
        assert_eq!(
            issues[0].labels,
            vec![crate::model::Label { name: "bug".into(), color: "d73a4a".into() }]
        );
    }

    #[test]
    fn parses_closed_state_case_insensitively() {
        let json = r#"[{"number": 1, "title": "t", "body": "", "labels": [], "state": "closed", "url": "u"}]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(issues[0].state, crate::model::IssueState::Closed);
    }

    #[test]
    fn rejects_unrecognized_state() {
        let json = r#"[{"number": 1, "title": "t", "body": "", "labels": [], "state": "weird", "url": "u"}]"#;
        assert!(parse_issues_json(json).is_err());
    }

    #[test]
    fn parses_labels_json() {
        let json = r#"[{"name": "bug", "color": "d73a4a"}, {"name": "docs", "color": "0075ca"}]"#;
        let labels = parse_labels_json(json).unwrap();
        assert_eq!(
            labels,
            vec![
                crate::model::Label { name: "bug".into(), color: "d73a4a".into() },
                crate::model::Label { name: "docs".into(), color: "0075ca".into() },
            ]
        );
    }

    #[test]
    fn list_args_includes_state_filter() {
        let args = list_args(StateFilter::Closed);
        assert!(args.contains(&"--state".to_string()));
        assert!(args.contains(&"closed".to_string()));
    }

    #[test]
    fn state_filter_cycles_open_closed_all() {
        assert_eq!(StateFilter::Open.cycle(), StateFilter::Closed);
        assert_eq!(StateFilter::Closed.cycle(), StateFilter::All);
        assert_eq!(StateFilter::All.cycle(), StateFilter::Open);
    }
}
