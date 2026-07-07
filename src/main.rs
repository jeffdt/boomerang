mod copy;
mod diagnostics;
mod gh;
mod loading;
mod model;
mod search;
mod ui;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use gh::{GhCliSource, IssueSource, StateFilter};
use model::{AppState, FormState, Issue, Label, Mode, PendingOperation};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use ui::{
    map_confirm_key, map_form_key, map_list_key, map_little_create_key, map_search_key,
    ConfirmInput, FormInput, ListInput, LittleCreateInput, SearchInput,
};

const HELP: &str = "\
issue-browser - a tmux-popup TUI for GitHub issues

Usage:
  issue-browser            Launch the picker (intended via `tmux popup -E`)
  issue-browser --doctor   Print gh, repo, auth, and logging diagnostics
  issue-browser --version  Print version and exit
  issue-browser --help     Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind i display-popup -E -B -w 84 -h 60% \"exec issue-browser\"";

fn main() -> anyhow::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("issue-browser {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            "--doctor" => {
                diagnostics::run_doctor()?;
                return Ok(());
            }
            other => {
                eprintln!("issue-browser: unknown argument '{other}'\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    if std::process::Command::new("gh")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("issue-browser: `gh` CLI not found on PATH. Install it from https://cli.github.com and run `gh auth login`.");
        std::process::exit(1);
    }
    let authenticated = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !authenticated {
        eprintln!("issue-browser: `gh` is not authenticated. Run `gh auth login` first.");
        std::process::exit(1);
    }

    let source = GhCliSource::new();
    let mut state = AppState::loading();
    let initial_load_rx = spawn_initial_load(source.clone());

    run_ui(&mut state, &source, initial_load_rx)
}

