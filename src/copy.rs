use crate::model::Issue;
use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn format_reference(issue: &Issue) -> String {
    format!("#{}", issue.number)
}

pub fn format_markdown_link(issue: &Issue) -> String {
    format!("[#{}: {}]({})", issue.number, issue.title, issue.url)
}

pub fn format_url(issue: &Issue) -> String {
    issue.url.clone()
}

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Issue, IssueState};

    fn sample_issue() -> Issue {
        Issue {
            number: 123,
            title: "Fix login bug".into(),
            body: String::new(),
            labels: vec![],
            state: IssueState::Open,
            url: "https://github.com/owner/repo/issues/123".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn formats_issue_reference() {
        assert_eq!(format_reference(&sample_issue()), "#123");
    }

    #[test]
    fn formats_markdown_link() {
        assert_eq!(
            format_markdown_link(&sample_issue()),
            "[#123: Fix login bug](https://github.com/owner/repo/issues/123)"
        );
    }

    #[test]
    fn formats_plain_url() {
        assert_eq!(
            format_url(&sample_issue()),
            "https://github.com/owner/repo/issues/123"
        );
    }
}
