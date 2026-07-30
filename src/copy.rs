use crate::model::Issue;
use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

pub const DEFAULT_TEMPLATE_PRIMARY: &str = "#{number}";
pub const DEFAULT_TEMPLATE_SECONDARY: &str = "[#{number}: {title}]({url})";
pub const DEFAULT_TEMPLATE_TERTIARY: &str = "{url}";
pub const DEFAULT_MULTI_DELIMITER: &str = ", ";

const KNOWN_VARIABLES: &[&str] = &["number", "title", "url", "body", "body_short"];

/// Splits a template on its optional `<<...>>` repeat-block marker,
/// returning `(prefix, repeat_unit, suffix)`. When no marker is present,
/// `prefix` and `suffix` are empty and `repeat_unit` is the whole template.
fn split_repeat_block(template: &str) -> std::result::Result<(String, String, String), String> {
    let Some(start) = template.find("<<") else {
        return Ok((String::new(), template.to_string(), String::new()));
    };
    let after_start = start + 2;
    let Some(end_offset) = template[after_start..].find(">>") else {
        return Err("unclosed << repeating block".to_string());
    };
    let end = after_start + end_offset;
    if template[end + 2..].contains("<<") {
        return Err("only one repeating block is allowed".to_string());
    }
    Ok((
        template[..start].to_string(),
        template[after_start..end].to_string(),
        template[end + 2..].to_string(),
    ))
}

/// Finds every `{variable}` or `{variable:arg}` placeholder in `text`.
fn find_placeholders(text: &str) -> Vec<(String, Option<String>)> {
    let mut placeholders = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let Some(end_offset) = rest[start..].find('}') else {
            break;
        };
        let inner = &rest[start + 1..start + end_offset];
        match inner.split_once(':') {
            Some((name, arg)) => placeholders.push((name.to_string(), Some(arg.to_string()))),
            None => placeholders.push((inner.to_string(), None)),
        }
        rest = &rest[start + end_offset + 1..];
    }
    placeholders
}

/// Validates a yank template's syntax: known variable names, a numeric
/// `body_short` argument, at most one repeat block, and no variables
/// outside that block when one is present.
pub fn validate_template(template: &str) -> std::result::Result<(), String> {
    let (prefix, unit, suffix) = split_repeat_block(template)?;
    let has_block = template.contains("<<");
    if has_block {
        let mut outside = find_placeholders(&prefix)
            .into_iter()
            .chain(find_placeholders(&suffix));
        if outside.next().is_some() {
            return Err(
                "variables must be inside << >> when a repeating block is present".to_string(),
            );
        }
    }
    for (name, arg) in find_placeholders(&unit) {
        if !KNOWN_VARIABLES.contains(&name.as_str()) {
            return Err(format!("unknown variable: {{{name}}}"));
        }
        if name == "body_short" {
            match arg {
                Some(value) if value.parse::<usize>().is_ok() => {}
                Some(value) => return Err(format!("invalid body_short length: {{body_short:{value}}}")),
                None => {
                    return Err("body_short requires a length, e.g. {body_short:60}".to_string())
                }
            }
        }
    }
    Ok(())
}

fn render_variable(name: &str, arg: Option<&str>, issue: &Issue) -> String {
    match name {
        "number" => issue.number.to_string(),
        "title" => issue.title.clone(),
        "url" => issue.url.clone(),
        "body" => issue.body.clone(),
        "body_short" => {
            let len: usize = arg.and_then(|a| a.parse().ok()).unwrap_or(usize::MAX);
            let char_count = issue.body.chars().count();
            if char_count <= len {
                issue.body.clone()
            } else {
                let truncated: String = issue.body.chars().take(len).collect();
                format!("{truncated}...")
            }
        }
        _ => String::new(),
    }
}

fn render_unit(unit: &str, issue: &Issue) -> String {
    let mut output = String::new();
    let mut rest = unit;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let Some(end_offset) = rest[start..].find('}') else {
            output.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let inner = &rest[start + 1..start + end_offset];
        let (name, arg) = match inner.split_once(':') {
            Some((name, arg)) => (name, Some(arg)),
            None => (inner, None),
        };
        output.push_str(&render_variable(name, arg, issue));
        rest = &rest[start + end_offset + 1..];
    }
    output.push_str(rest);
    output
}