fn run_ui<S: IssueSource>(
    state: &mut AppState,
    source: &S,
    initial_load_rx: InitialLoadReceiver,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let result = event_loop(&mut terminal, state, source, Some(initial_load_rx));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

#[derive(Debug, Clone)]
enum MutationDraft {
    LittleCreate { title: String },
    Form(FormState),
}

#[derive(Debug, Clone)]
struct CreateRequest {
    title: String,
    body: String,
    labels: Vec<String>,
}

#[derive(Debug, Clone)]
struct EditRequest {
    number: u32,
    title: String,
    body: String,
    add_labels: Vec<String>,
    remove_labels: Vec<String>,
}

#[derive(Debug, Clone)]
enum MutationRequest {
    Create(CreateRequest),
    Edit(EditRequest),
}

impl MutationRequest {
    fn operation(&self) -> PendingOperation {
        match self {
            MutationRequest::Create(_) => PendingOperation::CreateIssue,
            MutationRequest::Edit(_) => PendingOperation::EditIssue,
        }
    }

    fn run<S: IssueSource>(self, source: &S) -> anyhow::Result<()> {
        match self {
            MutationRequest::Create(request) => {
                source.create(&request.title, &request.body, &request.labels)
            }
            MutationRequest::Edit(request) => source.edit(
                request.number,
                &request.title,
                &request.body,
                &request.add_labels,
                &request.remove_labels,
            ),
        }
    }
}

#[derive(Debug)]
struct MutationSuccess {
    operation: PendingOperation,
    issues: Vec<Issue>,
    action_elapsed: Duration,
    refresh_elapsed: Duration,
}

type MutationReceiver = Receiver<anyhow::Result<MutationSuccess>>;
type InitialLoadReceiver = Receiver<anyhow::Result<InitialLoadSuccess>>;

#[derive(Debug)]
struct InitialLoadSuccess {
    issues: Vec<Issue>,
    labels: Vec<Label>,
    elapsed: Duration,
}

fn event_loop<S: IssueSource>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    source: &S,
    mut initial_load_rx: Option<InitialLoadReceiver>,
) -> anyhow::Result<()> {
    let mut mutation_rx: Option<MutationReceiver> = None;
    let mut mutation_draft: Option<MutationDraft> = None;

    loop {
        if let Some(result) = poll_initial_load(&initial_load_rx) {
            initial_load_rx = None;
            finish_initial_load(state, result);
        }

        if let Some(result) = poll_mutation(&mutation_rx) {
            mutation_rx = None;
            finish_mutation(state, mutation_draft.take(), result);
        }

        terminal.draw(|f| ui::draw(f, state))?;
        state.clear_expired_status();
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != event::KeyEventKind::Press {
                continue;
            }
            if state.is_loading() {
                if map_list_key(key) == ListInput::Quit {
                    return Ok(());
                }
                continue;
            }
            if state.is_pending() {
                match (&state.mode, map_list_key(key)) {
                    (Mode::List, ListInput::Down) => state.move_cursor(1),
                    (Mode::List, ListInput::Up) => state.move_cursor(-1),
                    (Mode::List, ListInput::TogglePane) => state.toggle_pane(),
                    (_, ListInput::Quit) => return Ok(()),
                    _ => {}
                }
                continue;
            }
            match &state.mode {
                Mode::List => match map_list_key(key) {
                    ListInput::Down => state.move_cursor(1),
                    ListInput::Up => state.move_cursor(-1),
                    ListInput::TogglePane => state.toggle_pane(),
                    ListInput::EnterSearch => state.enter_search(),
                    ListInput::CycleStateFilter => {
                        state.cycle_state_filter();
                        refresh(state, source);
                    }
                    ListInput::LittleCreate => state.enter_little_create(),
                    ListInput::BigCreate => state.enter_big_create(),
                    ListInput::Edit => state.enter_edit(),
                    ListInput::RequestClose => state.request_close(),
                    ListInput::CopyReference => copy_selected(state, copy::format_reference),
                    ListInput::CopyMarkdownLink => copy_selected(state, copy::format_markdown_link),
                    ListInput::CopyUrl => copy_selected(state, copy::format_url),
                    ListInput::OpenInBrowser => open_in_browser(state),
                    ListInput::Quit => return Ok(()),
                    ListInput::None => {}
                },
                Mode::Search => match map_search_key(key) {
                    SearchInput::Char(c) => state.search_push(c),
                    SearchInput::Backspace => state.search_backspace(),
                    SearchInput::DeleteWord => state.search_delete_word(),
                    SearchInput::Clear => state.search_clear(),
                    SearchInput::Exit => state.exit_search(),
                    SearchInput::None => {}
                },
                Mode::LittleCreate(_) => match map_little_create_key(key) {
                    LittleCreateInput::Char(c) => state.little_create_push(c),
                    LittleCreateInput::Backspace => state.little_create_backspace(),
                    LittleCreateInput::Submit => {
                        if let Some(title) = state.little_create_submit() {
                            start_mutation(
                                state,
                                &mut mutation_rx,
                                &mut mutation_draft,
                                (*source).clone(),
                                MutationDraft::LittleCreate {
                                    title: title.clone(),
                                },
                                MutationRequest::Create(CreateRequest {
                                    title,
                                    body: String::new(),
                                    labels: Vec::new(),
                                }),
                            );
                        }
                    }
                    LittleCreateInput::Cancel => state.cancel_form_or_create(),
                    LittleCreateInput::None => {}
                },
                Mode::Form(form) => {
                    let field = form.field;
                    let form_draft = form.clone();
                    match map_form_key(key, field) {
                        FormInput::Char(c) => state.form_push_char(c),
                        FormInput::Backspace => state.form_backspace(),
                        FormInput::NextField => state.form_next_field(),
                        FormInput::PrevField => state.form_prev_field(),
                        FormInput::MoveUp => state.form_move_label_cursor(-1),
                        FormInput::MoveDown => state.form_move_label_cursor(1),
                        FormInput::ToggleLabel => state.form_toggle_label(),
                        FormInput::Cancel => state.cancel_form_or_create(),
                        FormInput::Enter => {
                            if let Some(submission) = state.form_enter() {
                                match submission.editing {
                                    Some(number) => start_mutation(
                                        state,
                                        &mut mutation_rx,
                                        &mut mutation_draft,
                                        (*source).clone(),
                                        MutationDraft::Form(form_draft),
                                        MutationRequest::Edit(EditRequest {
                                            number,
                                            title: submission.title,
                                            body: submission.body,
                                            add_labels: submission.add_labels,
                                            remove_labels: submission.remove_labels,
                                        }),
                                    ),
                                    None => start_mutation(
                                        state,
                                        &mut mutation_rx,
                                        &mut mutation_draft,
                                        (*source).clone(),
                                        MutationDraft::Form(form_draft),
                                        MutationRequest::Create(CreateRequest {
                                            title: submission.title,
                                            body: submission.body,
                                            labels: submission.add_labels,
                                        }),
                                    ),
                                }
                            }
                        }
                        FormInput::None => {}
                    }
                }
                Mode::ConfirmClose(_) => match map_confirm_key(key) {
                    ConfirmInput::Yes => {
                        if let Some(number) = state.confirm_close_yes() {
                            let result = source.close(number);
                            apply_result(state, source, result);
                        }
                    }
                    ConfirmInput::No => state.confirm_close_no(),
                    ConfirmInput::None => {}
                },
            }
        }
    }
}

