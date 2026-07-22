mod config;
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
    map_confirm_key, map_form_key, map_label_picker_key, map_list_key, map_little_create_key,
    map_search_key, map_settings_key, ConfirmInput, FormInput, LabelPickerInput, ListInput,
    LittleCreateInput, RepoPickerInput, SearchInput, SettingsInput,
};

const HELP: &str = "\
boomerang - a tmux-popup TUI for GitHub issues

Usage:
  boomerang                                   Launch the picker (intended via `tmux popup -E`)
  boomerang OWNER/REPO                        Launch targeting a specific repo, regardless of cwd
  boomerang --repo OWNER/REPO                 Same, via an explicit flag (also accepts a github.com URL)
  boomerang --preview-loading [ANIMATION] [DURATION]
                                              Play a loading animation preview and exit
  boomerang --doctor                          Print gh, repo, auth, and logging diagnostics
  boomerang --capture                         Instant title-only capture, then exit
  boomerang --capture-full                    Full create form (title/body/labels), then exit
  boomerang --version                         Print version and exit
  boomerang --help                            Print this help and exit

Launching outside a git repo (and without OWNER/REPO) opens the repo picker
directly; press R from the issue list to switch repos at any time.

Bind it in ~/.tmux.conf, e.g.:
  bind i display-popup -E -B -w 84 -h 60% \"exec boomerang\"";

#[derive(Debug, PartialEq)]
enum StartupCommand {
    Launch {
        repo: Option<String>,
    },
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
        [] => Ok(StartupCommand::Launch { repo: None }),
        [arg] if matches!(arg.as_str(), "-V" | "--version") => Ok(StartupCommand::Version),
        [arg] if matches!(arg.as_str(), "-h" | "--help") => Ok(StartupCommand::Help),
        [arg] if arg == "--doctor" => Ok(StartupCommand::Doctor),
        [arg] if arg == "--capture" => Ok(StartupCommand::Capture),
        [arg] if arg == "--capture-full" => Ok(StartupCommand::CaptureFull),
        [arg, rest @ ..] if arg == "--preview-loading" => parse_loading_preview(rest),
        [flag, repo] if flag == "--repo" => gh::parse_repo_spec(repo)
            .map(|repo| StartupCommand::Launch { repo: Some(repo) })
            .ok_or_else(|| {
                format!("'{repo}' doesn't look like a GitHub repo (expected OWNER/REPO or a github.com URL)")
            }),
        [arg] if arg == "--repo" => Err("--repo requires an OWNER/REPO argument".to_string()),
        [arg] if !arg.starts_with('-') => gh::parse_repo_spec(arg)
            .map(|repo| StartupCommand::Launch { repo: Some(repo) })
            .ok_or_else(|| format!("unknown argument '{arg}'")),
        [arg, ..] => Err(format!("unknown argument '{arg}'")),
    }
}