/// Renders `template` against `issues`. A `<<...>>` repeat block (if
/// present) is rendered once per issue and joined with `delimiter`; the
/// surrounding literal text is emitted once regardless of issue count. A
/// template with no repeat block is itself treated as the repeating unit,
/// matching the pre-templating join behavior.
pub fn render_template(template: &str, issues: &[&Issue], delimiter: &str) -> String {
    let (prefix, unit, suffix) = split_repeat_block(template)
        .unwrap_or_else(|_| (String::new(), template.to_string(), String::new()));
    let rendered: Vec<String> = issues.iter().map(|issue| render_unit(&unit, issue)).collect();
    format!("{prefix}{}{suffix}", rendered.join(delimiter))
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

    fn issue_with_body(body: &str) -> Issue {
        let mut issue = sample_issue();
        issue.body = body.to_string();
        issue
    }

    #[test]
    fn renders_number_variable() {
        let issue = sample_issue();
        assert_eq!(render_template("{number}", &[&issue], ", "), "123");
    }

    #[test]
    fn renders_title_variable() {
        let issue = sample_issue();
        assert_eq!(render_template("{title}", &[&issue], ", "), "Fix login bug");
    }

    #[test]
    fn renders_url_variable() {
        let issue = sample_issue();
        assert_eq!(
            render_template("{url}", &[&issue], ", "),
            "https://github.com/owner/repo/issues/123"
        );
    }

    #[test]
    fn renders_body_variable() {
        let issue = issue_with_body("Full body text.");
        assert_eq!(render_template("{body}", &[&issue], ", "), "Full body text.");
    }

    #[test]
    fn renders_literal_text_around_a_variable() {
        let issue = sample_issue();
        assert_eq!(render_template("#{number}", &[&issue], ", "), "#123");
    }

    #[test]
    fn renders_default_markdown_link_template() {
        let issue = sample_issue();
        assert_eq!(
            render_template(DEFAULT_TEMPLATE_SECONDARY, &[&issue], ", "),
            "[#123: Fix login bug](https://github.com/owner/repo/issues/123)"
        );
    }

    #[test]
    fn body_short_truncates_and_appends_ellipsis_when_over_length() {
        let issue = issue_with_body("0123456789");
        assert_eq!(
            render_template("{body_short:5}", &[&issue], ", "),
            "01234..."
        );
    }

    #[test]
    fn body_short_omits_ellipsis_when_body_fits_exactly() {
        let issue = issue_with_body("01234");
        assert_eq!(render_template("{body_short:5}", &[&issue], ", "), "01234");
    }

    #[test]
    fn body_short_omits_ellipsis_when_body_is_shorter_than_length() {
        let issue = issue_with_body("hi");
        assert_eq!(render_template("{body_short:5}", &[&issue], ", "), "hi");
    }

    #[test]
    fn body_short_truncates_on_character_boundaries_for_multi_byte_utf8() {
        let issue = issue_with_body("héllo wörld 🎉🎉🎉");
        assert_eq!(
            render_template("{body_short:7}", &[&issue], ", "),
            "héllo w..."
        );
    }

    #[test]
    fn renders_repeat_block_once_per_issue_and_joins_with_delimiter() {
        let one = issue(1, "Create repo");
        let two = issue(2, "Create readme.md");
        let three = issue(3, "Set up CI");
        let template = "claude \"Let's implement <<#{number} - {title}>>\"";
        assert_eq!(
            render_template(template, &[&one, &two, &three], ", "),
            "claude \"Let's implement #1 - Create repo, #2 - Create readme.md, #3 - Set up CI\""
        );
    }

    #[test]
    fn renders_repeat_block_with_a_single_issue_and_no_delimiter() {
        let one = issue(1, "Create repo");
        let template = "claude \"Let's implement <<#{number} - {title}>>\"";
        assert_eq!(
            render_template(template, &[&one], ", "),
            "claude \"Let's implement #1 - Create repo\""
        );
    }

    #[test]
    fn no_repeat_block_marker_repeats_whole_template_matching_todays_behavior() {
        let one = issue(1, "one");
        let two = issue(2, "two");
        assert_eq!(
            render_template("#{number}", &[&one, &two], ", "),
            "#1, #2"
        );
    }

    fn issue(number: u32, title: &str) -> Issue {
        let mut issue = sample_issue();
        issue.number = number;
        issue.title = title.to_string();
        issue
    }

    #[test]
    fn validate_template_accepts_all_known_variables() {
        assert!(validate_template("{number} {title} {url} {body} {body_short:10}").is_ok());
    }

    #[test]
    fn validate_template_accepts_default_templates() {
        assert!(validate_template(DEFAULT_TEMPLATE_PRIMARY).is_ok());
        assert!(validate_template(DEFAULT_TEMPLATE_SECONDARY).is_ok());
        assert!(validate_template(DEFAULT_TEMPLATE_TERTIARY).is_ok());
    }

    #[test]
    fn validate_template_rejects_unknown_variable() {
        let err = validate_template("{foo}").unwrap_err();
        assert_eq!(err, "unknown variable: {foo}");
    }

    #[test]
    fn validate_template_rejects_body_short_without_length() {
        assert!(validate_template("{body_short}").is_err());
    }

    #[test]
    fn validate_template_rejects_body_short_with_non_numeric_length() {
        assert!(validate_template("{body_short:abc}").is_err());
    }

    #[test]
    fn validate_template_rejects_variable_outside_repeat_block() {
        let err = validate_template("{title} <<#{number}>>").unwrap_err();
        assert_eq!(
            err,
            "variables must be inside << >> when a repeating block is present"
        );
    }

    #[test]
    fn validate_template_rejects_more_than_one_repeat_block() {
        assert!(validate_template("<<#{number}>> and <<{title}>>").is_err());
    }

    #[test]
    fn validate_template_rejects_unclosed_repeat_block() {
        assert!(validate_template("<<#{number}").is_err());
    }

    #[test]
    fn formats_plain_url_via_default_tertiary_template() {
        let issue = sample_issue();
        assert_eq!(
            render_template(DEFAULT_TEMPLATE_TERTIARY, &[&issue], ", "),
            "https://github.com/owner/repo/issues/123"
        );
    }
}