fn spawn_initial_load<S: IssueSource>(source: S) -> InitialLoadReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let started = Instant::now();
        let issues_source = source.clone();
        let labels_source = source;
        let issues_handle = std::thread::spawn(move || issues_source.list(StateFilter::Open));
        let labels_handle = std::thread::spawn(move || labels_source.labels());
        let issues_result = issues_handle
            .join()
            .map_err(|_| anyhow::anyhow!("issue list thread panicked"))
            .and_then(|result| result);
        let result = match issues_result {
            Ok(issues) => match labels_handle.join() {
                Ok(labels_result) => Ok(InitialLoadSuccess {
                    issues,
                    labels: labels_result.unwrap_or_default(),
                    elapsed: started.elapsed(),
                }),
                Err(_) => Err(anyhow::anyhow!("label list thread panicked")),
            },
            Err(e) => Err(e),
        };
        let _ = tx.send(result);
    });
    rx
}

fn poll_initial_load(
    rx: &Option<InitialLoadReceiver>,
) -> Option<anyhow::Result<InitialLoadSuccess>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "initial issue load worker stopped before returning a result"
        ))),
    }
}

fn finish_initial_load(state: &mut AppState, result: anyhow::Result<InitialLoadSuccess>) {
    match result {
        Ok(success) => {
            let count = success.issues.len();
            state.set_loaded(success.issues, success.labels);
            state.set_status(format!(
                "loaded {count} issues in {}",
                format_duration(success.elapsed)
            ));
        }
        Err(e) => {
            state.finish_loading();
            state.set_status(gh_error_status(&e));
        }
    }
}

fn poll_mutation(rx: &Option<MutationReceiver>) -> Option<anyhow::Result<MutationSuccess>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "issue operation worker stopped before returning a result"
        ))),
    }
}

fn start_mutation<S: IssueSource>(
    state: &mut AppState,
    mutation_rx: &mut Option<MutationReceiver>,
    mutation_draft: &mut Option<MutationDraft>,
    source: S,
    draft: MutationDraft,
    request: MutationRequest,
) {
    if state.is_pending() {
        return;
    }
    let operation = request.operation();
    show_pending_draft(state, &draft);
    *mutation_rx = Some(spawn_mutation(source, request, state.state_filter));
    *mutation_draft = Some(draft);
    state.begin_pending(operation);
}

fn spawn_mutation<S: IssueSource>(
    source: S,
    request: MutationRequest,
    state_filter: StateFilter,
) -> MutationReceiver {
    let operation = request.operation();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let action_started = Instant::now();
        let result = request.run(&source).and_then(|()| {
            let action_elapsed = action_started.elapsed();
            let refresh_started = Instant::now();
            let issues = source.list(state_filter)?;
            Ok(MutationSuccess {
                operation,
                issues,
                action_elapsed,
                refresh_elapsed: refresh_started.elapsed(),
            })
        });
        let _ = tx.send(result);
    });
    rx
}

fn finish_mutation(
    state: &mut AppState,
    draft: Option<MutationDraft>,
    result: anyhow::Result<MutationSuccess>,
) {
    state.finish_pending();
    match result {
        Ok(success) => {
            state.mode = Mode::List;
            state.set_issues(success.issues);
            state.set_status(format!(
                "{} in {}, refresh {}",
                success_status_action(success.operation),
                format_duration(success.action_elapsed),
                format_duration(success.refresh_elapsed)
            ));
        }
        Err(e) => {
            restore_mutation_draft(state, draft);
            state.set_status(gh_error_status(&e));
        }
    }
}