/// Whether the current working directory is inside a git work tree. Checked
/// with `git` directly (not `gh`) so a non-repo directory is detected
/// instantly and deterministically, without depending on how `gh`'s error
/// text happens to be worded.
fn inside_git_work_tree() -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
        .unwrap_or(false)
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
    let cli_repo = match parse_command(std::env::args().skip(1)) {
        Ok(StartupCommand::Launch { repo }) => repo,
        Ok(StartupCommand::Version) => {
            println!("boomerang {}", env!("CARGO_PKG_VERSION"));
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
            eprintln!("boomerang: {message}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    let config_path = config::config_path();
    let mut loaded_config = config::Config::load_from(&config_path);
    if let Some(repo) = &cli_repo {
        loaded_config.remember_repo(repo);
        let _ = loaded_config.save_to(&config_path);
    }

    let source = match &cli_repo {
        Some(repo) => GhCliSource::with_repo(repo.clone()),
        None => GhCliSource::new(),
    };

    // A repo passed on the CLI always has somewhere to point `gh -R` at, so
    // it never needs the cwd fallback. Otherwise, launching outside a git
    // repo would otherwise leave `gh` with nothing to auto-detect and no
    // issues to show, so go straight to the picker instead (issue #20).
    let has_repo_context = cli_repo.is_some() || inside_git_work_tree();
    let mut state = if has_repo_context {
        AppState::loading()
    } else {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_repo_picker(loaded_config.recent_repos.clone(), false);
        state
    };
    state.exit_on_copy_yank = loaded_config.exit_on_copy_yank;
    state.zebra_striping = loaded_config.zebra_striping;
    state.shortcuts_on_demand = loaded_config.shortcuts_on_demand;
    state.accent_color = loaded_config.accent_color.clone();

    run_ui(&mut state, &source, &config_path, has_repo_context)
}

fn run_ui<S: IssueSource>(
    state: &mut AppState,
    source: &S,
    config_path: &std::path::Path,
    has_repo_context: bool,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    diagnostics::log_event("terminal_ready");

    let initial_load_rx = has_repo_context.then(|| spawn_initial_load(source.clone()));
    let result = event_loop(&mut terminal, state, source, initial_load_rx, config_path);

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
        let _ = tx.send(source.create(&title, &body, &labels).map(|_| ()));
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
    let repo_name_rx = Some(spawn_repo_name_fetch(source.clone()));

    let result = capture_loop(&mut terminal, &mut state, source, repo_name_rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn capture_loop<S: IssueSource>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    source: &S,
    mut repo_name_rx: Option<RepoNameReceiver>,
) -> anyhow::Result<()> {
    let mut create_rx: Option<CreateReceiver> = None;

    loop {
        if let Some(result) = poll_repo_name(&repo_name_rx) {
            repo_name_rx = None;
            apply_repo_name_result(state, result);
        }

        if let Some(result) = poll_create(&create_rx) {
            create_rx = None;
            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    state.finish_pending();
                    state.set_status_error(gh_error_status(&e));
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

type RepoNameReceiver = Receiver<anyhow::Result<String>>;

fn spawn_repo_name_fetch<S: IssueSource>(source: S) -> RepoNameReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(source.repo_name());
    });
    rx
}

fn poll_repo_name(rx: &Option<RepoNameReceiver>) -> Option<anyhow::Result<String>> {
    match rx.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "repo name fetch worker stopped before returning a result"
        ))),
    }
}

