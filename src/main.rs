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
use model::{AppState, FormState, Issue, Label, LoadingAnimation, Mode, PendingOperation};
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
  issue-browser                                   Launch the picker (intended via `tmux popup -E`)
  issue-browser --preview-loading [ANIMATION] [DURATION]
                                                  Play a loading animation preview and exit
  issue-browser --doctor                          Print gh, repo, auth, and logging diagnostics
  issue-browser --capture                         Instant title-only capture, then exit
  issue-browser --capture-full                    Full create form (title/body/labels), then exit
  issue-browser --version                         Print version and exit
  issue-browser --help                            Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind i display-popup -E -B -w 84 -h 60% \"exec issue-browser\"";

#[derive(Debug, PartialEq)]
enum StartupCommand {
    Launch,
    Version,
    Help,
    Doctor,
    Capture,
    CaptureFull,
    PreviewLoading {
        animation: Option<LoadingAnimation>,
        duration: Duration,
    },
}

fn parse_command(args: impl IntoIterator<Item = String>) -> Result<StartupCommand, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => Ok(StartupCommand::Launch),
        [arg] if matches!(arg.as_str(), "-V" | "--version") => Ok(StartupCommand::Version),
        [arg] if matches!(arg.as_str(), "-h" | "--help") => Ok(StartupCommand::Help),
        [arg] if arg == "--doctor" => Ok(StartupCommand::Doctor),
        [arg] if arg == "--capture" => Ok(StartupCommand::Capture),
        [arg] if arg == "--capture-full" => Ok(StartupCommand::CaptureFull),
        [arg, rest @ ..] if arg == "--preview-loading" => parse_loading_preview(rest),
        [arg, ..] => Err(format!("unknown argument '{arg}'")),
    }
}

fn parse_loading_preview(args: &[String]) -> Result<StartupCommand, String> {
    const DEFAULT_PREVIEW_DURATION: Duration = Duration::from_secs(5);
    match args {
        [] => Ok(StartupCommand::PreviewLoading {
            animation: None,
            duration: DEFAULT_PREVIEW_DURATION,
        }),
        [single] => {
            if let Some(animation) = LoadingAnimation::parse(single) {
                return Ok(StartupCommand::PreviewLoading {
                    animation: Some(animation),
                    duration: DEFAULT_PREVIEW_DURATION,
                });
            }
            Ok(StartupCommand::PreviewLoading {
                animation: None,
                duration: parse_preview_duration(single)?,
            })
        }
        [animation, duration] => Ok(StartupCommand::PreviewLoading {
            animation: Some(
                LoadingAnimation::parse(animation)
                    .ok_or_else(|| format!("unknown loading animation '{animation}'"))?,
            ),
            duration: parse_preview_duration(duration)?,
        }),
        _ => Err("--preview-loading accepts at most ANIMATION and DURATION".to_string()),
    }
}

fn parse_preview_duration(value: &str) -> Result<Duration, String> {
    let trimmed = value.trim();
    let duration = if let Some(milliseconds) = trimmed.strip_suffix("ms") {
        milliseconds
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| format!("invalid preview duration '{value}'"))?
    } else if let Some(seconds) = trimmed.strip_suffix('s') {
        seconds
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid preview duration '{value}'"))?
    } else {
        trimmed
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid preview duration '{value}'"))?
    };
    if duration.is_zero() {
        Err("preview duration must be greater than zero".to_string())
    } else {
        Ok(duration)
    }
}

