use crate::model::{AppState, FormField, Mode};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListInput {
    Up,
    Down,
    ToggleExpand,
    EnterSearch,
    CycleStateFilter,
    LittleCreate,
    BigCreate,
    Edit,
    RequestClose,
    CopyReference,
    CopyMarkdownLink,
    CopyUrl,
    Quit,
    None,
}

pub fn map_list_key(key: KeyEvent) -> ListInput {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => ListInput::Down,
        KeyCode::Char('k') | KeyCode::Up => ListInput::Up,
        KeyCode::Enter => ListInput::ToggleExpand,
        KeyCode::Char('/') => ListInput::EnterSearch,
        KeyCode::Char('a') => ListInput::CycleStateFilter,
        KeyCode::Char('c') => ListInput::LittleCreate,
        KeyCode::Char('C') => ListInput::BigCreate,
        KeyCode::Char('e') => ListInput::Edit,
        KeyCode::Char('x') => ListInput::RequestClose,
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => ListInput::CopyUrl,
        KeyCode::Char('y') => ListInput::CopyReference,
        KeyCode::Char('Y') => ListInput::CopyMarkdownLink,
        KeyCode::Char('q') | KeyCode::Esc => ListInput::Quit,
        _ => ListInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchInput {
    Char(char),
    Backspace,
    DeleteWord,
    Clear,
    Exit,
    None,
}

pub fn map_search_key(key: KeyEvent) -> SearchInput {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc | KeyCode::Enter => SearchInput::Exit,
        KeyCode::Backspace => SearchInput::Backspace,
        KeyCode::Char('w') if ctrl => SearchInput::DeleteWord,
        KeyCode::Char('u') if ctrl => SearchInput::Clear,
        KeyCode::Char(_) if ctrl => SearchInput::None,
        KeyCode::Char(c) => SearchInput::Char(c),
        _ => SearchInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LittleCreateInput {
    Char(char),
    Backspace,
    Submit,
    Cancel,
    None,
}

pub fn map_little_create_key(key: KeyEvent) -> LittleCreateInput {
    match key.code {
        KeyCode::Char(c) => LittleCreateInput::Char(c),
        KeyCode::Backspace => LittleCreateInput::Backspace,
        KeyCode::Enter => LittleCreateInput::Submit,
        KeyCode::Esc => LittleCreateInput::Cancel,
        _ => LittleCreateInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormInput {
    Char(char),
    Backspace,
    NextField,
    PrevField,
    Enter,
    MoveUp,
    MoveDown,
    ToggleLabel,
    Cancel,
    None,
}

pub fn map_form_key(key: KeyEvent, field: FormField) -> FormInput {
    match key.code {
        KeyCode::Tab => FormInput::NextField,
        KeyCode::BackTab => FormInput::PrevField,
        KeyCode::Esc => FormInput::Cancel,
        KeyCode::Enter => FormInput::Enter,
        KeyCode::Backspace => FormInput::Backspace,
        KeyCode::Up if field == FormField::Labels => FormInput::MoveUp,
        KeyCode::Down if field == FormField::Labels => FormInput::MoveDown,
        KeyCode::Char(' ') if field == FormField::Labels => FormInput::ToggleLabel,
        KeyCode::Char(c) => FormInput::Char(c),
        _ => FormInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmInput {
    Yes,
    No,
    None,
}

pub fn map_confirm_key(key: KeyEvent) -> ConfirmInput {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => ConfirmInput::Yes,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => ConfirmInput::No,
        _ => ConfirmInput::None,
    }
}

pub fn draw(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    match &state.mode {
        Mode::Form(form) => draw_form(frame, area, form),
        Mode::ConfirmClose(number) => draw_confirm_close(frame, area, *number, state),
        Mode::LittleCreate(buf) => draw_little_create(frame, area, buf),
        _ => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
                .split(area);
            draw_list(frame, chunks[0], state);
            draw_shortcuts_hint(frame, chunks[1], state);
            draw_toast(frame, chunks[2], state);
        }
    }
}

fn draw_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let visible = state.visible_indices();
    if visible.is_empty() {
        let message = format!("No issues found for state filter {:?}", state.state_filter);
        let list = List::new(vec![ListItem::new(message)])
            .block(Block::default().borders(Borders::ALL).title("Issues"));
        frame.render_widget(list, area);
        return;
    }
    let mut items: Vec<ListItem> = Vec::new();
    for (row, &idx) in visible.iter().enumerate() {
        let issue = &state.issues[idx];
        let mut spans = vec![Span::raw(format!("#{} ", issue.number)), Span::raw(issue.title.clone())];
        for label in &issue.labels {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!(" {} ", label.name),
                Style::default().bg(label_color(&label.color)).fg(Color::Black),
            ));
        }
        let style = if row == state.cursor {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        items.push(ListItem::new(Line::from(spans)).style(style));
        if state.expanded.contains(&issue.number) {
            if issue.body.is_empty() {
                items.push(ListItem::new("    (no description)"));
            } else {
                for line in issue.body.lines() {
                    items.push(ListItem::new(format!("    {line}")));
                }
            }
        }
    }
    let title = format!("Issues ({:?})", state.state_filter);
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, area);
}

fn draw_shortcuts_hint(frame: &mut Frame, area: Rect, state: &AppState) {
    let text = match &state.mode {
        Mode::Search => format!("/{}", state.search_query),
        _ => "j/k move  enter/←/→ expand  / search  a state  c/C create  e edit  x close  y/Y/^y copy  q quit".to_string(),
    };
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_toast(frame: &mut Frame, area: Rect, state: &AppState) {
    let text = state.status.as_ref().map(|(msg, _)| msg.as_str()).unwrap_or("");
    frame.render_widget(Paragraph::new(text), area);
}

fn label_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::Gray;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);
    Color::Rgb(r, g, b)
}