fn apply_repo_name_result(state: &mut AppState, result: anyhow::Result<String>) {
    if let Ok(name) = result {
        state.repo_name_with_owner = Some(name);
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
            let labels = result?;
            state.all_labels = labels;
            state.finish_loading();
            state.enter_big_create();
        }

        if let Some(result) = poll_create(&create_rx) {
            create_rx = None;
            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    state.finish_pending();
                    state.set_status_error(gh_error_status(&e));
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

    /// Runs the mutation against `source`, returning the issue number to
    /// keep selected and flash once the list refreshes, if any: the newly
    /// created issue for `Create`, the edited issue for `Edit`, and `None`
    /// for `Close` (the closed issue drops out of the default Open-filtered
    /// view, so there's nothing left to keep selected or flash).
    fn run<S: IssueSource>(self, source: &S) -> anyhow::Result<Option<u32>> {
        match self {
            MutationRequest::Create(request) => source
                .create(&request.title, &request.body, &request.labels)
                .map(Some),
            MutationRequest::Edit(request) => {
                source.edit(
                    request.number,
                    &request.title,
                    &request.body,
                    &request.add_labels,
                    &request.remove_labels,
                )?;
                Ok(Some(request.number))
            }
            MutationRequest::Close(number) => {
                source.close(number)?;
                Ok(None)
            }
        }
    }
}

#[derive(Debug)]
struct MutationSuccess {
    operation: PendingOperation,
    issues: Vec<Issue>,
    action_elapsed: Duration,
    refresh_elapsed: Duration,
    target_issue: Option<u32>,
}

type MutationReceiver = Receiver<anyhow::Result<MutationSuccess>>;
type InitialLoadReceiver = Receiver<anyhow::Result<InitialLoadSuccess>>;

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
    config_path: &std::path::Path,
) -> anyhow::Result<()> {
    let mut mutation_rx: Option<MutationReceiver> = None;
    let mut mutation_draft: Option<MutationDraft> = None;
    let mut refresh_rx: Option<RefreshReceiver> = None;
    let mut first_draw_logged = false;

    loop {
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
        state.clear_expired_flash();
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
                    }
                    ListInput::LabelFilter => state.enter_label_picker(),
                    ListInput::BigCreate => state.enter_big_create(),
                    ListInput::Edit => state.enter_edit(),
                    ListInput::RequestClose => state.request_close(),
                    ListInput::ToggleCheck => state.toggle_check(),
                    ListInput::CopyReference => {
                        if copy_selected(state, copy::format_reference, copy::copy_to_clipboard) {
                            return Ok(());
                        }
                    }
                    ListInput::CopyMarkdownLink => {
                        if copy_selected(state, copy::format_markdown_link, copy::copy_to_clipboard)
                        {
                            return Ok(());
                        }
                    }
                    ListInput::CopyUrl => {
                        if copy_selected(state, copy::format_url, copy::copy_to_clipboard) {
                            return Ok(());
                        }
                    }
                    ListInput::OpenInBrowser => open_in_browser(state),
                    ListInput::Refresh => start_refresh(state, &mut refresh_rx, (*source).clone()),
                    ListInput::EnterSettings => state.enter_settings(),
                    ListInput::SwitchRepo => {
                        let recent = config::Config::load_from(config_path).recent_repos;
                        state.enter_repo_picker(recent, true);
                    }
                    ListInput::ToggleShortcuts => state.toggle_shortcuts(),
                    ListInput::Quit => {
                        if handle_quit(state) {
                            return Ok(());
                        }
                    }
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
                // Unreachable in the full popup: nothing in ListInput enters
                // Mode::LittleCreate here anymore. The variant itself stays
                // on Mode because the standalone `--capture` flow still uses
                // it via its own event loop (capture_loop), not this one.
                Mode::LittleCreate(_) => {}
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
                Mode::Settings => match map_settings_key(key) {
                    SettingsInput::Down => state.settings_move_cursor(1),
                    SettingsInput::Up => state.settings_move_cursor(-1),
                    SettingsInput::Toggle => {
                        state.settings_toggle();
                        let mut cfg = config::Config::load_from(config_path);
                        cfg.exit_on_copy_yank = state.exit_on_copy_yank;
                        cfg.zebra_striping = state.zebra_striping;
                        cfg.shortcuts_on_demand = state.shortcuts_on_demand;
                        cfg.accent_color = state.accent_color.clone();
                        let _ = cfg.save_to(config_path);
                    }
                    SettingsInput::Exit => state.exit_settings(),
                    SettingsInput::None => {}
                },
                Mode::RepoPicker(_) => match ui::map_repo_picker_key(key) {
                    RepoPickerInput::Char(c) => state.repo_picker_push(c),
                    RepoPickerInput::Backspace => state.repo_picker_backspace(),
                    RepoPickerInput::Up => state.repo_picker_move(-1),
                    RepoPickerInput::Down => state.repo_picker_move(1),
                    RepoPickerInput::Cancel => {
                        if state.repo_picker_cancel() {
                            return Ok(());
                        }
                    }
                    RepoPickerInput::Submit => {
                        if let Some(repo) = state.repo_picker_submit() {
                            switch_repo(state, source, config_path, &mut initial_load_rx, repo);
                        }
                    }
                    RepoPickerInput::None => {}
                },
                Mode::LabelPicker(_) => match map_label_picker_key(key) {
                    LabelPickerInput::Down => state.label_picker_move(1),
                    LabelPickerInput::Up => state.label_picker_move(-1),
                    LabelPickerInput::Select => state.label_picker_select(),
                    LabelPickerInput::Cancel => state.label_picker_cancel(),
                    LabelPickerInput::None => {}
                },
            }
        }
    }
}

fn merge_issue_lists(open: Vec<Issue>, closed: Vec<Issue>) -> Vec<Issue> {
    let mut issues = open;
    issues.extend(closed);
    issues
}

fn join_issue_list_handle(
    handle: std::thread::JoinHandle<anyhow::Result<Vec<Issue>>>,
    bucket: &str,
) -> anyhow::Result<Vec<Issue>> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("{bucket} issue list thread panicked"))
        .and_then(|result| result)
}