fn main() -> anyhow::Result<()> {
    diagnostics::log_event("process_start");
    match parse_command(std::env::args().skip(1)) {
        Ok(StartupCommand::Launch) => {}
        Ok(StartupCommand::Version) => {
            println!("issue-browser {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Ok(StartupCommand::Help) => {
            println!("{HELP}");
            return Ok(());
        }
        Ok(StartupCommand::Doctor) => {
            diagnostics::run_doctor()?;
            return Ok(());
        }
        Ok(StartupCommand::Capture) => {
            let source = GhCliSource::new();
            return run_capture(&source);
        }
        Ok(StartupCommand::CaptureFull) => {
            let source = GhCliSource::new();
            return run_capture_full(&source);
        }
        Ok(StartupCommand::PreviewLoading {
            animation,
            duration,
        }) => return run_loading_preview(animation, duration),
        Err(message) => {
            eprintln!("issue-browser: {message}\n\n{HELP}");
            std::process::exit(2);
        }
    }

    let source = GhCliSource::new();
    let mut state = AppState::loading();
    run_ui(&mut state, &source)
}

fn check_gh_cli() -> Result<(), String> {
    if std::process::Command::new("gh")
        .arg("--version")
        .output()
        .is_err()
    {
        return Err("`gh` CLI not found on PATH. Install it from https://cli.github.com and run `gh auth login`.".to_string());
    }
    let authenticated = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !authenticated {
        return Err("`gh` is not authenticated. Run `gh auth login` first.".to_string());
    }
    Ok(())
}

fn spawn_preflight_check() -> PreflightReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(check_gh_cli());
    });
    rx
}

fn poll_preflight(rx: &Option<PreflightReceiver>) -> Option<Result<(), String>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(
            "preflight check worker stopped before returning a result".to_string(),
        )),
    }
}

fn run_ui<S: IssueSource>(state: &mut AppState, source: &S) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    diagnostics::log_event("terminal_ready");

    let preflight_rx = spawn_preflight_check();
    let result = event_loop(&mut terminal, state, source, None, Some(preflight_rx));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loading_preview(
    animation: Option<LoadingAnimation>,
    duration: Duration,
) -> anyhow::Result<()> {
    let mut state = AppState::loading();
    if let Some(animation) = animation {
        if let Some(loading) = state.loading.as_mut() {
            loading.animation = animation;
        }
    }

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let started = Instant::now();
    let result = loop {
        terminal.draw(|f| ui::draw(f, &state))?;
        if started.elapsed() >= duration {
            break Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

type CreateReceiver = Receiver<anyhow::Result<()>>;

fn spawn_create<S: IssueSource>(
    source: S,
    title: String,
    body: String,
    labels: Vec<String>,
) -> CreateReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(source.create(&title, &body, &labels));
    });
    rx
}

fn poll_create(rx: &Option<CreateReceiver>) -> Option<anyhow::Result<()>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "create worker stopped before returning a result"
        ))),
    }
}

fn run_capture<S: IssueSource>(source: &S) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let mut state = AppState::new(vec![], vec![]);
    state.enter_little_create();

    let result = capture_loop(&mut terminal, &mut state, source);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn capture_loop<S: IssueSource>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    source: &S,
) -> anyhow::Result<()> {
    let mut create_rx: Option<CreateReceiver> = None;

    loop {
        if let Some(result) = poll_create(&create_rx) {
            create_rx = None;
            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    state.finish_pending();
                    state.set_status(gh_error_status(&e));
                }
            }
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
            if state.is_pending() {
                if map_list_key(key) == ListInput::Quit {
                    return Ok(());
                }
                continue;
            }
            match map_little_create_key(key) {
                LittleCreateInput::Char(c) => state.little_create_push(c),
                LittleCreateInput::Backspace => state.little_create_backspace(),
                LittleCreateInput::Submit => {
                    if let Some(title) = state.little_create_submit() {
                        state.mode = Mode::LittleCreate(title.clone());
                        state.begin_pending(PendingOperation::CreateIssue);
                        create_rx = Some(spawn_create(
                            (*source).clone(),
                            title,
                            String::new(),
                            Vec::new(),
                        ));
                    }
                }
                LittleCreateInput::Cancel => return Ok(()),
                LittleCreateInput::None => {}
            }
        }
    }
}