fn success_status_action(operation: PendingOperation) -> &'static str {
    match operation {
        PendingOperation::CreateIssue => "created issue",
        PendingOperation::EditIssue => "updated issue",
    }
}

fn restore_mutation_draft(state: &mut AppState, draft: Option<MutationDraft>) {
    if let Some(draft) = draft {
        show_pending_draft(state, &draft);
    }
}

fn show_pending_draft(state: &mut AppState, draft: &MutationDraft) {
    match draft {
        MutationDraft::LittleCreate { title } => state.mode = Mode::LittleCreate(title.clone()),
        MutationDraft::Form(form) => state.mode = Mode::Form(form.clone()),
    }
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        format!("{millis}ms")
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

fn refresh<S: IssueSource>(state: &mut AppState, source: &S) {
    match source.list(state.state_filter) {
        Ok(issues) => state.set_issues(issues),
        Err(e) => state.set_status(gh_error_status(&e)),
    }
}

/// Refresh the issue list on success, or surface the error on the toast line.
/// Used by synchronous close actions.
fn apply_result<S: IssueSource>(state: &mut AppState, source: &S, result: anyhow::Result<()>) {
    match result {
        Ok(()) => refresh(state, source),
        Err(e) => state.set_status(gh_error_status(&e)),
    }
}

fn gh_error_status(error: &anyhow::Error) -> String {
    gh_error_status_with_token_hint(error, std::env::var_os("GITHUB_TOKEN").is_some())
}

fn gh_error_status_with_token_hint(error: &anyhow::Error, github_token_set: bool) -> String {
    let mut message = format!("gh error: {error}");
    if github_token_set && looks_like_auth_or_access_error(&message) {
        message.push_str(" (GITHUB_TOKEN is set; try env -u GITHUB_TOKEN issue-browser)");
    }
    message
}

fn looks_like_auth_or_access_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("resource not accessible")
        || lower.contains("could not resolve")
        || lower.contains("not found")
        || lower.contains("forbidden")
        || lower.contains("permission")
        || lower.contains("authentication")
        || lower.contains("authorization")
        || lower.contains("http 403")
        || lower.contains("http 404")
}

fn copy_selected(state: &mut AppState, format: impl Fn(&model::Issue) -> String) {
    if let Some(issue) = state.selected_issue() {
        let text = format(issue);
        match copy::copy_to_clipboard(&text) {
            Ok(()) => state.set_status(format!("copied: {text}")),
            Err(e) => state.set_status(format!("copy failed: {e}")),
        }
    }
}