/// Fetches open and closed issues in parallel and merges them into one list.
/// Blocks until both fetches complete, so callers that also need other data
/// (labels, repo name) should spawn those threads first and call this after,
/// to keep everything running concurrently.
fn fetch_open_and_closed<S: IssueSource>(source: S) -> anyhow::Result<Vec<Issue>> {
    let open_source = source.clone();
    let closed_source = source;
    let open_handle = std::thread::spawn(move || open_source.list(StateFilter::Open));
    let closed_handle = std::thread::spawn(move || closed_source.list(StateFilter::Closed));
    let open_result = join_issue_list_handle(open_handle, "open");
    let closed_result = join_issue_list_handle(closed_handle, "closed");
    open_result.and_then(|open| closed_result.map(|closed| merge_issue_lists(open, closed)))
}

fn spawn_initial_load<S: IssueSource>(source: S) -> InitialLoadReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let started = Instant::now();
        let issues_source = source.clone();
        let labels_source = source.clone();
        let repo_name_source = source;
        let labels_handle = std::thread::spawn(move || labels_source.labels());
        let repo_name_handle = std::thread::spawn(move || repo_name_source.repo_name());
        let issues_result =
            fetch_open_and_closed(issues_source).map_err(diagnose_initial_load_error);
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

/// Retarget `source` at `repo`, remember it in the persisted recent-repos
/// list, and reset `state` back to a fresh loading screen for it. Mirrors
/// what a relaunch with `boomerang OWNER/REPO` would do, without actually
/// restarting the process.
fn switch_repo<S: IssueSource>(
    state: &mut AppState,
    source: &S,
    config_path: &std::path::Path,
    initial_load_rx: &mut Option<InitialLoadReceiver>,
    repo: String,
) {
    source.set_repo(Some(repo.clone()));
    let mut cfg = config::Config::load_from(config_path);
    cfg.remember_repo(&repo);
    let _ = cfg.save_to(config_path);

    state.issues = Vec::new();
    state.all_labels = Vec::new();
    state.state_filter = StateFilter::Open;
    state.label_filter = None;
    state.checked.clear();
    state.search_query.clear();
    state.repo_name_with_owner = None;
    state.cursor = 0;
    state.mode = Mode::List;
    state.begin_loading("issues");
    *initial_load_rx = Some(spawn_initial_load(source.clone()));
}