type LabelsReceiver = Receiver<anyhow::Result<Vec<Label>>>;

fn spawn_labels_fetch<S: IssueSource>(source: S) -> LabelsReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(source.labels());
    });
    rx
}

fn poll_labels(rx: &Option<LabelsReceiver>) -> Option<anyhow::Result<Vec<Label>>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "label fetch worker stopped before returning a result"
        ))),
    }
}

fn run_capture_full<S: IssueSource>(source: &S) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let mut state = AppState::new(vec![], vec![]);
    state.begin_loading("labels");
    let labels_rx = spawn_labels_fetch(source.clone());

    let result = capture_full_loop(&mut terminal, &mut state, source, Some(labels_rx));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn begin_full_create<S: IssueSource>(
    state: &mut AppState,
    create_rx: &mut Option<CreateReceiver>,
    source: S,
    form_draft: FormState,
    submission: crate::model::FormSubmission,
) {
    state.mode = Mode::Form(Box::new(form_draft));
    state.begin_pending(PendingOperation::CreateIssue);
    *create_rx = Some(spawn_create(
        source,
        submission.title,
        submission.body,
        submission.add_labels,
    ));
}

fn capture_full_loop<S: IssueSource>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    source: &S,
    mut labels_rx: Option<LabelsReceiver>,
) -> anyhow::Result<()> {
    let mut create_rx: Option<CreateReceiver> = None;

    loop {
        if let Some(result) = poll_labels(&labels_rx) {
            labels_rx = None;
            match result {
                Ok(labels) => {
                    state.all_labels = labels;
                    state.finish_loading();
                    state.enter_big_create();
                }
                Err(e) => return Err(e),
            }
        }

        if let Some(result) = poll_create(&create_rx) {
            create_rx = None;
            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    state.finish_pending();
                    state.set_status(gh_error_status(&e));
                }
            }
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
                if map_list_key(key) == ListInput::Quit {
                    return Ok(());
                }
                continue;
            }
            match &state.mode {
                Mode::Form(form) => {
                    let field = form.field;
                    let form_draft = (**form).clone();
                    match map_form_key(key, field) {
                        FormInput::TextEdit(input) => state.form_input(input),
                        FormInput::NextField => state.form_next_field(),
                        FormInput::PrevField => state.form_prev_field(),
                        FormInput::MoveUp => state.form_move_label_cursor(-1),
                        FormInput::MoveDown => state.form_move_label_cursor(1),
                        FormInput::ToggleLabel => state.form_toggle_label(),
                        FormInput::Cancel => state.cancel_form_or_create(),
                        FormInput::Enter => {
                            if let Some(submission) = state.form_enter() {
                                begin_full_create(
                                    state,
                                    &mut create_rx,
                                    (*source).clone(),
                                    form_draft,
                                    submission,
                                );
                            }
                        }
                        FormInput::SubmitNow => {
                            if let Some(submission) = state.form_submit_now() {
                                begin_full_create(
                                    state,
                                    &mut create_rx,
                                    (*source).clone(),
                                    form_draft,
                                    submission,
                                );
                            }
                        }
                        FormInput::None => {}
                    }
                }
                Mode::ConfirmDiscard(_) => match map_confirm_key(key) {
                    ConfirmInput::Yes => state.confirm_discard_yes(),
                    ConfirmInput::No => state.confirm_discard_no(),
                    ConfirmInput::None => {}
                },
                _ => {}
            }
            if state.mode == Mode::List {
                return Ok(());
            }
        }
    }
}

#[derive(Debug, Clone)]
enum MutationDraft {
    LittleCreate { title: String },
    Form(Box<FormState>),
    None,
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
    Close(u32),
}

