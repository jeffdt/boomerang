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
}

use crate::gh::StateFilter;
use crate::search;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    List,
    Search,
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
    pub status: Option<String>,
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

    pub fn exit_search(&mut self) {
        if let Some(&idx) = self.search_ranked.get(self.cursor) {
            self.cursor = idx;
        }
        self.mode = Mode::List;
        self.search_query.clear();
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
    fn cycle_state_filter_advances_open_closed_all() {
        let mut state = AppState::new(vec![], vec![]);
        assert_eq!(state.cycle_state_filter(), StateFilter::Closed);
        assert_eq!(state.cycle_state_filter(), StateFilter::All);
        assert_eq!(state.cycle_state_filter(), StateFilter::Open);
    }
}
