mod copy;
mod gh;
mod model;
mod search;
mod ui;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use gh::{GhCliSource, IssueSource, StateFilter};
use model::{AppState, Mode};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use std::time::Duration;
use ui::{
    map_confirm_key, map_form_key, map_list_key, map_little_create_key, map_search_key, ConfirmInput, FormInput,
    LittleCreateInput, ListInput, SearchInput,
};

const HELP: &str = "\
issue-browser - a tmux-popup TUI for GitHub issues

Usage:
  issue-browser            Launch the picker (intended via `tmux popup -E`)
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
            other => {
                eprintln!("issue-browser: unknown argument '{other}'\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    if std::process::Command::new("gh").arg("--version").output().is_err() {
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
    let issues_handle = std::thread::spawn(move || GhCliSource::new().list(StateFilter::Open));
    let labels_handle = std::thread::spawn(move || GhCliSource::new().labels());
    let issues = issues_handle.join().expect("issue list thread panicked")?;
    let labels = labels_handle.join().expect("label list thread panicked").unwrap_or_default();
    let mut state = AppState::new(issues, labels);

    run_ui(&mut state, &source)
}

fn run_ui(state: &mut AppState, source: &impl IssueSource) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let result = event_loop(&mut terminal, state, source);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    source: &impl IssueSource,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, state))?;
        state.clear_expired_status();
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != event::KeyEventKind::Press {
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
                            let result = source.create(&title, "", &[]);
                            apply_result(state, source, result);
                        }
                    }
                    LittleCreateInput::Cancel => state.cancel_form_or_create(),
                    LittleCreateInput::None => {}
                },
                Mode::Form(form) => {
                    let field = form.field;
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
                                let result = match submission.editing {
                                    Some(number) => source.edit(
                                        number,
                                        &submission.title,
                                        &submission.body,
                                        &submission.add_labels,
                                        &submission.remove_labels,
                                    ),
                                    None => {
                                        source.create(&submission.title, &submission.body, &submission.add_labels)
                                    }
                                };
                                apply_result(state, source, result);
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

fn refresh(state: &mut AppState, source: &impl IssueSource) {
    match source.list(state.state_filter) {
        Ok(issues) => state.set_issues(issues),
        Err(e) => state.set_status(format!("gh error: {e}")),
    }
}

/// Refresh the issue list on success, or surface the error on the toast line.
/// Shared by every mutating action (create/edit/close) since they all react
/// to their `gh` call's result the same way.
fn apply_result(state: &mut AppState, source: &impl IssueSource, result: anyhow::Result<()>) {
    match result {
        Ok(()) => refresh(state, source),
        Err(e) => state.set_status(format!("gh error: {e}")),
    }
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