impl MutationRequest {
    fn operation(&self) -> PendingOperation {
        match self {
            MutationRequest::Create(_) => PendingOperation::CreateIssue,
            MutationRequest::Edit(_) => PendingOperation::EditIssue,
            MutationRequest::Close(_) => PendingOperation::CloseIssue,
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
            MutationRequest::Close(number) => source.close(number),
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
type PreflightReceiver = Receiver<Result<(), String>>;

#[derive(Debug)]
struct InitialLoadSuccess {
    issues: Vec<Issue>,
    labels: Vec<Label>,
    repo_name: Option<String>,
    elapsed: Duration,
}

fn event_loop<S: IssueSource>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    source: &S,
    mut initial_load_rx: Option<InitialLoadReceiver>,
    mut preflight_rx: Option<PreflightReceiver>,
) -> anyhow::Result<()> {
    let mut mutation_rx: Option<MutationReceiver> = None;
    let mut mutation_draft: Option<MutationDraft> = None;
    let mut refresh_rx: Option<RefreshReceiver> = None;
    let mut first_draw_logged = false;

    loop {
        if let Some(result) = poll_preflight(&preflight_rx) {
            preflight_rx = None;
            match result {
                Ok(()) => initial_load_rx = Some(spawn_initial_load(source.clone())),
                Err(message) => return Err(anyhow::anyhow!(message)),
            }
        }

        if let Some(result) = poll_initial_load(&initial_load_rx) {
            initial_load_rx = None;
            finish_initial_load(state, result);
        }

        if let Some(result) = poll_mutation(&mutation_rx) {
            mutation_rx = None;
            finish_mutation(state, mutation_draft.take(), result);
        }

        if let Some(result) = poll_refresh(&refresh_rx) {
            refresh_rx = None;
            finish_refresh(state, result);
        }

        terminal.draw(|f| ui::draw(f, state))?;
        if !first_draw_logged {
            diagnostics::log_event("first_draw_complete");
            first_draw_logged = true;
        }
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
                        start_refresh(state, &mut refresh_rx, (*source).clone());
                    }
                    ListInput::LittleCreate => state.enter_little_create(),
                    ListInput::BigCreate => state.enter_big_create(),
                    ListInput::Edit => state.enter_edit(),
                    ListInput::RequestClose => state.request_close(),
                    ListInput::CopyReference => copy_selected(state, copy::format_reference),
                    ListInput::CopyMarkdownLink => copy_selected(state, copy::format_markdown_link),
                    ListInput::CopyUrl => copy_selected(state, copy::format_url),
                    ListInput::OpenInBrowser => open_in_browser(state),
                    ListInput::Refresh => start_refresh(state, &mut refresh_rx, (*source).clone()),
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
                    let form_draft = (**form).clone();
                    match map_form_key(key, field) {
                        FormInput::TextEdit(input) => state.form_input(input),
                        FormInput::NextField => state.form_next_field(),
                        FormInput::PrevField => state.form_prev_field(),
                        FormInput::MoveUp => state.form_move_label_cursor(-1),
                        FormInput::MoveDown => state.form_move_label_cursor(1),
                        FormInput::ToggleLabel => state.form_toggle_label(),
                        FormInput::Cancel => state.cancel_form_or_create(),
                        FormInput::Enter => {
                            if let Some(submission) = state.form_enter() {
                                submit_form(
                                    state,
                                    &mut mutation_rx,
                                    &mut mutation_draft,
                                    (*source).clone(),
                                    form_draft,
                                    submission,
                                );
                            }
                        }
                        FormInput::SubmitNow => {
                            if let Some(submission) = state.form_submit_now() {
                                submit_form(
                                    state,
                                    &mut mutation_rx,
                                    &mut mutation_draft,
                                    (*source).clone(),
                                    form_draft,
                                    submission,
                                );
                            }
                        }
                        FormInput::None => {}
                    }
                }
                Mode::ConfirmClose(_) => match map_confirm_key(key) {
                    ConfirmInput::Yes => {
                        if let Some(number) = state.confirm_close_yes() {
                            start_mutation(
                                state,
                                &mut mutation_rx,
                                &mut mutation_draft,
                                (*source).clone(),
                                MutationDraft::None,
                                MutationRequest::Close(number),
                            );
                        }
                    }
                    ConfirmInput::No => state.confirm_close_no(),
                    ConfirmInput::None => {}
                },
                Mode::ConfirmDiscard(_) => match map_confirm_key(key) {
                    ConfirmInput::Yes => state.confirm_discard_yes(),
                    ConfirmInput::No => state.confirm_discard_no(),
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
        let labels_source = source.clone();
        let repo_name_source = source;
        let issues_handle = std::thread::spawn(move || issues_source.list(StateFilter::Open));
        let labels_handle = std::thread::spawn(move || labels_source.labels());
        let repo_name_handle = std::thread::spawn(move || repo_name_source.repo_name());
        let issues_result = issues_handle
            .join()
            .map_err(|_| anyhow::anyhow!("issue list thread panicked"))
            .and_then(|result| result)
            .map_err(diagnose_initial_load_error);
        let result = match issues_result {
            Ok(issues) => match labels_handle.join() {
                Ok(labels_result) => Ok(InitialLoadSuccess {
                    issues,
                    labels: labels_result.unwrap_or_default(),
                    repo_name: repo_name_handle.join().ok().and_then(|r| r.ok()),
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

fn diagnose_initial_load_error(e: anyhow::Error) -> anyhow::Error {
    let message = e.to_string();
    if looks_like_auth_or_access_error(&message) {
        anyhow::anyhow!(refine_auth_error(message, probe_auth_status()))
    } else {
        e
    }
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
            state.repo_name_with_owner = success.repo_name.clone();
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

#[derive(Debug)]
struct RefreshSuccess {
    issues: Vec<Issue>,
    labels: Vec<Label>,
    elapsed: Duration,
}

type RefreshReceiver = Receiver<anyhow::Result<RefreshSuccess>>;

fn spawn_refresh<S: IssueSource>(source: S, state_filter: StateFilter) -> RefreshReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let started = Instant::now();
        let issues_source = source.clone();
        let labels_source = source;
        let issues_handle = std::thread::spawn(move || issues_source.list(state_filter));
        let labels_handle = std::thread::spawn(move || labels_source.labels());
        let issues_result = issues_handle
            .join()
            .map_err(|_| anyhow::anyhow!("issue list thread panicked"))
            .and_then(|result| result);
        let result = match issues_result {
            Ok(issues) => match labels_handle.join() {
                Ok(labels_result) => Ok(RefreshSuccess {
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

fn poll_refresh(rx: &Option<RefreshReceiver>) -> Option<anyhow::Result<RefreshSuccess>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "refresh worker stopped before returning a result"
        ))),
    }
}

fn start_refresh<S: IssueSource>(
    state: &mut AppState,
    refresh_rx: &mut Option<RefreshReceiver>,
    source: S,
) {
    if state.is_pending() {
        return;
    }
    *refresh_rx = Some(spawn_refresh(source, state.state_filter));
    state.begin_pending(PendingOperation::RefreshList);
}

fn finish_refresh(state: &mut AppState, result: anyhow::Result<RefreshSuccess>) {
    state.finish_pending();
    match result {
        Ok(success) => {
            let count = success.issues.len();
            state.all_labels = success.labels;
            state.set_issues(success.issues);
            state.set_status(format!(
                "refreshed {count} issues in {}",
                format_duration(success.elapsed)
            ));
        }
        Err(e) => state.set_status(gh_error_status(&e)),
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
        PendingOperation::CloseIssue => "closed issue",
        PendingOperation::RefreshList => {
            unreachable!("RefreshList never flows through the mutation success path")
        }
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
        MutationDraft::None => {}
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

fn submit_form<S: IssueSource>(
    state: &mut AppState,
    mutation_rx: &mut Option<MutationReceiver>,
    mutation_draft: &mut Option<MutationDraft>,
    source: S,
    form_draft: FormState,
    submission: crate::model::FormSubmission,
) {
    match submission.editing {
        Some(number) => start_mutation(
            state,
            mutation_rx,
            mutation_draft,
            source,
            MutationDraft::Form(Box::new(form_draft)),
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
            mutation_rx,
            mutation_draft,
            source,
            MutationDraft::Form(Box::new(form_draft)),
            MutationRequest::Create(CreateRequest {
                title: submission.title,
                body: submission.body,
                labels: submission.add_labels,
            }),
        ),
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

fn refine_auth_error(message: String, auth_status_ok: bool) -> String {
    if auth_status_ok || !looks_like_auth_or_access_error(&message) {
        message
    } else {
        format!("{message} (and `gh auth status` failed — run `gh auth login`)")
    }
}

fn probe_auth_status() -> bool {
    std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn parse_command_defaults_to_launch() {
        assert_eq!(parse_command(args(&[])), Ok(StartupCommand::Launch));
    }

    #[test]
    fn parse_command_accepts_loading_preview_default() {
        assert_eq!(
            parse_command(args(&["--preview-loading"])),
            Ok(StartupCommand::PreviewLoading {
                animation: None,
                duration: Duration::from_secs(5),
            })
        );
    }

    #[test]
    fn parse_command_accepts_capture() {
        assert_eq!(
            parse_command(args(&["--capture"])),
            Ok(StartupCommand::Capture)
        );
    }

    #[test]
    fn parse_command_accepts_capture_full() {
        assert_eq!(
            parse_command(args(&["--capture-full"])),
            Ok(StartupCommand::CaptureFull)
        );
    }

    #[test]
    fn parse_command_accepts_loading_preview_animation_and_duration() {
        assert_eq!(
            parse_command(args(&["--preview-loading", "ripple", "250ms"])),
            Ok(StartupCommand::PreviewLoading {
                animation: Some(LoadingAnimation::ColorRipple),
                duration: Duration::from_millis(250),
            })
        );
    }

    #[test]
    fn parse_command_accepts_loading_preview_duration_only() {
        assert_eq!(
            parse_command(args(&["--preview-loading", "2s"])),
            Ok(StartupCommand::PreviewLoading {
                animation: None,
                duration: Duration::from_secs(2),
            })
        );
    }

    #[test]
    fn parse_command_rejects_zero_loading_preview_duration() {
        assert_eq!(
            parse_command(args(&["--preview-loading", "0"])),
            Err("preview duration must be greater than zero".to_string())
        );
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
                repo_name: None,
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
    fn finish_refresh_success_updates_issues_labels_and_reports_timing() {
        let refreshed = issue(9, "Refreshed issue");
        let label = Label {
            name: "priority".into(),
            color: "faa29b".into(),
        };
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::RefreshList);
        finish_refresh(
            &mut state,
            Ok(RefreshSuccess {
                issues: vec![refreshed.clone()],
                labels: vec![label.clone()],
                elapsed: Duration::from_millis(200),
            }),
        );
        assert!(!state.is_pending());
        assert_eq!(state.issues, vec![refreshed]);
        assert_eq!(state.all_labels, vec![label]);
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("refreshed 1 issues in 200ms")
        );
    }

    #[test]
    fn finish_refresh_failure_clears_pending_and_reports_error() {
        let mut state = AppState::new(vec![issue(1, "Existing issue")], vec![]);
        state.begin_pending(PendingOperation::RefreshList);
        finish_refresh(&mut state, Err(anyhow::anyhow!("network unreachable")));
        assert!(!state.is_pending());
        assert_eq!(state.issues, vec![issue(1, "Existing issue")]);
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("gh error: network unreachable")
        );
    }

    #[test]
    fn finish_initial_load_sets_repo_name_when_available() {
        let mut state = AppState::loading();
        finish_initial_load(
            &mut state,
            Ok(InitialLoadSuccess {
                issues: vec![],
                labels: vec![],
                repo_name: Some("jeffdt/issue-browser".to_string()),
                elapsed: Duration::from_millis(10),
            }),
        );
        assert_eq!(
            state.repo_name_with_owner,
            Some("jeffdt/issue-browser".to_string())
        );
    }

    #[test]
    fn finish_initial_load_leaves_repo_name_none_when_unavailable() {
        let mut state = AppState::loading();
        finish_initial_load(
            &mut state,
            Ok(InitialLoadSuccess {
                issues: vec![],
                labels: vec![],
                repo_name: None,
                elapsed: Duration::from_millis(10),
            }),
        );
        assert_eq!(state.repo_name_with_owner, None);
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
            ..FormState::with_title_body("Pending edit", "")
        };
        let mut state = AppState::new(vec![], vec![]);
        state.mode = Mode::Form(Box::new(draft));
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
    fn finish_close_success_updates_issues_and_reports_timing() {
        let remaining = issue(7, "Still open");
        let mut state = AppState::new(vec![issue(42, "To be closed"), remaining.clone()], vec![]);
        state.begin_pending(PendingOperation::CloseIssue);
        finish_mutation(
            &mut state,
            Some(MutationDraft::None),
            Ok(MutationSuccess {
                operation: PendingOperation::CloseIssue,
                issues: vec![remaining.clone()],
                action_elapsed: Duration::from_millis(400),
                refresh_elapsed: Duration::from_millis(30),
            }),
        );
        assert_eq!(state.issues, vec![remaining]);
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("closed issue in 400ms, refresh 30ms")
        );
    }

    #[test]
    fn finish_close_failure_leaves_list_mode_and_reports_error() {
        let mut state = AppState::new(vec![issue(42, "To be closed")], vec![]);
        state.begin_pending(PendingOperation::CloseIssue);
        finish_mutation(
            &mut state,
            Some(MutationDraft::None),
            Err(anyhow::anyhow!("close failed")),
        );
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_eq!(
            state.status.as_ref().map(|(msg, _)| msg.as_str()),
            Some("gh error: close failed")
        );
    }

    #[test]
    fn pending_mutation_keeps_form_draft_visible() {
        let draft = FormState {
            editing: Some(42),
            ..FormState::with_title_body("Pending title", "Pending body")
        };
        let mut state = AppState::new(vec![], vec![]);
        show_pending_draft(&mut state, &MutationDraft::Form(Box::new(draft.clone())));
        state.begin_pending(PendingOperation::EditIssue);
        assert_eq!(state.mode, Mode::Form(Box::new(draft)));
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
        let draft = FormState::with_title_body("Draft title", "Draft body");
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::EditIssue);
        finish_mutation(
            &mut state,
            Some(MutationDraft::Form(Box::new(draft.clone()))),
            Err(anyhow::anyhow!("network failed")),
        );
        assert_eq!(state.mode, Mode::Form(Box::new(draft)));
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

    #[test]
    fn refine_auth_error_appends_hint_when_auth_status_failed_and_error_looks_auth_shaped() {
        let message = "gh error: HTTP 403 forbidden".to_string();
        assert_eq!(
            refine_auth_error(message, false),
            "gh error: HTTP 403 forbidden (and `gh auth status` failed — run `gh auth login`)"
        );
    }

    #[test]
    fn refine_auth_error_leaves_message_untouched_when_auth_status_succeeded() {
        let message = "gh error: HTTP 403 forbidden".to_string();
        assert_eq!(refine_auth_error(message.clone(), true), message);
    }

    #[test]
    fn refine_auth_error_leaves_message_untouched_when_error_is_not_auth_shaped() {
        let message = "gh error: network unreachable".to_string();
        assert_eq!(refine_auth_error(message.clone(), false), message);
    }
}
