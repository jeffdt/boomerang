#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub number: u32,
    pub title: String,
    pub body: String,
    pub labels: Vec<Label>,
    pub state: IssueState,
    pub url: String,
    pub created_at: String,
}

use crate::gh::StateFilter;
use crate::search;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub const STATUS_TOAST_DURATION: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    List,
    Search,
    LittleCreate(String),
    Form(FormState),
    ConfirmClose(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormField {
    #[default]
    Title,
    Body,
    Labels,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FormState {
    pub editing: Option<u32>,
    pub title: String,
    pub body: String,
    pub all_label_names: Vec<String>,
    pub selected_labels: HashSet<String>,
    pub label_cursor: usize,
    pub field: FormField,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSubmission {
    pub editing: Option<u32>,
    pub title: String,
    pub body: String,
    pub add_labels: Vec<String>,
    pub remove_labels: Vec<String>,
}

pub struct AppState {
    pub issues: Vec<Issue>,
    pub all_labels: Vec<Label>,
    pub state_filter: StateFilter,
    pub mode: Mode,
    pub cursor: usize,
    pub expanded: HashSet<u32>,
    pub search_query: String,
    pub search_ranked: Vec<usize>,
    pub status: Option<(String, Instant)>,
}

impl AppState {
    pub fn new(issues: Vec<Issue>, all_labels: Vec<Label>) -> Self {
        AppState {
            issues,
            all_labels,
            state_filter: StateFilter::Open,
            mode: Mode::List,
            cursor: 0,
            expanded: HashSet::new(),
            search_query: String::new(),
            search_ranked: Vec::new(),
            status: None,
        }
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        match self.mode {
            Mode::Search => self.search_ranked.clone(),
            _ => (0..self.issues.len()).collect(),
        }
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        let visible = self.visible_indices();
        visible.get(self.cursor).and_then(|&i| self.issues.get(i))
    }

    pub fn find_issue(&self, number: u32) -> Option<&Issue> {
        self.issues.iter().find(|i| i.number == number)
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.visible_indices().len();
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let current = self.cursor as isize;
        let next = (current + delta).rem_euclid(len as isize);
        self.cursor = next as usize;
    }

    pub fn toggle_expand(&mut self) {
        if let Some(number) = self.selected_issue().map(|i| i.number) {
            if !self.expanded.remove(&number) {
                self.expanded.insert(number);
            }
        }
    }

    pub fn expand_selected(&mut self) {
        if let Some(number) = self.selected_issue().map(|i| i.number) {
            self.expanded.insert(number);
        }
    }

    pub fn collapse_selected(&mut self) {
        if let Some(number) = self.selected_issue().map(|i| i.number) {
            self.expanded.remove(&number);
        }
    }

    pub fn set_issues(&mut self, issues: Vec<Issue>) {
        self.issues = issues;
        self.cursor = 0;
    }

    pub fn cycle_state_filter(&mut self) -> StateFilter {
        self.state_filter = self.state_filter.cycle();
        self.state_filter
    }

    pub fn enter_search(&mut self) {
        self.mode = Mode::Search;
        self.search_query.clear();
        self.search_ranked = (0..self.issues.len()).collect();
        self.cursor = 0;
    }

    fn recompute_search(&mut self) {
        if self.search_query.is_empty() {
            self.search_ranked = (0..self.issues.len()).collect();
        } else {
            let titles: Vec<&str> = self.issues.iter().map(|i| i.title.as_str()).collect();
            self.search_ranked = search::rank(&self.search_query, &titles);
        }
        self.cursor = 0;
    }

    pub fn search_push(&mut self, c: char) {
        self.search_query.push(c);
        self.recompute_search();
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.recompute_search();
    }

    pub fn search_delete_word(&mut self) {
        while !self.search_query.is_empty() && self.search_query.ends_with(' ') {
            self.search_query.pop();
        }
        while !self.search_query.is_empty() && !self.search_query.ends_with(' ') {
            self.search_query.pop();
        }
        self.recompute_search();
    }

    pub fn search_clear(&mut self) {
        self.search_query.clear();
        self.recompute_search();
    }

    pub fn exit_search(&mut self) {
        if let Some(&idx) = self.search_ranked.get(self.cursor) {
            self.cursor = idx;
        }
        self.mode = Mode::List;
        self.search_query.clear();
    }

    pub fn enter_little_create(&mut self) {
        self.mode = Mode::LittleCreate(String::new());
    }

    pub fn little_create_push(&mut self, c: char) {
        if let Mode::LittleCreate(buf) = &mut self.mode {
            buf.push(c);
        }
    }

    pub fn little_create_backspace(&mut self) {
        if let Mode::LittleCreate(buf) = &mut self.mode {
            buf.pop();
        }
    }

    pub fn little_create_submit(&mut self) -> Option<String> {
        if let Mode::LittleCreate(buf) = &self.mode {
            let title = buf.trim().to_string();
            if title.is_empty() {
                return None;
            }
            self.mode = Mode::List;
            return Some(title);
        }
        None
    }

    pub fn cancel_form_or_create(&mut self) {
        self.mode = Mode::List;
    }

    fn new_form_state(&self, editing: Option<u32>) -> FormState {
        let all_label_names: Vec<String> = self.all_labels.iter().map(|l| l.name.clone()).collect();
        let (title, body, selected_labels) = match editing.and_then(|n| self.find_issue(n)) {
            Some(issue) => (
                issue.title.clone(),
                issue.body.clone(),
                issue.labels.iter().map(|l| l.name.clone()).collect(),
            ),
            None => (String::new(), String::new(), HashSet::new()),
        };
        FormState { editing, title, body, all_label_names, selected_labels, label_cursor: 0, field: FormField::Title }
    }

    pub fn enter_big_create(&mut self) {
        self.mode = Mode::Form(self.new_form_state(None));
    }

    pub fn enter_edit(&mut self) {
        if let Some(number) = self.selected_issue().map(|i| i.number) {
            self.mode = Mode::Form(self.new_form_state(Some(number)));
        }
    }

    pub fn form_push_char(&mut self, c: char) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => form.title.push(c),
                FormField::Body => form.body.push(c),
                FormField::Labels => {}
            }
        }
    }

    pub fn form_backspace(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => { form.title.pop(); }
                FormField::Body => { form.body.pop(); }
                FormField::Labels => {}
            }
        }
    }

    pub fn form_next_field(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            form.field = match form.field {
                FormField::Title => FormField::Body,
                FormField::Body => FormField::Labels,
                FormField::Labels => FormField::Title,
            };
        }
    }

    pub fn form_prev_field(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            form.field = match form.field {
                FormField::Title => FormField::Labels,
                FormField::Body => FormField::Title,
                FormField::Labels => FormField::Body,
            };
        }
    }

    pub fn form_move_label_cursor(&mut self, delta: isize) {
        if let Mode::Form(form) = &mut self.mode {
            let len = form.all_label_names.len();
            if len == 0 {
                return;
            }
            let next = (form.label_cursor as isize + delta).rem_euclid(len as isize);
            form.label_cursor = next as usize;
        }
    }

    pub fn form_toggle_label(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            if form.field != FormField::Labels {
                return;
            }
            if let Some(name) = form.all_label_names.get(form.label_cursor).cloned() {
                if !form.selected_labels.remove(&name) {
                    form.selected_labels.insert(name);
                }
            }
        }
    }

    pub fn form_enter(&mut self) -> Option<FormSubmission> {
        let (editing, field) = match &self.mode {
            Mode::Form(form) => (form.editing, form.field),
            _ => return None,
        };
        match field {
            FormField::Title => {
                if let Mode::Form(form) = &mut self.mode {
                    form.field = FormField::Body;
                }
                None
            }
            FormField::Body => {
                if let Mode::Form(form) = &mut self.mode {
                    form.body.push('\n');
                }
                None
            }
            FormField::Labels => {
                let original: HashSet<String> = editing
                    .and_then(|n| self.find_issue(n))
                    .map(|issue| issue.labels.iter().map(|l| l.name.clone()).collect())
                    .unwrap_or_default();
                let (title, body, add_labels, remove_labels) = match &self.mode {
                    Mode::Form(form) => {
                        let add_labels: Vec<String> = form.selected_labels.difference(&original).cloned().collect();
                        let remove_labels: Vec<String> = original.difference(&form.selected_labels).cloned().collect();
                        (form.title.clone(), form.body.clone(), add_labels, remove_labels)
                    }
                    _ => return None,
                };
                let submission = FormSubmission { editing, title, body, add_labels, remove_labels };
                self.mode = Mode::List;
                Some(submission)
            }
        }
    }

    pub fn request_close(&mut self) {
        if let Some(number) = self.selected_issue().map(|i| i.number) {
            self.mode = Mode::ConfirmClose(number);
        }
    }

    pub fn confirm_close_yes(&mut self) -> Option<u32> {
        if let Mode::ConfirmClose(number) = self.mode {
            self.mode = Mode::List;
            Some(number)
        } else {
            None
        }
    }

    pub fn confirm_close_no(&mut self) {
        self.mode = Mode::List;
    }

    pub fn set_status(&mut self, message: String) {
        self.status = Some((message, Instant::now()));
    }

    pub fn clear_expired_status(&mut self) {
        if let Some((_, set_at)) = &self.status {
            if set_at.elapsed() >= STATUS_TOAST_DURATION {
                self.status = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gh::StateFilter;

    fn issue(number: u32, title: &str) -> Issue {
        Issue {
            number,
            title: title.into(),
            body: String::new(),
            labels: vec![],
            state: IssueState::Open,
            url: format!("https://example.com/{number}"),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn move_cursor_wraps_around() {
        let mut state = AppState::new(vec![issue(1, "a"), issue(2, "b")], vec![]);
        state.move_cursor(-1);
        assert_eq!(state.cursor, 1, "moving up from 0 wraps to the last row");
        state.move_cursor(1);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn toggle_expand_tracks_selected_issue_number() {
        let mut state = AppState::new(vec![issue(1, "a"), issue(2, "b")], vec![]);
        state.toggle_expand();
        assert!(state.expanded.contains(&1));
        state.toggle_expand();
        assert!(!state.expanded.contains(&1));
    }

    #[test]
    fn expand_selected_is_idempotent() {
        let mut state = AppState::new(vec![issue(1, "a"), issue(2, "b")], vec![]);
        state.expand_selected();
        assert!(state.expanded.contains(&1));
        state.expand_selected();
        assert!(state.expanded.contains(&1), "expanding an already-expanded issue is a no-op");
    }

    #[test]
    fn collapse_selected_is_idempotent() {
        let mut state = AppState::new(vec![issue(1, "a"), issue(2, "b")], vec![]);
        state.collapse_selected();
        assert!(!state.expanded.contains(&1), "collapsing a not-expanded issue is a no-op");
        state.expand_selected();
        state.collapse_selected();
        assert!(!state.expanded.contains(&1));
    }

    #[test]
    fn search_filters_and_ranks_by_title() {
        let mut state = AppState::new(vec![issue(1, "a pretty big input"), issue(2, "api gateway")], vec![]);
        state.enter_search();
        state.search_push('a');
        state.search_push('p');
        state.search_push('i');
        assert_eq!(state.search_ranked.first().copied(), Some(1), "consecutive prefix match ranks first");
    }

    #[test]
    fn exit_search_parks_cursor_on_matched_issue() {
        let mut state = AppState::new(vec![issue(1, "alpha"), issue(2, "beta")], vec![]);
        state.enter_search();
        state.search_push('b');
        assert_eq!(state.cursor, 0, "only match is at ranked position 0");
        state.exit_search();
        assert_eq!(state.cursor, 1, "cursor lands on issue 2's absolute index in the full list");
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn search_delete_word_removes_last_word() {
        let mut state = AppState::new(vec![issue(1, "fix login error")], vec![]);
        state.enter_search();
        for c in "fix login".chars() {
            state.search_push(c);
        }
        assert_eq!(state.search_query, "fix login");
        state.search_delete_word();
        assert_eq!(state.search_query, "fix ");
    }

    #[test]
    fn search_clear_empties_query() {
        let mut state = AppState::new(vec![issue(1, "test")], vec![]);
        state.enter_search();
        for c in "test query".chars() {
            state.search_push(c);
        }
        assert_eq!(state.search_query, "test query");
        state.search_clear();
        assert_eq!(state.search_query, "");
        assert_eq!(state.search_ranked.len(), 1, "resets ranking to show all issues");
    }

    #[test]
    fn cycle_state_filter_advances_open_closed_all() {
        let mut state = AppState::new(vec![], vec![]);
        assert_eq!(state.cycle_state_filter(), StateFilter::Closed);
        assert_eq!(state.cycle_state_filter(), StateFilter::All);
        assert_eq!(state.cycle_state_filter(), StateFilter::Open);
    }

    #[test]
    fn little_create_submit_returns_trimmed_title_and_resets_mode() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        for c in "  Fix the bug  ".chars() {
            state.little_create_push(c);
        }
        let submitted = state.little_create_submit();
        assert_eq!(submitted, Some("Fix the bug".to_string()));
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn little_create_submit_rejects_blank_title() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        state.little_create_push(' ');
        assert_eq!(state.little_create_submit(), None);
        assert!(matches!(state.mode, Mode::LittleCreate(_)), "stays in create mode on blank submit");
    }

    #[test]
    fn enter_edit_prefills_form_from_selected_issue() {
        let mut issue1 = issue(1, "Fix bug");
        issue1.body = "steps to repro".into();
        issue1.labels = vec![Label { name: "bug".into(), color: "d73a4a".into() }];
        let mut state = AppState::new(
            vec![issue1],
            vec![
                Label { name: "bug".into(), color: "d73a4a".into() },
                Label { name: "docs".into(), color: "0075ca".into() },
            ],
        );
        state.enter_edit();
        match &state.mode {
            Mode::Form(form) => {
                assert_eq!(form.editing, Some(1));
                assert_eq!(form.title, "Fix bug");
                assert_eq!(form.body, "steps to repro");
                assert!(form.selected_labels.contains("bug"));
                assert_eq!(form.all_label_names, vec!["bug".to_string(), "docs".to_string()]);
            }
            other => panic!("expected Form mode, got {other:?}"),
        }
    }

    #[test]
    fn form_toggle_label_only_applies_when_labels_field_focused() {
        let mut state = AppState::new(vec![], vec![Label { name: "bug".into(), color: "d73a4a".into() }]);
        state.enter_big_create();
        state.form_toggle_label(); // field is Title, should be ignored
        if let Mode::Form(form) = &state.mode {
            assert!(form.selected_labels.is_empty());
        }
        state.form_next_field(); // Body
        state.form_next_field(); // Labels
        state.form_toggle_label();
        if let Mode::Form(form) = &state.mode {
            assert!(form.selected_labels.contains("bug"));
        }
    }

    #[test]
    fn form_enter_on_title_advances_to_body_without_submitting() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        assert_eq!(state.form_enter(), None);
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.field, FormField::Body);
        }
    }

    #[test]
    fn form_enter_on_labels_submits_and_returns_to_list() {
        let mut state = AppState::new(vec![], vec![Label { name: "bug".into(), color: "d73a4a".into() }]);
        state.enter_big_create();
        state.form_push_char('T');
        state.form_next_field();
        state.form_push_char('B');
        state.form_next_field();
        state.form_toggle_label();
        let submission = state.form_enter().expect("labels field submits on enter");
        assert_eq!(submission.editing, None);
        assert_eq!(submission.title, "T");
        assert_eq!(submission.body, "B");
        assert_eq!(submission.add_labels, vec!["bug".to_string()]);
        assert!(submission.remove_labels.is_empty());
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn form_enter_while_editing_diffs_labels_against_original() {
        let mut issue1 = issue(1, "Fix bug");
        issue1.labels = vec![
            Label { name: "bug".into(), color: "d73a4a".into() },
            Label { name: "keep".into(), color: "0075ca".into() },
        ];
        let all_labels = vec![
            Label { name: "bug".into(), color: "d73a4a".into() },
            Label { name: "keep".into(), color: "0075ca".into() },
            Label { name: "docs".into(), color: "0075ca".into() },
        ];
        let mut state = AppState::new(vec![issue1], all_labels);
        state.enter_edit();
        state.form_next_field(); // Body
        state.form_next_field(); // Labels, cursor starts at 0 ("bug")
        state.form_toggle_label(); // toggle "bug" off
        state.form_move_label_cursor(2); // to "docs"
        state.form_toggle_label(); // toggle "docs" on
        let submission = state.form_enter().unwrap();
        assert_eq!(submission.editing, Some(1));
        assert_eq!(submission.add_labels, vec!["docs".to_string()]);
        assert_eq!(submission.remove_labels, vec!["bug".to_string()]);
    }

    #[test]
    fn confirm_close_flow() {
        let mut state = AppState::new(vec![issue(9, "close me")], vec![]);
        state.request_close();
        assert_eq!(state.mode, Mode::ConfirmClose(9));
        assert_eq!(state.confirm_close_yes(), Some(9));
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn confirm_close_no_returns_to_list_without_closing() {
        let mut state = AppState::new(vec![issue(9, "close me")], vec![]);
        state.request_close();
        state.confirm_close_no();
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn fresh_status_survives_expiry_check() {
        let mut state = AppState::new(vec![], vec![]);
        state.set_status("copied: #1".to_string());
        state.clear_expired_status();
        assert!(state.status.is_some(), "a just-set status hasn't reached the toast duration yet");
    }

    #[test]
    fn stale_status_is_cleared_by_expiry_check() {
        let mut state = AppState::new(vec![], vec![]);
        let set_at = Instant::now() - STATUS_TOAST_DURATION - Duration::from_millis(1);
        state.status = Some(("copied: #1".to_string(), set_at));
        state.clear_expired_status();
        assert!(state.status.is_none(), "a status older than STATUS_TOAST_DURATION should be cleared");
    }
}