fn open_in_browser(state: &mut AppState) {
    if let Some(issue) = state.selected_issue() {
        let url = issue.url.clone();
        match std::process::Command::new("open").arg(&url).status() {
            Ok(_) => state.set_status(format!("opened: {url}")),
            Err(e) => state.set_status(format!("failed to open: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IssueState;

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
    fn finish_initial_load_populates_state_and_reports_timing() {
        let loaded = issue(7, "Loaded issue");
        let label = Label {
            name: "priority".into(),
            color: "faa29b".into(),
        };
        let mut state = AppState::loading();
        finish_initial_load(
            &mut state,
            Ok(InitialLoadSuccess {
                issues: vec![loaded.clone()],
                labels: vec![label.clone()],
                elapsed: Duration::from_millis(350),
            }),
        );
        assert_eq!(state.issues, vec![loaded]);
        assert_eq!(state.all_labels, vec![label]);
        assert!(!state.is_loading());
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("loaded 1 issues in 350ms")
        );
    }

    #[test]
    fn finish_initial_load_failure_clears_loading_and_reports_error() {
        let mut state = AppState::loading();
        finish_initial_load(&mut state, Err(anyhow::anyhow!("repo unavailable")));
        assert!(!state.is_loading());
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("gh error: repo unavailable")
        );
    }

    #[test]
    fn finish_create_success_updates_issues_and_reports_timing() {
        let created = issue(42, "Created issue");
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::CreateIssue);
        finish_mutation(
            &mut state,
            Some(MutationDraft::LittleCreate {
                title: "Draft".into(),
            }),
            Ok(MutationSuccess {
                operation: PendingOperation::CreateIssue,
                issues: vec![created.clone()],
                action_elapsed: Duration::from_millis(1_200),
                refresh_elapsed: Duration::from_millis(50),
            }),
        );
        assert_eq!(state.issues, vec![created]);
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("created issue in 1.2s, refresh 50ms")
        );
    }

    #[test]
    fn finish_edit_success_updates_issues_and_reports_timing() {
        let updated = issue(42, "Updated issue");
        let draft = FormState {
            editing: Some(42),
            title: "Pending edit".into(),
            ..Default::default()
        };
        let mut state = AppState::new(vec![], vec![]);
        state.mode = Mode::Form(draft);
        state.begin_pending(PendingOperation::EditIssue);
        finish_mutation(
            &mut state,
            None,
            Ok(MutationSuccess {
                operation: PendingOperation::EditIssue,
                issues: vec![updated.clone()],
                action_elapsed: Duration::from_millis(950),
                refresh_elapsed: Duration::from_millis(75),
            }),
        );
        assert_eq!(state.issues, vec![updated]);
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("updated issue in 950ms, refresh 75ms")
        );
    }

    #[test]
    fn pending_mutation_keeps_form_draft_visible() {
        let draft = FormState {
            editing: Some(42),
            title: "Pending title".into(),
            body: "Pending body".into(),
            ..Default::default()
        };
        let mut state = AppState::new(vec![], vec![]);
        show_pending_draft(&mut state, &MutationDraft::Form(draft.clone()));
        state.begin_pending(PendingOperation::EditIssue);
        assert_eq!(state.mode, Mode::Form(draft));
        assert!(state
            .pending_message()
            .unwrap()
            .contains("Updating issue..."));
    }

    #[test]
    fn pending_mutation_keeps_little_create_draft_visible() {
        let mut state = AppState::new(vec![], vec![]);
        show_pending_draft(
            &mut state,
            &MutationDraft::LittleCreate {
                title: "Pending title".into(),
            },
        );
        state.begin_pending(PendingOperation::CreateIssue);
        assert_eq!(state.mode, Mode::LittleCreate("Pending title".into()));
        assert!(state
            .pending_message()
            .unwrap()
            .contains("Creating issue..."));
    }

    #[test]
    fn finish_create_failure_restores_little_create_draft() {
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::CreateIssue);
        finish_mutation(
            &mut state,
            Some(MutationDraft::LittleCreate {
                title: "Still typed".into(),
            }),
            Err(anyhow::anyhow!("create failed")),
        );
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::LittleCreate("Still typed".into()));
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("gh error: create failed")
        );
    }

    #[test]
    fn finish_mutation_failure_restores_form_draft() {
        let draft = FormState {
            title: "Draft title".into(),
            body: "Draft body".into(),
            ..Default::default()
        };
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::EditIssue);
        finish_mutation(
            &mut state,
            Some(MutationDraft::Form(draft.clone())),
            Err(anyhow::anyhow!("network failed")),
        );
        assert_eq!(state.mode, Mode::Form(draft));
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("gh error: network failed")
        );
    }

    #[test]
    fn gh_error_status_includes_token_hint_for_access_errors_when_github_token_is_set() {
        let error = anyhow::anyhow!("GraphQL: Resource not accessible by integration");
        assert_eq!(
            gh_error_status_with_token_hint(&error, true),
            "gh error: GraphQL: Resource not accessible by integration (GITHUB_TOKEN is set; try env -u GITHUB_TOKEN issue-browser)"
        );
    }

    #[test]
    fn gh_error_status_omits_token_hint_when_error_is_not_auth_related() {
        let error = anyhow::anyhow!("network failed");
        assert_eq!(
            gh_error_status_with_token_hint(&error, true),
            "gh error: network failed"
        );
    }

    #[test]
    fn gh_error_status_omits_token_hint_when_github_token_is_unset() {
        let error = anyhow::anyhow!("HTTP 403 forbidden");
        assert_eq!(
            gh_error_status_with_token_hint(&error, false),
            "gh error: HTTP 403 forbidden"
        );
    }
}