fn draw_little_create(frame: &mut Frame, area: Rect, buf: &str) {
    let block = Block::default().borders(Borders::ALL).title("New issue title (Enter to create, Esc to cancel)");
    frame.render_widget(Paragraph::new(buf).block(block), area);
}

fn draw_form(frame: &mut Frame, area: Rect, form: &crate::model::FormState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Min(3)])
        .split(area);

    frame.render_widget(
        Paragraph::new(form.title.as_str()).block(
            Block::default().borders(Borders::ALL).title("Title").border_style(field_style(form.field == FormField::Title)),
        ),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(form.body.as_str()).block(
            Block::default().borders(Borders::ALL).title("Body").border_style(field_style(form.field == FormField::Body)),
        ),
        chunks[1],
    );

    let items: Vec<ListItem> = form
        .all_label_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mark = if form.selected_labels.contains(name) { "[x]" } else { "[ ]" };
            let style = if i == form.label_cursor {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(format!("{mark} {name}")).style(style)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(
            Block::default().borders(Borders::ALL).title("Labels (space to toggle)").border_style(field_style(form.field == FormField::Labels)),
        ),
        chunks[2],
    );
}

fn field_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn draw_confirm_close(frame: &mut Frame, area: Rect, number: u32, state: &AppState) {
    let title = state.issues.iter().find(|i| i.number == number).map(|i| i.title.as_str()).unwrap_or("");
    let text = format!("Close #{number}: {title}? (y/n)");
    frame.render_widget(Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Confirm")), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    #[test]
    fn maps_lowercase_y_to_copy_reference() {
        assert_eq!(map_list_key(key(KeyCode::Char('y'))), ListInput::CopyReference);
    }

    #[test]
    fn maps_uppercase_y_to_copy_markdown_link() {
        assert_eq!(map_list_key(key(KeyCode::Char('Y'))), ListInput::CopyMarkdownLink);
    }

    #[test]
    fn maps_ctrl_y_to_copy_url() {
        let k = key_with(KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(map_list_key(k), ListInput::CopyUrl);
    }

    #[test]
    fn maps_shift_c_to_big_create_and_lowercase_c_to_little_create() {
        assert_eq!(map_list_key(key(KeyCode::Char('c'))), ListInput::LittleCreate);
        assert_eq!(map_list_key(key(KeyCode::Char('C'))), ListInput::BigCreate);
    }

    #[test]
    fn search_key_mapping_exits_on_enter_or_esc() {
        assert_eq!(map_search_key(key(KeyCode::Enter)), SearchInput::Exit);
        assert_eq!(map_search_key(key(KeyCode::Esc)), SearchInput::Exit);
        assert_eq!(map_search_key(key(KeyCode::Char('x'))), SearchInput::Char('x'));
    }

    #[test]
    fn search_key_mapping_swallows_ctrl_chars() {
        let ctrl_h = key_with(KeyCode::Char('h'), KeyModifiers::CONTROL);
        assert_eq!(map_search_key(ctrl_h), SearchInput::None);
    }

    #[test]
    fn search_key_mapping_handles_ctrl_u_as_clear() {
        let ctrl_u = key_with(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(map_search_key(ctrl_u), SearchInput::Clear);
    }

    #[test]
    fn search_key_mapping_handles_ctrl_w_as_delete_word() {
        let ctrl_w = key_with(KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(map_search_key(ctrl_w), SearchInput::DeleteWord);
    }

    #[test]
    fn search_key_mapping_plain_char_unaffected() {
        assert_eq!(map_search_key(key(KeyCode::Char('h'))), SearchInput::Char('h'));
    }

    #[test]
    fn form_key_mapping_reserves_space_and_arrows_for_labels_field() {
        assert_eq!(map_form_key(key(KeyCode::Char(' ')), FormField::Labels), FormInput::ToggleLabel);
        assert_eq!(map_form_key(key(KeyCode::Char(' ')), FormField::Title), FormInput::Char(' '));
        assert_eq!(map_form_key(key(KeyCode::Down), FormField::Labels), FormInput::MoveDown);
    }

    #[test]
    fn confirm_key_mapping() {
        assert_eq!(map_confirm_key(key(KeyCode::Char('y'))), ConfirmInput::Yes);
        assert_eq!(map_confirm_key(key(KeyCode::Char('n'))), ConfirmInput::No);
        assert_eq!(map_confirm_key(key(KeyCode::Esc)), ConfirmInput::No);
    }

    use crate::gh::StateFilter;
    use crate::model::{Issue, IssueState, Label};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn issue(number: u32, title: &str) -> Issue {
        Issue { number, title: title.into(), body: String::new(), labels: vec![], state: IssueState::Open, url: String::new() }
    }

    fn render_to_string(state: &AppState) -> String {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_issue_number_and_title() {
        let state = AppState::new(vec![issue(42, "Fix login bug")], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("#42"));
        assert!(rendered.contains("Fix login bug"));
    }

    #[test]
    fn renders_state_filter_in_list_title() {
        let state = AppState::new(vec![], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains(&format!("{:?}", StateFilter::Open)));
    }

    #[test]
    fn expanded_issue_shows_body_text() {
        let mut issue = issue(1, "Fix bug");
        issue.body = "steps to repro".into();
        let mut state = AppState::new(vec![issue], vec![]);
        state.toggle_expand();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("steps to repro"));
    }

    #[test]
    fn search_mode_shows_query_in_status_line() {
        let mut state = AppState::new(vec![issue(1, "a")], vec![]);
        state.enter_search();
        state.search_push('x');
        let rendered = render_to_string(&state);
        assert!(rendered.contains("/x"));
    }

    #[test]
    fn shows_friendly_message_when_no_issues_match_filter() {
        let state = AppState::new(vec![], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("No issues"));
    }

    #[test]
    fn little_create_mode_renders_typed_title() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        state.little_create_push('F');
        state.little_create_push('i');
        state.little_create_push('x');
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Fix"));
    }

    #[test]
    fn big_create_form_renders_title_body_and_labels() {
        let mut state = AppState::new(vec![], vec![Label { name: "bug".into(), color: "d73a4a".into() }]);
        state.enter_big_create();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Title"));
        assert!(rendered.contains("Body"));
        assert!(rendered.contains("bug"));
    }

    #[test]
    fn confirm_close_renders_issue_title_and_prompt() {
        let mut state = AppState::new(vec![issue(9, "Close me")], vec![]);
        state.request_close();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Close me"));
        assert!(rendered.contains("(y/n)"));
    }
}