fn diagnose_initial_load_error(e: anyhow::Error) -> anyhow::Error {
    let message = e.to_string();
    if message.contains("CLI not found on PATH") {
        return e;
    }
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
            state.set_status_success(format!(
                "loaded {count} issues in {}",
                format_duration(success.elapsed)
            ));
        }
        Err(e) => {
            state.finish_loading();
            state.set_status_error(gh_error_status(&e));
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

fn spawn_refresh<S: IssueSource>(source: S) -> RefreshReceiver {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let started = Instant::now();
        let issues_source = source.clone();
        let labels_source = source;
        let labels_handle = std::thread::spawn(move || labels_source.labels());
        let issues_result = fetch_open_and_closed(issues_source);
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
    *refresh_rx = Some(spawn_refresh(source));
    state.begin_pending(PendingOperation::RefreshList);
}

fn finish_refresh(state: &mut AppState, result: anyhow::Result<RefreshSuccess>) {
    state.finish_pending();
    match result {
        Ok(success) => {
            let count = success.issues.len();
            state.all_labels = success.labels;
            state.set_issues(success.issues);
            state.set_status_success(format!(
                "refreshed {count} issues in {}",
                format_duration(success.elapsed)
            ));
        }
        Err(e) => state.set_status_error(gh_error_status(&e)),
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
    *mutation_rx = Some(spawn_mutation(source, request));
    *mutation_draft = Some(draft);
    state.begin_pending(operation);
}

fn spawn_mutation<S: IssueSource>(source: S, request: MutationRequest) -> MutationReceiver {
    let operation = request.operation();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let action_started = Instant::now();
        let result = request.run(&source).and_then(|target_issue| {
            let action_elapsed = action_started.elapsed();
            let refresh_started = Instant::now();
            fetch_open_and_closed(source.clone()).map(|issues| MutationSuccess {
                operation,
                issues,
                action_elapsed,
                refresh_elapsed: refresh_started.elapsed(),
                target_issue,
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
            state.set_issues_selecting(success.issues, success.target_issue);
            if let Some(number) = success.target_issue {
                state.start_flash(number);
            }
            state.set_status_success(format!(
                "{} in {}, refresh {}",
                success_status_action(success.operation),
                format_duration(success.action_elapsed),
                format_duration(success.refresh_elapsed)
            ));
        }
        Err(e) => {
            restore_mutation_draft(state, draft);
            state.set_status_error(gh_error_status(&e));
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
        message.push_str(" (GITHUB_TOKEN is set; try env -u GITHUB_TOKEN boomerang)");
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
        format!("{message} (and `gh auth status` failed: run `gh auth login`)")
    }
}

fn probe_auth_status() -> bool {
    std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn copy_selected(
    state: &mut AppState,
    format: impl Fn(&model::Issue) -> String,
    copy_fn: impl Fn(&str) -> anyhow::Result<()>,
) -> bool {
    if !state.checked.is_empty() {
        let numbers: Vec<u32> = state.checked.iter().copied().collect();
        let texts: Vec<String> = numbers
            .iter()
            .filter_map(|&number| state.find_issue(number))
            .map(&format)
            .collect();
        if texts.is_empty() {
            return false;
        }
        let text = texts.join(", ");
        return match copy_fn(&text) {
            Ok(()) => {
                state.set_status(format!("copied {}: {text}", texts.len()));
                state.checked.clear();
                state.exit_on_copy_yank
            }
            Err(e) => {
                state.set_status(format!("copy failed: {e}"));
                false
            }
        };
    }
    if let Some(issue) = state.selected_issue() {
        let text = format(issue);
        match copy_fn(&text) {
            Ok(()) => {
                state.set_status(format!("copied: {text}"));
                return state.exit_on_copy_yank;
            }
            Err(e) => state.set_status(format!("copy failed: {e}")),
        }
    }
    false
}

fn handle_quit(state: &mut AppState) -> bool {
    if state.checked.is_empty() {
        true
    } else {
        state.checked.clear();
        false
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
    use crate::model::{IssueState, ERROR_ICONS, SUCCESS_ICONS};
    use ratatui::style::Color;

    fn assert_status_ends_with(state: &AppState, icons: &[char], suffix: &str) {
        let message = state
            .status
            .as_ref()
            .expect("status should be set")
            .0
            .clone();
        assert!(
            message.ends_with(suffix),
            "expected status to end with {suffix:?}, got {message:?}"
        );
        let icon = message
            .chars()
            .next()
            .expect("status message should have a leading icon");
        assert!(
            icons.contains(&icon),
            "expected leading icon {icon:?} to be one of {icons:?}"
        );
    }

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

    fn fake_copy_ok(_text: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn fake_copy_err(_text: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("no clipboard"))
    }

    #[test]
    fn copy_selected_returns_true_when_exit_on_copy_yank_is_set() {
        let mut state = AppState::new(vec![issue(1, "test")], vec![]);
        state.exit_on_copy_yank = true;
        assert!(copy_selected(
            &mut state,
            copy::format_reference,
            fake_copy_ok
        ));
    }

    #[test]
    fn copy_selected_returns_false_when_exit_on_copy_yank_is_off() {
        let mut state = AppState::new(vec![issue(1, "test")], vec![]);
        state.exit_on_copy_yank = false;
        assert!(!copy_selected(
            &mut state,
            copy::format_reference,
            fake_copy_ok
        ));
    }

    #[test]
    fn copy_selected_returns_false_with_no_selected_issue_even_if_setting_is_on() {
        let mut state = AppState::new(vec![], vec![]);
        state.exit_on_copy_yank = true;
        assert!(!copy_selected(
            &mut state,
            copy::format_reference,
            fake_copy_ok
        ));
    }

    #[test]
    fn copy_selected_joins_multiple_checked_issues_with_comma() {
        let mut state = AppState::new(
            vec![issue(1, "one"), issue(2, "two"), issue(3, "three")],
            vec![],
        );
        state.checked.insert(1);
        state.checked.insert(3);
        copy_selected(&mut state, copy::format_reference, fake_copy_ok);
        assert_eq!(state.status.as_ref().unwrap().0, "copied 2: #1, #3");
    }

    #[test]
    fn copy_selected_clears_checked_after_successful_multi_copy_even_when_exit_on_copy_yank_is_off()
    {
        let mut state = AppState::new(vec![issue(1, "one"), issue(2, "two")], vec![]);
        state.exit_on_copy_yank = false;
        state.checked.insert(1);
        state.checked.insert(2);
        copy_selected(&mut state, copy::format_reference, fake_copy_ok);
        assert!(state.checked.is_empty());
    }

    #[test]
    fn copy_selected_skips_checked_numbers_that_no_longer_resolve() {
        let mut state = AppState::new(vec![issue(1, "one")], vec![]);
        state.checked.insert(1);
        state.checked.insert(999);
        copy_selected(&mut state, copy::format_reference, fake_copy_ok);
        assert_eq!(state.status.as_ref().unwrap().0, "copied 1: #1");
    }

    #[test]
    fn copy_selected_falls_back_to_single_issue_when_nothing_checked() {
        let mut state = AppState::new(vec![issue(1, "one"), issue(2, "two")], vec![]);
        copy_selected(&mut state, copy::format_reference, fake_copy_ok);
        assert_eq!(state.status.as_ref().unwrap().0, "copied: #1");
    }

    #[test]
    fn copy_selected_sets_status_and_returns_false_when_copy_fails() {
        let mut state = AppState::new(vec![issue(1, "one")], vec![]);
        state.exit_on_copy_yank = true;
        assert!(!copy_selected(
            &mut state,
            copy::format_reference,
            fake_copy_err
        ));
        assert_eq!(
            state.status.as_ref().unwrap().0,
            "copy failed: no clipboard"
        );
    }

    #[test]
    fn handle_quit_clears_checked_and_does_not_quit_when_checks_active() {
        let mut state = AppState::new(vec![issue(1, "one")], vec![]);
        state.checked.insert(1);
        assert!(!handle_quit(&mut state));
        assert!(state.checked.is_empty());
    }

    #[test]
    fn handle_quit_quits_when_nothing_checked() {
        let mut state = AppState::new(vec![issue(1, "one")], vec![]);
        assert!(handle_quit(&mut state));
    }

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn parse_command_defaults_to_launch() {
        assert_eq!(
            parse_command(args(&[])),
            Ok(StartupCommand::Launch { repo: None })
        );
    }

    #[test]
    fn parse_command_accepts_a_bare_owner_repo_positional_arg() {
        assert_eq!(
            parse_command(args(&["jeffdt/rolomux"])),
            Ok(StartupCommand::Launch {
                repo: Some("jeffdt/rolomux".to_string())
            })
        );
    }

    #[test]
    fn parse_command_accepts_repo_flag() {
        assert_eq!(
            parse_command(args(&["--repo", "jeffdt/rolomux"])),
            Ok(StartupCommand::Launch {
                repo: Some("jeffdt/rolomux".to_string())
            })
        );
    }

    #[test]
    fn parse_command_accepts_a_github_url_positional_arg() {
        assert_eq!(
            parse_command(args(&["https://github.com/jeffdt/rolomux"])),
            Ok(StartupCommand::Launch {
                repo: Some("jeffdt/rolomux".to_string())
            })
        );
    }

    #[test]
    fn parse_command_rejects_repo_flag_missing_its_value() {
        assert_eq!(
            parse_command(args(&["--repo"])),
            Err("--repo requires an OWNER/REPO argument".to_string())
        );
    }

    #[test]
    fn parse_command_rejects_repo_flag_with_an_invalid_value() {
        assert!(parse_command(args(&["--repo", "not-a-repo"])).is_err());
    }

    #[test]
    fn parse_command_rejects_an_unrecognized_flag() {
        assert_eq!(
            parse_command(args(&["--nonsense"])),
            Err("unknown argument '--nonsense'".to_string())
        );
    }

    #[test]
    fn parse_command_rejects_a_positional_arg_that_is_not_a_repo() {
        assert_eq!(
            parse_command(args(&["not-a-repo"])),
            Err("unknown argument 'not-a-repo'".to_string())
        );
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
    fn merge_issue_lists_concatenates_open_then_closed() {
        let open = vec![issue(1, "Open one")];
        let closed = vec![issue(2, "Closed one")];
        assert_eq!(
            merge_issue_lists(open.clone(), closed.clone()),
            vec![issue(1, "Open one"), issue(2, "Closed one")]
        );
    }

    #[test]
    fn merge_issue_lists_handles_an_empty_bucket() {
        let open = vec![issue(1, "Only open")];
        assert_eq!(merge_issue_lists(open.clone(), vec![]), open);
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
        assert_status_ends_with(&state, &SUCCESS_ICONS, "loaded 1 issues in 350ms");
        assert_eq!(state.status_color(), Some(Color::Green));
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
        assert_status_ends_with(&state, &SUCCESS_ICONS, "refreshed 1 issues in 200ms");
        assert_eq!(state.status_color(), Some(Color::Green));
    }

    #[test]
    fn finish_refresh_failure_clears_pending_and_reports_error() {
        let mut state = AppState::new(vec![issue(1, "Existing issue")], vec![]);
        state.begin_pending(PendingOperation::RefreshList);
        finish_refresh(&mut state, Err(anyhow::anyhow!("network unreachable")));
        assert!(!state.is_pending());
        assert_eq!(state.issues, vec![issue(1, "Existing issue")]);
        assert_status_ends_with(&state, &ERROR_ICONS, "gh error: network unreachable");
        assert_eq!(state.status_color(), Some(Color::Red));
    }

    #[test]
    fn finish_initial_load_sets_repo_name_when_available() {
        let mut state = AppState::loading();
        finish_initial_load(
            &mut state,
            Ok(InitialLoadSuccess {
                issues: vec![],
                labels: vec![],
                repo_name: Some("jeffdt/boomerang".to_string()),
                elapsed: Duration::from_millis(10),
            }),
        );
        assert_eq!(
            state.repo_name_with_owner,
            Some("jeffdt/boomerang".to_string())
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
    fn apply_repo_name_result_sets_repo_name_on_success() {
        let mut state = AppState::new(vec![], vec![]);
        apply_repo_name_result(&mut state, Ok("jeffdt/boomerang".to_string()));
        assert_eq!(
            state.repo_name_with_owner,
            Some("jeffdt/boomerang".to_string())
        );
    }

    #[test]
    fn apply_repo_name_result_leaves_repo_name_none_on_failure() {
        let mut state = AppState::new(vec![], vec![]);
        apply_repo_name_result(&mut state, Err(anyhow::anyhow!("gh not found")));
        assert_eq!(state.repo_name_with_owner, None);
    }

    #[test]
    fn finish_initial_load_failure_clears_loading_and_reports_error() {
        let mut state = AppState::loading();
        finish_initial_load(&mut state, Err(anyhow::anyhow!("repo unavailable")));
        assert!(!state.is_loading());
        assert_status_ends_with(&state, &ERROR_ICONS, "gh error: repo unavailable");
        assert_eq!(state.status_color(), Some(Color::Red));
    }

    #[test]
    fn finish_create_success_updates_issues_and_reports_timing() {
        let created = issue(42, "Created issue");
        let mut state = AppState::new(vec![], vec![]);
        state.begin_pending(PendingOperation::CreateIssue);
        finish_mutation(
            &mut state,
            None,
            Ok(MutationSuccess {
                operation: PendingOperation::CreateIssue,
                issues: vec![created.clone()],
                action_elapsed: Duration::from_millis(1_200),
                refresh_elapsed: Duration::from_millis(50),
                target_issue: None,
            }),
        );
        assert_eq!(state.issues, vec![created]);
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_status_ends_with(&state, &SUCCESS_ICONS, "created issue in 1.2s, refresh 50ms");
        assert_eq!(state.status_color(), Some(Color::Green));
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
                target_issue: Some(42),
            }),
        );
        assert_eq!(state.issues, vec![updated]);
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_status_ends_with(&state, &SUCCESS_ICONS, "updated issue in 950ms, refresh 75ms");
        assert_eq!(state.status_color(), Some(Color::Green));
    }

    #[test]
    fn finish_edit_success_keeps_edited_issue_selected() {
        let mut state = AppState::new(
            vec![issue(1, "First"), issue(42, "Second"), issue(7, "Third")],
            vec![],
        );
        state.mode = Mode::Form(Box::new(FormState {
            editing: Some(42),
            ..FormState::with_title_body("Pending edit", "")
        }));
        state.begin_pending(PendingOperation::EditIssue);
        finish_mutation(
            &mut state,
            None,
            Ok(MutationSuccess {
                operation: PendingOperation::EditIssue,
                issues: vec![
                    issue(42, "Second, updated"),
                    issue(1, "First"),
                    issue(7, "Third"),
                ],
                action_elapsed: Duration::from_millis(950),
                refresh_elapsed: Duration::from_millis(75),
                target_issue: Some(42),
            }),
        );
        assert_eq!(state.selected_issue().map(|i| i.number), Some(42));
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
                target_issue: None,
            }),
        );
        assert_eq!(state.issues, vec![remaining]);
        assert!(!state.is_pending());
        assert_eq!(state.mode, Mode::List);
        assert_status_ends_with(&state, &SUCCESS_ICONS, "closed issue in 400ms, refresh 30ms");
        assert_eq!(state.status_color(), Some(Color::Green));
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
        assert_status_ends_with(&state, &ERROR_ICONS, "gh error: close failed");
        assert_eq!(state.status_color(), Some(Color::Red));
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
        assert_status_ends_with(&state, &ERROR_ICONS, "gh error: network failed");
        assert_eq!(state.status_color(), Some(Color::Red));
    }

    #[test]
    fn finish_mutation_starts_flash_for_edit_target() {
        let mut state = AppState::new(vec![issue(1, "one"), issue(2, "two")], vec![]);
        finish_mutation(
            &mut state,
            None,
            Ok(MutationSuccess {
                operation: PendingOperation::EditIssue,
                issues: vec![issue(1, "one"), issue(2, "two")],
                action_elapsed: Duration::from_millis(1),
                refresh_elapsed: Duration::from_millis(1),
                target_issue: Some(2),
            }),
        );
        assert_eq!(
            state.flash.map(|(number, _)| number),
            Some(2),
            "edit success should start a flash on the target issue"
        );
    }

    #[test]
    fn finish_mutation_starts_flash_for_create_target() {
        let mut state = AppState::new(vec![issue(1, "one")], vec![]);
        finish_mutation(
            &mut state,
            None,
            Ok(MutationSuccess {
                operation: PendingOperation::CreateIssue,
                issues: vec![issue(1, "one"), issue(42, "Created issue")],
                action_elapsed: Duration::from_millis(1),
                refresh_elapsed: Duration::from_millis(1),
                target_issue: Some(42),
            }),
        );
        assert_eq!(
            state.flash.map(|(number, _)| number),
            Some(42),
            "create success should start a flash on the newly created issue"
        );
    }

    #[test]
    fn finish_mutation_does_not_flash_when_there_is_no_target_issue() {
        let mut state = AppState::new(vec![issue(1, "one")], vec![]);
        finish_mutation(
            &mut state,
            None,
            Ok(MutationSuccess {
                operation: PendingOperation::CloseIssue,
                issues: vec![],
                action_elapsed: Duration::from_millis(1),
                refresh_elapsed: Duration::from_millis(1),
                target_issue: None,
            }),
        );
        assert_eq!(
            state.flash, None,
            "close success has no target issue, so nothing should flash"
        );
    }

    #[test]
    fn gh_error_status_includes_token_hint_for_access_errors_when_github_token_is_set() {
        let error = anyhow::anyhow!("GraphQL: Resource not accessible by integration");
        assert_eq!(
            gh_error_status_with_token_hint(&error, true),
            "gh error: GraphQL: Resource not accessible by integration (GITHUB_TOKEN is set; try env -u GITHUB_TOKEN boomerang)"
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
            "gh error: HTTP 403 forbidden (and `gh auth status` failed: run `gh auth login`)"
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

    #[test]
    fn diagnose_initial_load_error_leaves_gh_not_found_message_untouched() {
        let message = "`gh` CLI not found on PATH. Install it from https://cli.github.com and run `gh auth login`.";
        let e = anyhow::anyhow!(message);
        let result = diagnose_initial_load_error(e);
        assert_eq!(result.to_string(), message);
    }
}
