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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const STATUS_TOAST_DURATION: Duration = Duration::from_secs(2);
const ACTIVITY_SPINNER_INTERVAL: Duration = Duration::from_millis(100);
const ACTIVITY_SPINNER_FRAMES: [&str; 4] = ["|", "/", "-", "\\"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOperation {
    CreateIssue,
    EditIssue,
}

#[derive(Debug, Clone)]
pub struct PendingState {
    pub operation: PendingOperation,
    pub started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingAnimation {
    MatrixRain,
    ColorRipple,
    RainbowRipple,
}

impl LoadingAnimation {
    const ALL: [LoadingAnimation; 1] = [LoadingAnimation::MatrixRain];

    pub fn for_launch() -> Self {
        std::env::var("ISSUE_BROWSER_LOADING_ANIMATION")
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or_else(Self::rotated)
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "matrix" | "matrix-rain" | "rain" => Some(Self::MatrixRain),
            "ripple" | "color-ripple" | "bullseye" => Some(Self::ColorRipple),
            "rainbow" | "rainbow-ripple" | "rings" => Some(Self::RainbowRipple),
            _ => None,
        }
    }

    fn rotated() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as usize)
            .unwrap_or(0);
        Self::ALL[millis % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub struct LoadingState {
    pub started_at: Instant,
    pub animation: LoadingAnimation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    List,
    Search,
    LittleCreate(String),
    Form(Box<FormState>),
    ConfirmClose(u32),
    ConfirmDiscard(Box<Mode>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormField {
    #[default]
    Title,
    Body,
    Labels,
    Submit,
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
    pub original_title: String,
    pub original_body: String,
    pub original_labels: HashSet<String>,
    pub title_cursor: usize,
    pub body_cursor: usize,
}

impl FormState {
    pub fn is_dirty(&self) -> bool {
        self.title != self.original_title
            || self.body != self.original_body
            || self.selected_labels != self.original_labels
    }
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
    pub search_query: String,
    pub search_ranked: Vec<usize>,
    pub status: Option<(String, Instant)>,
    pub loading: Option<LoadingState>,
    pub pending: Option<PendingState>,
    pub pane_open: bool,
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

fn delete_word_before(s: &mut String, cursor: &mut usize) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = *cursor;
    while i > 0 && chars[i - 1] == ' ' {
        i -= 1;
    }
    while i > 0 && chars[i - 1] != ' ' && chars[i - 1] != '\n' {
        i -= 1;
    }
    let start_byte = char_byte_index(s, i);
    let end_byte = char_byte_index(s, *cursor);
    s.replace_range(start_byte..end_byte, "");
    *cursor = i;
}

impl AppState {
    pub fn new(issues: Vec<Issue>, all_labels: Vec<Label>) -> Self {
        AppState {
            issues,
            all_labels,
            state_filter: StateFilter::Open,
            mode: Mode::List,
            cursor: 0,
            search_query: String::new(),
            search_ranked: Vec::new(),
            status: None,
            loading: None,
            pending: None,
            pane_open: false,
        }
    }

    pub fn loading() -> Self {
        let mut state = Self::new(vec![], vec![]);
        state.begin_loading();
        state
    }

    pub fn set_loaded(&mut self, issues: Vec<Issue>, all_labels: Vec<Label>) {
        self.all_labels = all_labels;
        self.set_issues(issues);
        self.finish_loading();
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

    pub fn toggle_pane(&mut self) {
        self.pane_open = !self.pane_open;
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
        let dirty = match &self.mode {
            Mode::Form(form) => form.is_dirty(),
            Mode::LittleCreate(buf) => !buf.trim().is_empty(),
            _ => false,
        };
        if dirty {
            let previous = std::mem::replace(&mut self.mode, Mode::List);
            self.mode = Mode::ConfirmDiscard(Box::new(previous));
        } else {
            self.mode = Mode::List;
        }
    }

    pub fn confirm_discard_yes(&mut self) {
        if matches!(self.mode, Mode::ConfirmDiscard(_)) {
            self.mode = Mode::List;
        }
    }

    pub fn confirm_discard_no(&mut self) {
        if let Mode::ConfirmDiscard(previous) = std::mem::replace(&mut self.mode, Mode::List) {
            self.mode = *previous;
        }
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
        let title_cursor = title.chars().count();
        let body_cursor = body.chars().count();
        FormState {
            editing,
            title: title.clone(),
            body: body.clone(),
            all_label_names,
            selected_labels: selected_labels.clone(),
            label_cursor: 0,
            field: FormField::Title,
            original_title: title,
            original_body: body,
            original_labels: selected_labels,
            title_cursor,
            body_cursor,
        }
    }

    pub fn enter_big_create(&mut self) {
        self.mode = Mode::Form(Box::new(self.new_form_state(None)));
    }

    pub fn enter_edit(&mut self) {
        if let Some(number) = self.selected_issue().map(|i| i.number) {
            self.mode = Mode::Form(Box::new(self.new_form_state(Some(number))));
        }
    }

    pub fn form_push_char(&mut self, c: char) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => {
                    let idx = char_byte_index(&form.title, form.title_cursor);
                    form.title.insert(idx, c);
                    form.title_cursor += 1;
                }
                FormField::Body => {
                    let idx = char_byte_index(&form.body, form.body_cursor);
                    form.body.insert(idx, c);
                    form.body_cursor += 1;
                }
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_backspace(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => {
                    if form.title_cursor > 0 {
                        let start = char_byte_index(&form.title, form.title_cursor - 1);
                        let end = char_byte_index(&form.title, form.title_cursor);
                        form.title.replace_range(start..end, "");
                        form.title_cursor -= 1;
                    }
                }
                FormField::Body => {
                    if form.body_cursor > 0 {
                        let start = char_byte_index(&form.body, form.body_cursor - 1);
                        let end = char_byte_index(&form.body, form.body_cursor);
                        form.body.replace_range(start..end, "");
                        form.body_cursor -= 1;
                    }
                }
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_move_cursor(&mut self, delta: isize) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => {
                    let len = char_len(&form.title) as isize;
                    form.title_cursor = (form.title_cursor as isize + delta).clamp(0, len) as usize;
                }
                FormField::Body => {
                    let len = char_len(&form.body) as isize;
                    form.body_cursor = (form.body_cursor as isize + delta).clamp(0, len) as usize;
                }
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_cursor_home(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => form.title_cursor = 0,
                FormField::Body => {
                    let chars: Vec<char> = form.body.chars().collect();
                    let mut i = form.body_cursor;
                    while i > 0 && chars[i - 1] != '\n' {
                        i -= 1;
                    }
                    form.body_cursor = i;
                }
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_cursor_end(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => form.title_cursor = char_len(&form.title),
                FormField::Body => {
                    let chars: Vec<char> = form.body.chars().collect();
                    let mut i = form.body_cursor;
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                    form.body_cursor = i;
                }
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_move_cursor_vertical(&mut self, delta: isize) {
        if let Mode::Form(form) = &mut self.mode {
            if form.field != FormField::Body {
                return;
            }
            let chars: Vec<char> = form.body.chars().collect();
            let mut line_start = form.body_cursor;
            while line_start > 0 && chars[line_start - 1] != '\n' {
                line_start -= 1;
            }
            let column = form.body_cursor - line_start;

            if delta < 0 {
                if line_start == 0 {
                    return;
                }
                let prev_line_end = line_start - 1;
                let mut prev_line_start = prev_line_end;
                while prev_line_start > 0 && chars[prev_line_start - 1] != '\n' {
                    prev_line_start -= 1;
                }
                let prev_line_len = prev_line_end - prev_line_start;
                form.body_cursor = prev_line_start + column.min(prev_line_len);
            } else if delta > 0 {
                let mut line_end = form.body_cursor;
                while line_end < chars.len() && chars[line_end] != '\n' {
                    line_end += 1;
                }
                if line_end == chars.len() {
                    return;
                }
                let next_line_start = line_end + 1;
                let mut next_line_end = next_line_start;
                while next_line_end < chars.len() && chars[next_line_end] != '\n' {
                    next_line_end += 1;
                }
                let next_line_len = next_line_end - next_line_start;
                form.body_cursor = next_line_start + column.min(next_line_len);
            }
        }
    }

    pub fn form_delete_word(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => delete_word_before(&mut form.title, &mut form.title_cursor),
                FormField::Body => delete_word_before(&mut form.body, &mut form.body_cursor),
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_clear_to_line_start(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            match form.field {
                FormField::Title => {
                    let end_byte = char_byte_index(&form.title, form.title_cursor);
                    form.title.replace_range(0..end_byte, "");
                    form.title_cursor = 0;
                }
                FormField::Body => {
                    let chars: Vec<char> = form.body.chars().collect();
                    let mut line_start = form.body_cursor;
                    while line_start > 0 && chars[line_start - 1] != '\n' {
                        line_start -= 1;
                    }
                    let start_byte = char_byte_index(&form.body, line_start);
                    let end_byte = char_byte_index(&form.body, form.body_cursor);
                    form.body.replace_range(start_byte..end_byte, "");
                    form.body_cursor = line_start;
                }
                FormField::Labels | FormField::Submit => {}
            }
        }
    }

    pub fn form_next_field(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            form.field = match form.field {
                FormField::Title => FormField::Body,
                FormField::Body => FormField::Labels,
                FormField::Labels => FormField::Submit,
                FormField::Submit => FormField::Title,
            };
        }
    }

    pub fn form_prev_field(&mut self) {
        if let Mode::Form(form) = &mut self.mode {
            form.field = match form.field {
                FormField::Title => FormField::Submit,
                FormField::Body => FormField::Title,
                FormField::Labels => FormField::Body,
                FormField::Submit => FormField::Labels,
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
                        let add_labels: Vec<String> = form
                            .selected_labels
                            .difference(&original)
                            .cloned()
                            .collect();
                        let remove_labels: Vec<String> = original
                            .difference(&form.selected_labels)
                            .cloned()
                            .collect();
                        (
                            form.title.clone(),
                            form.body.clone(),
                            add_labels,
                            remove_labels,
                        )
                    }
                    _ => return None,
                };
                let submission = FormSubmission {
                    editing,
                    title,
                    body,
                    add_labels,
                    remove_labels,
                };
                self.mode = Mode::List;
                Some(submission)
            }
            FormField::Submit => None,
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

    pub fn begin_loading(&mut self) {
        self.loading = Some(LoadingState {
            started_at: Instant::now(),
            animation: LoadingAnimation::for_launch(),
        });
    }

    pub fn finish_loading(&mut self) {
        self.loading = None;
    }

    pub fn is_loading(&self) -> bool {
        self.loading.is_some()
    }

    pub fn loading_message(&self) -> Option<String> {
        let loading = self.loading.as_ref()?;
        Some(format!(
            "{} Loading issues...",
            spinner_frame(&loading.started_at)
        ))
    }

    pub fn begin_pending(&mut self, operation: PendingOperation) {
        self.pending = Some(PendingState {
            operation,
            started_at: Instant::now(),
        });
    }

    pub fn finish_pending(&mut self) {
        self.pending = None;
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn pending_message(&self) -> Option<String> {
        let pending = self.pending.as_ref()?;
        let action = match pending.operation {
            PendingOperation::CreateIssue => "Creating issue",
            PendingOperation::EditIssue => "Updating issue",
        };
        Some(format!(
            "{} {action}...",
            spinner_frame(&pending.started_at)
        ))
    }

    pub fn clear_expired_status(&mut self) {
        if let Some((_, set_at)) = &self.status {
            if set_at.elapsed() >= STATUS_TOAST_DURATION {
                self.status = None;
            }
        }
    }
}

fn spinner_frame(started_at: &Instant) -> &'static str {
    let frame_index = ((started_at.elapsed().as_millis() / ACTIVITY_SPINNER_INTERVAL.as_millis())
        as usize)
        % ACTIVITY_SPINNER_FRAMES.len();
    ACTIVITY_SPINNER_FRAMES[frame_index]
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
    fn search_filters_and_ranks_by_title() {
        let mut state = AppState::new(
            vec![issue(1, "a pretty big input"), issue(2, "api gateway")],
            vec![],
        );
        state.enter_search();
        state.search_push('a');
        state.search_push('p');
        state.search_push('i');
        assert_eq!(
            state.search_ranked.first().copied(),
            Some(1),
            "consecutive prefix match ranks first"
        );
    }

    #[test]
    fn exit_search_parks_cursor_on_matched_issue() {
        let mut state = AppState::new(vec![issue(1, "alpha"), issue(2, "beta")], vec![]);
        state.enter_search();
        state.search_push('b');
        assert_eq!(state.cursor, 0, "only match is at ranked position 0");
        state.exit_search();
        assert_eq!(
            state.cursor, 1,
            "cursor lands on issue 2's absolute index in the full list"
        );
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
        assert_eq!(
            state.search_ranked.len(),
            1,
            "resets ranking to show all issues"
        );
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
        assert!(
            matches!(state.mode, Mode::LittleCreate(_)),
            "stays in create mode on blank submit"
        );
    }

    #[test]
    fn enter_edit_prefills_form_from_selected_issue() {
        let mut issue1 = issue(1, "Fix bug");
        issue1.body = "steps to repro".into();
        issue1.labels = vec![Label {
            name: "bug".into(),
            color: "d73a4a".into(),
        }];
        let mut state = AppState::new(
            vec![issue1],
            vec![
                Label {
                    name: "bug".into(),
                    color: "d73a4a".into(),
                },
                Label {
                    name: "docs".into(),
                    color: "0075ca".into(),
                },
            ],
        );
        state.enter_edit();
        match &state.mode {
            Mode::Form(form) => {
                assert_eq!(form.editing, Some(1));
                assert_eq!(form.title, "Fix bug");
                assert_eq!(form.body, "steps to repro");
                assert!(form.selected_labels.contains("bug"));
                assert_eq!(
                    form.all_label_names,
                    vec!["bug".to_string(), "docs".to_string()]
                );
            }
            other => panic!("expected Form mode, got {other:?}"),
        }
    }

    #[test]
    fn form_toggle_label_only_applies_when_labels_field_focused() {
        let mut state = AppState::new(
            vec![],
            vec![Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
        );
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
    fn tab_cycle_now_includes_submit_after_labels() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        state.form_next_field(); // Labels
        state.form_next_field(); // Submit
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.field, FormField::Submit);
        } else {
            panic!("expected Form mode");
        }
        state.form_next_field(); // wraps back to Title
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.field, FormField::Title);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn shift_tab_from_title_reaches_submit() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_prev_field();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.field, FormField::Submit);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn new_form_state_starts_cursors_at_end_of_existing_text() {
        let mut issue1 = issue(1, "Fix bug");
        issue1.body = "line one".to_string();
        let mut state = AppState::new(vec![issue1], vec![]);
        state.enter_edit();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title_cursor, "Fix bug".chars().count());
            assert_eq!(form.body_cursor, "line one".chars().count());
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn form_push_char_inserts_at_cursor_not_always_at_end() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('f');
        state.form_push_char('o');
        state.form_push_char('o');
        state.form_move_cursor(-1); // between the two 'o's
        state.form_push_char('X');
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title, "foXo");
            assert_eq!(form.title_cursor, 3);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn form_backspace_deletes_char_before_cursor_not_always_last_char() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('f');
        state.form_push_char('o');
        state.form_push_char('o');
        state.form_move_cursor(-1);
        state.form_backspace();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title, "fo");
            assert_eq!(form.title_cursor, 1);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn form_backspace_at_start_of_field_is_a_no_op() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('a');
        state.form_move_cursor(-10); // clamps to 0
        state.form_backspace();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title, "a");
            assert_eq!(form.title_cursor, 0);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn cursor_home_and_end_on_title_go_to_field_boundaries() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        for c in "hello".chars() {
            state.form_push_char(c);
        }
        state.form_cursor_home();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title_cursor, 0);
        } else {
            panic!("expected Form mode");
        }
        state.form_cursor_end();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title_cursor, 5);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn cursor_home_and_end_on_body_are_scoped_to_current_line() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        for c in "one\ntwo".chars() {
            state.form_push_char(c);
        }
        // cursor is at the end, on the "two" line
        state.form_cursor_home();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body_cursor, 4, "home should land right after the \\n, not at index 0");
        } else {
            panic!("expected Form mode");
        }
        state.form_cursor_end();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body_cursor, 7, "end should land at the end of the string");
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn vertical_movement_preserves_column_across_explicit_lines() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        for c in "abcdef\nxy".chars() {
            state.form_push_char(c);
        }
        state.form_cursor_home(); // column 0 of "xy" line (index 7)
        state.form_move_cursor(2); // column 2 of "xy" line (index 9, end of "xy")
        state.form_move_cursor_vertical(-1);
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body_cursor, 2, "should land at column 2 on the first line (\"abcdef\")");
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn vertical_movement_clamps_to_shorter_line_length() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        for c in "abcdef\nxy".chars() {
            state.form_push_char(c);
        }
        // cursor at end of "xy" (index 9); go up then back down
        state.form_move_cursor_vertical(-1); // column 2 on "abcdef" -> index 2
        state.form_move_cursor_vertical(1); // back down to "xy", column 2 doesn't exist (len 2) -> clamp to end, index 9
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body_cursor, 9);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn vertical_movement_is_a_no_op_past_first_or_last_line() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        for c in "only one line".chars() {
            state.form_push_char(c);
        }
        state.form_move_cursor_vertical(-1);
        state.form_move_cursor_vertical(1);
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body_cursor, "only one line".chars().count());
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn vertical_movement_on_title_is_a_no_op() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('a');
        state.form_move_cursor_vertical(-1);
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title_cursor, 1);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn delete_word_removes_word_immediately_before_cursor() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        for c in "fix the bug".chars() {
            state.form_push_char(c);
        }
        state.form_delete_word();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title, "fix the ");
            assert_eq!(form.title_cursor, 8);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn delete_word_skips_trailing_spaces_before_the_word() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        for c in "fix   ".chars() {
            state.form_push_char(c);
        }
        state.form_delete_word();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title, "");
            assert_eq!(form.title_cursor, 0);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn delete_word_in_body_does_not_cross_a_newline() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        for c in "one\ntwo".chars() {
            state.form_push_char(c);
        }
        state.form_delete_word();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body, "one\n", "word delete must stop at the preceding newline, not eat \"one\" too");
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn clear_to_line_start_on_title_clears_whole_field() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        for c in "hello world".chars() {
            state.form_push_char(c);
        }
        state.form_move_cursor(-5); // between "hello " and "world"
        state.form_clear_to_line_start();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.title, "world");
            assert_eq!(form.title_cursor, 0);
        } else {
            panic!("expected Form mode");
        }
    }

    #[test]
    fn clear_to_line_start_on_body_is_scoped_to_current_line() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Body
        for c in "one\ntwo".chars() {
            state.form_push_char(c);
        }
        state.form_clear_to_line_start();
        if let Mode::Form(form) = &state.mode {
            assert_eq!(form.body, "one\n", "clearing on the second line must not touch the first line");
            assert_eq!(form.body_cursor, 4);
        } else {
            panic!("expected Form mode");
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
        let mut state = AppState::new(
            vec![],
            vec![Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
        );
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
            Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            },
            Label {
                name: "keep".into(),
                color: "0075ca".into(),
            },
        ];
        let all_labels = vec![
            Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            },
            Label {
                name: "keep".into(),
                color: "0075ca".into(),
            },
            Label {
                name: "docs".into(),
                color: "0075ca".into(),
            },
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
    fn cancel_on_clean_form_returns_directly_to_list() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.cancel_form_or_create();
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn cancel_on_dirty_form_asks_for_confirmation() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('T');
        state.cancel_form_or_create();
        assert!(matches!(state.mode, Mode::ConfirmDiscard(_)), "typed title should be treated as dirty");
    }

    #[test]
    fn cancel_on_dirty_form_via_label_toggle_asks_for_confirmation() {
        let mut state = AppState::new(vec![], vec![Label { name: "bug".into(), color: "d73a4a".into() }]);
        state.enter_big_create();
        state.form_next_field(); // Body
        state.form_next_field(); // Labels
        state.form_toggle_label();
        state.cancel_form_or_create();
        assert!(matches!(state.mode, Mode::ConfirmDiscard(_)), "a checked label box should be treated as dirty");
    }

    #[test]
    fn confirm_discard_yes_abandons_changes_and_returns_to_list() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('T');
        state.cancel_form_or_create();
        state.confirm_discard_yes();
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn confirm_discard_no_restores_the_in_progress_form() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_push_char('T');
        state.cancel_form_or_create();
        state.confirm_discard_no();
        match &state.mode {
            Mode::Form(form) => assert_eq!(form.title, "T"),
            other => panic!("expected to return to Form mode with typed content intact, got {other:?}"),
        }
    }

    #[test]
    fn cancel_on_dirty_little_create_asks_for_confirmation() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        state.little_create_push('F');
        state.cancel_form_or_create();
        assert!(matches!(state.mode, Mode::ConfirmDiscard(_)));
        state.confirm_discard_no();
        assert_eq!(state.mode, Mode::LittleCreate("F".to_string()));
    }

    #[test]
    fn cancel_on_blank_little_create_returns_directly_to_list() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        state.cancel_form_or_create();
        assert_eq!(state.mode, Mode::List);
    }

    #[test]
    fn editing_an_issue_without_changes_is_not_dirty() {
        let mut issue1 = issue(1, "Fix bug");
        issue1.labels = vec![Label { name: "bug".into(), color: "d73a4a".into() }];
        let mut state = AppState::new(vec![issue1], vec![Label { name: "bug".into(), color: "d73a4a".into() }]);
        state.enter_edit();
        state.cancel_form_or_create();
        assert_eq!(state.mode, Mode::List, "re-opening an edit form unchanged should not be considered dirty");
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
        assert!(
            state.status.is_some(),
            "a just-set status hasn't reached the toast duration yet"
        );
    }

    #[test]
    fn stale_status_is_cleared_by_expiry_check() {
        let mut state = AppState::new(vec![], vec![]);
        let set_at = Instant::now() - STATUS_TOAST_DURATION - Duration::from_millis(1);
        state.status = Some(("copied: #1".to_string(), set_at));
        state.clear_expired_status();
        assert!(
            state.status.is_none(),
            "a status older than STATUS_TOAST_DURATION should be cleared"
        );
    }

    #[test]
    fn begin_create_pending_preserves_mode_and_reports_spinner_text() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        state.begin_pending(PendingOperation::CreateIssue);
        assert!(matches!(state.mode, Mode::LittleCreate(_)));
        assert!(state.is_pending());
        assert_eq!(
            state.pending.as_ref().map(|p| p.operation),
            Some(PendingOperation::CreateIssue)
        );
        assert!(state
            .pending_message()
            .unwrap()
            .contains("Creating issue..."));
    }

    #[test]
    fn begin_edit_pending_preserves_mode_and_reports_spinner_text() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.begin_pending(PendingOperation::EditIssue);
        assert!(matches!(state.mode, Mode::Form(_)));
        assert!(state.is_pending());
        assert_eq!(
            state.pending.as_ref().map(|p| p.operation),
            Some(PendingOperation::EditIssue)
        );
        assert!(state
            .pending_message()
            .unwrap()
            .contains("Updating issue..."));
    }

    #[test]
    fn finish_pending_clears_pending_state() {
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::CreateIssue);
        state.finish_pending();
        assert!(!state.is_pending());
        assert_eq!(state.pending_message(), None);
    }

    #[test]
    fn loading_state_reports_message_and_selected_animation() {
        let state = AppState::loading();
        assert!(state.is_loading());
        assert_eq!(
            state.loading.as_ref().map(|loading| loading.animation),
            Some(LoadingAnimation::MatrixRain)
        );
        assert!(state
            .loading_message()
            .expect("loading message")
            .contains("Loading issues..."));
    }

    #[test]
    fn set_loaded_replaces_startup_data_and_clears_loading() {
        let loaded_issue = issue(7, "Loaded issue");
        let label = Label {
            name: "bug".into(),
            color: "d73a4a".into(),
        };
        let mut state = AppState::loading();
        state.set_loaded(vec![loaded_issue.clone()], vec![label.clone()]);
        assert_eq!(state.issues, vec![loaded_issue]);
        assert_eq!(state.all_labels, vec![label]);
        assert!(!state.is_loading());
        assert_eq!(state.loading_message(), None);
    }

    #[test]
    fn loading_animation_parse_accepts_experiment_names() {
        assert_eq!(
            LoadingAnimation::parse("matrix-rain"),
            Some(LoadingAnimation::MatrixRain)
        );
        assert_eq!(
            LoadingAnimation::parse("bullseye"),
            Some(LoadingAnimation::ColorRipple)
        );
        assert_eq!(
            LoadingAnimation::parse("rainbow-ripple"),
            Some(LoadingAnimation::RainbowRipple)
        );
        assert_eq!(LoadingAnimation::parse("orbit"), None);
        assert_eq!(LoadingAnimation::parse("pipes"), None);
        assert_eq!(LoadingAnimation::parse("unknown"), None);
    }

    #[test]
    fn toggle_pane_flips_the_flag_and_starts_closed() {
        let mut state = AppState::new(vec![], vec![]);
        assert!(
            !state.pane_open,
            "pane should be hidden on a fresh AppState"
        );
        state.toggle_pane();
        assert!(state.pane_open);
        state.toggle_pane();
        assert!(!state.pane_open);
    }
}
