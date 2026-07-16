use crate::loading;
use crate::model::{AppState, FormField, Label, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use ratatui_textarea::Input;

const POPUP_MARGIN: u16 = 2;

const LABEL_PALETTE: [Color; 6] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::Red,
];

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const SEL_BG: Color = Color::DarkGray;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListInput {
    Up,
    Down,
    TogglePane,
    EnterSearch,
    CycleStateFilter,
    LittleCreate,
    BigCreate,
    Edit,
    RequestClose,
    CopyReference,
    CopyMarkdownLink,
    CopyUrl,
    OpenInBrowser,
    Refresh,
    Quit,
    None,
}

pub fn map_list_key(key: KeyEvent) -> ListInput {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => ListInput::Down,
        KeyCode::Char('k') | KeyCode::Up => ListInput::Up,
        KeyCode::Char('h') => ListInput::TogglePane,
        KeyCode::Enter | KeyCode::Char('e') => ListInput::Edit,
        KeyCode::Char('/') => ListInput::EnterSearch,
        KeyCode::Char('a') => ListInput::CycleStateFilter,
        KeyCode::Char('c') => ListInput::LittleCreate,
        KeyCode::Char('C') => ListInput::BigCreate,
        KeyCode::Char('x') => ListInput::RequestClose,
        KeyCode::Char('o') => ListInput::OpenInBrowser,
        KeyCode::Char('r') => ListInput::Refresh,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormInput {
    NextField,
    PrevField,
    Enter,
    MoveUp,
    MoveDown,
    ToggleLabel,
    Cancel,
    SubmitNow,
    TextEdit(Input),
    None,
}

pub fn map_form_key(key: KeyEvent, field: FormField) -> FormInput {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('s') if ctrl => return FormInput::SubmitNow,
        KeyCode::Tab => return FormInput::NextField,
        KeyCode::BackTab => return FormInput::PrevField,
        KeyCode::Esc => return FormInput::Cancel,
        KeyCode::Enter | KeyCode::Char(' ') if field == FormField::Submit => {
            return FormInput::Enter
        }
        KeyCode::Enter if field == FormField::Title => return FormInput::Enter,
        KeyCode::Up if field == FormField::Labels => return FormInput::MoveUp,
        KeyCode::Down if field == FormField::Labels => return FormInput::MoveDown,
        KeyCode::Char(' ') if field == FormField::Labels => return FormInput::ToggleLabel,
        _ => {}
    }
    match field {
        FormField::Title | FormField::Body => FormInput::TextEdit(Input::from(key)),
        FormField::Labels | FormField::Submit => FormInput::None,
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

/// Shrink `area` by `margin` cells on every side, reducing the margin toward
/// zero rather than panicking if the area is too small to inset cleanly.
fn inset(area: Rect, margin: u16) -> Rect {
    let mx = margin.min(area.width.saturating_sub(1) / 2);
    let my = margin.min(area.height.saturating_sub(1) / 2);
    Rect {
        x: area.x + mx,
        y: area.y + my,
        width: area.width.saturating_sub(2 * mx),
        height: area.height.saturating_sub(2 * my),
    }
}

pub fn draw(frame: &mut Frame, state: &AppState) {
    if let Mode::LittleCreate(buf) = &state.mode {
        draw_little_create(frame, buf, state);
        return;
    }

    let area = inset(frame.area(), POPUP_MARGIN);
    let border_style = Style::default().fg(ACCENT);
    let title_text = state
        .repo_name_with_owner
        .as_deref()
        .map(format_repo_title)
        .unwrap_or_else(|| "issue-browser".to_string());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Line::from(vec![
            Span::styled("─", border_style),
            Span::styled(
                format!("‹ {title_text} ›"),
                border_style.add_modifier(Modifier::BOLD | Modifier::ITALIC),
            ),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.is_loading() {
        loading::draw(frame, inner, state);
        return;
    }

    match &state.mode {
        Mode::Form(form) => draw_form(frame, inner, form, state),
        Mode::ConfirmClose(number) => draw_confirm_close(frame, inner, *number, state),
        Mode::ConfirmDiscard(previous) => draw_confirm_discard(frame, inner, previous),
        _ => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            if state.pane_open {
                let list_and_pane = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(chunks[0]);
                draw_list(frame, list_and_pane[0], state);
                draw_pane(frame, list_and_pane[1], state);
            } else {
                draw_list(frame, chunks[0], state);
            }
            draw_shortcuts_hint(frame, chunks[1], state);
            draw_toast(frame, chunks[2], state);
        }
    }
}

fn draw_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    let mut header = format!("Issues ({:?})", state.state_filter);
    if let Some(pending) = state.pending_message() {
        header.push_str("  ");
        header.push_str(&pending);
    }
    frame.render_widget(
        Paragraph::new(header).style(Style::default().fg(DIM)),
        chunks[1],
    );
    let list_area = chunks[2];

    let visible = state.visible_indices();
    if visible.is_empty() {
        let message = format!("No issues found for state filter {:?}", state.state_filter);
        frame.render_widget(List::new(vec![ListItem::new(message)]), list_area);
        return;
    }
    let available_width = list_area.width as usize;
    let mut items: Vec<ListItem> = Vec::new();
    for (row, &idx) in visible.iter().enumerate() {
        let issue = &state.issues[idx];
        let number_col = format!("{:<6}", format!("#{}", issue.number));
        let mut label_spans = Vec::new();
        let mut labels_width = 0usize;
        if !issue.labels.is_empty() {
            // Tally the labels' rendered width in the same pass that builds their
            // spans, so the padding calculation can never drift from what's
            // actually drawn (no separate width formula to keep in sync).
            for (i, label) in issue.labels.iter().enumerate() {
                if i > 0 {
                    label_spans.push(Span::raw(" "));
                    labels_width += 1;
                }
                let badge = label.name.clone();
                labels_width += badge.chars().count();
                label_spans.push(Span::styled(
                    badge,
                    label_style(label_palette_color(&state.all_labels, &label.name)),
                ));
            }
        }
        let max_title_width = available_width
            .saturating_sub(number_col.chars().count())
            .saturating_sub(labels_width);
        let title = truncate_title(&issue.title, max_title_width);
        let selected = row == state.cursor;
        let left_width = number_col.chars().count() + title.chars().count();
        let mut spans = vec![
            Span::styled(number_col, secondary(selected)),
            Span::raw(title),
        ];
        if !label_spans.is_empty() {
            let pad = available_width
                .saturating_sub(left_width)
                .saturating_sub(labels_width);
            spans.push(Span::raw(" ".repeat(pad)));
            spans.extend(label_spans);
        }
        let style = if selected {
            Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(Line::from(spans)).style(style));
    }
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));
    frame.render_stateful_widget(List::new(items), list_area, &mut list_state);
}

fn draw_pane(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(issue) = state.selected_issue() else {
        return;
    };
    let border_style = Style::default().fg(ACCENT);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Line::from(vec![
            Span::styled("─", border_style),
            Span::styled(
                format!("‹ #{} ›", issue.number),
                border_style.add_modifier(Modifier::BOLD | Modifier::ITALIC),
            ),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        // 2 rows reserved for the title: most titles wrap to 1 line, leaving
        // a blank row, but longer titles need the second line.
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(issue.title.as_str())
            .style(Style::default().add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    let date = issue
        .created_at
        .split('T')
        .next()
        .unwrap_or(&issue.created_at);
    let metadata = format!("opened {date}");
    frame.render_widget(
        Paragraph::new(metadata).style(Style::default().fg(DIM)),
        chunks[1],
    );
    let rule = "─".repeat(chunks[2].width as usize);
    frame.render_widget(
        Paragraph::new(rule).style(Style::default().fg(DIM)),
        chunks[2],
    );
    let body = if issue.body.is_empty() {
        "(no description)"
    } else {
        issue.body.as_str()
    };
    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), chunks[3]);
}

fn draw_shortcuts_hint(frame: &mut Frame, area: Rect, state: &AppState) {
    let idle = !state.is_loading() && !state.is_pending();
    if idle {
        if let Mode::Search = &state.mode {
            let text = format!("/{}", state.search_query);
            frame.render_widget(
                Paragraph::new(text)
                    .style(Style::default().fg(DIM))
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
    }
    // The pending spinner text itself renders in draw_toast; repeating it
    // here would show it twice stacked in the footer.
    let text = if !idle {
        "q quit".to_string()
    } else {
        match &state.mode {
            Mode::Form(_) => "tab/shift+tab field · ctrl+s submit · ctrl+w delete word · ctrl+u clear line · esc cancel".to_string(),
            _ => "j/k move · h hide pane · / search · a state · c/C create · enter/e edit · x close · o open · y/Y/^y copy · q quit".to_string(),
        }
    };
    frame.render_widget(
        Paragraph::new(styled_hint(&text)).wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_toast(frame: &mut Frame, area: Rect, state: &AppState) {
    let text = state
        .pending_message()
        .or_else(|| state.status.as_ref().map(|(msg, _)| msg.clone()))
        .unwrap_or_default();
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
}

/// Style a `"key desc · key desc"` hint line so each segment's leading key
/// token renders in Gray, a step brighter than its DarkGray description,
/// giving shortcut areas contrast against the rest of the dim chrome.
/// Ported from rolomux's `styled_hint` (`smux/src/ui.rs`); Gray without
/// Bold reads as a gentle nudge rather than a shout — an earlier version
/// used Bold with the plain default fg and was too bright.
fn styled_hint(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, segment) in text.split(" · ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(DIM)));
        }
        match segment.split_once(' ') {
            Some((key, desc)) => {
                spans.push(Span::styled(
                    key.to_string(),
                    Style::default().fg(Color::Gray),
                ));
                spans.push(Span::styled(format!(" {desc}"), Style::default().fg(DIM)));
            }
            None => spans.push(Span::styled(segment.to_string(), Style::default().fg(DIM))),
        }
    }
    Line::from(spans)
}

/// Assign a label a color from `LABEL_PALETTE` by its position in the repo's
/// full label list (fetched once at startup), cycling through the palette.
/// No persisted assignment and no hashing: `all_labels` is already stable
/// for the session, so the same name gets the same color for as long as the
/// popup is open, and colors are free to shift across launches as labels are
/// added/removed on the repo.
fn label_palette_color(all_labels: &[Label], name: &str) -> Color {
    let index = all_labels.iter().position(|l| l.name == name).unwrap_or(0);
    LABEL_PALETTE[index % LABEL_PALETTE.len()]
}

/// Truncate `title` to `max_width` characters, replacing the tail with `...`
/// when it doesn't fit. `max_width` is a character count, not a byte count,
/// so multi-byte titles truncate at character boundaries.
fn truncate_title(title: &str, max_width: usize) -> String {
    if title.chars().count() <= max_width {
        return title.to_string();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }
    let kept: String = title.chars().take(max_width - 3).collect();
    format!("{kept}...")
}

/// Seam for a future settings feature (e.g. showing the repo name without its
/// owner) to hook into without touching call sites; today it's a passthrough.
fn format_repo_title(repo: &str) -> String {
    repo.to_string()
}

fn label_style(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::ITALIC)
}

/// Style for less-important row chrome (currently just the issue-number
/// column). Drops back to the default foreground on the selected row so it
/// stays legible against the SEL_BG highlight bar rather than compounding
/// into DarkGray-on-DarkGray.
fn secondary(selected: bool) -> Style {
    if selected {
        Style::default()
    } else {
        Style::default().fg(DIM)
    }
}

fn draw_little_create(frame: &mut Frame, buf: &str, state: &AppState) {
    // Deliberately flush at the top, unlike every other screen: this is a
    // separate, intentionally distinct compact prompt (issue #46 follow-up),
    // not meant to visually match the rest of the app's chrome. Only left/
    // right margin comes from inset(); y/height are computed directly from
    // the frame so the popup can be sized to content with no wasted margin.
    let inset_area = inset(frame.area(), POPUP_MARGIN);
    let area = Rect {
        y: frame.area().y,
        height: frame.area().height.min(4),
        ..inset_area
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(1)])
        .split(area);

    let border_style = Style::default().fg(ACCENT);
    let title_text = match state.repo_name_with_owner.as_deref() {
        Some(repo) => format!("New issue in {repo}"),
        None => "New issue".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Line::from(vec![
            Span::styled("─", border_style),
            Span::styled(
                format!("‹ {title_text} ›"),
                border_style.add_modifier(Modifier::BOLD | Modifier::ITALIC),
            ),
        ]));
    frame.render_widget(Paragraph::new(buf).block(block), chunks[0]);

    // The hint is only ever actionable when idle (capture_loop ignores keys
    // other than quit while pending), so it's safe to swap it out entirely
    // for the pending/status message rather than reserving a second row —
    // same pattern draw_shortcuts_hint already uses for the main list.
    let footer_message = state
        .pending_message()
        .or_else(|| state.status.as_ref().map(|(msg, _)| msg.clone()));
    let footer_line = match footer_message {
        Some(msg) => Line::from(msg),
        None => styled_hint("enter create · esc cancel"),
    };
    frame.render_widget(
        Paragraph::new(footer_line).wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_form(frame: &mut Frame, area: Rect, form: &crate::model::FormState, state: &AppState) {
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(outer_chunks[0]);

    let title_block = Block::default()
        .borders(Borders::ALL)
        .title("Title")
        .border_style(field_style(form.field == FormField::Title));
    let title_inner = title_block.inner(chunks[0]);
    frame.render_widget(title_block, chunks[0]);
    frame.render_widget(&form.title_input, title_inner);

    let body_block = Block::default()
        .borders(Borders::ALL)
        .title("Body")
        .border_style(field_style(form.field == FormField::Body));
    let body_inner = body_block.inner(chunks[1]);
    frame.render_widget(body_block, chunks[1]);
    frame.render_widget(&form.body_input, body_inner);

    let items: Vec<ListItem> = form
        .all_label_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mark = if form.selected_labels.contains(name) {
                "[x]"
            } else {
                "[ ]"
            };
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
            Block::default()
                .borders(Borders::ALL)
                .title("Labels (space to toggle)")
                .border_style(field_style(form.field == FormField::Labels)),
        ),
        chunks[2],
    );

    let submit_focused = form.field == FormField::Submit;
    let submit_text = if submit_focused {
        Span::styled("Submit", Style::default().add_modifier(Modifier::REVERSED))
    } else {
        Span::raw("Submit")
    };
    frame.render_widget(
        Paragraph::new(Line::from(submit_text))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(field_style(submit_focused)),
            ),
        chunks[3],
    );

    draw_shortcuts_hint(frame, outer_chunks[1], state);
    draw_toast(frame, outer_chunks[2], state);
}

fn field_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn draw_confirm_close(frame: &mut Frame, area: Rect, number: u32, state: &AppState) {
    let title = state
        .find_issue(number)
        .map(|i| i.title.as_str())
        .unwrap_or("");
    let text = format!("Close #{number}: {title}? (y/n)");
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title("Confirm"),
        ),
        area,
    );
}

fn draw_confirm_discard(frame: &mut Frame, area: Rect, previous: &Mode) {
    let text = match previous {
        Mode::Form(form) => match form.editing {
            Some(number) => format!("Discard unsaved changes to #{number}? (y/n)"),
            None => "Discard this new issue? (y/n)".to_string(),
        },
        Mode::LittleCreate(_) => "Discard this new issue title? (y/n)".to_string(),
        _ => "Discard unsaved changes? (y/n)".to_string(),
    };
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title("Confirm"),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn type_into_form(state: &mut AppState, s: &str) {
        for c in s.chars() {
            state.form_input(Input {
                key: ratatui_textarea::Key::Char(c),
                ctrl: false,
                alt: false,
                shift: false,
            });
        }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn maps_lowercase_y_to_copy_reference() {
        assert_eq!(
            map_list_key(key(KeyCode::Char('y'))),
            ListInput::CopyReference
        );
    }

    #[test]
    fn maps_uppercase_y_to_copy_markdown_link() {
        assert_eq!(
            map_list_key(key(KeyCode::Char('Y'))),
            ListInput::CopyMarkdownLink
        );
    }

    #[test]
    fn maps_ctrl_y_to_copy_url() {
        let k = key_with(KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(map_list_key(k), ListInput::CopyUrl);
    }

    #[test]
    fn maps_lowercase_r_to_refresh() {
        assert_eq!(map_list_key(key(KeyCode::Char('r'))), ListInput::Refresh);
    }

    #[test]
    fn maps_lowercase_o_to_open_in_browser() {
        assert_eq!(
            map_list_key(key(KeyCode::Char('o'))),
            ListInput::OpenInBrowser
        );
    }

    #[test]
    fn maps_shift_c_to_big_create_and_lowercase_c_to_little_create() {
        assert_eq!(
            map_list_key(key(KeyCode::Char('c'))),
            ListInput::LittleCreate
        );
        assert_eq!(map_list_key(key(KeyCode::Char('C'))), ListInput::BigCreate);
    }

    #[test]
    fn search_key_mapping_exits_on_enter_or_esc() {
        assert_eq!(map_search_key(key(KeyCode::Enter)), SearchInput::Exit);
        assert_eq!(map_search_key(key(KeyCode::Esc)), SearchInput::Exit);
        assert_eq!(
            map_search_key(key(KeyCode::Char('x'))),
            SearchInput::Char('x')
        );
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
        assert_eq!(
            map_search_key(key(KeyCode::Char('h'))),
            SearchInput::Char('h')
        );
    }

    #[test]
    fn form_key_mapping_reserves_space_and_arrows_for_labels_field() {
        assert_eq!(
            map_form_key(key(KeyCode::Char(' ')), FormField::Labels),
            FormInput::ToggleLabel
        );
        assert_eq!(
            map_form_key(key(KeyCode::Char(' ')), FormField::Title),
            FormInput::TextEdit(Input::from(key(KeyCode::Char(' '))))
        );
        assert_eq!(
            map_form_key(key(KeyCode::Down), FormField::Labels),
            FormInput::MoveDown
        );
    }

    #[test]
    fn form_key_mapping_routes_navigation_keys_to_text_edit() {
        assert_eq!(
            map_form_key(key(KeyCode::Left), FormField::Title),
            FormInput::TextEdit(Input::from(key(KeyCode::Left)))
        );
        assert_eq!(
            map_form_key(key(KeyCode::Right), FormField::Body),
            FormInput::TextEdit(Input::from(key(KeyCode::Right)))
        );
        assert_eq!(
            map_form_key(key(KeyCode::Home), FormField::Title),
            FormInput::TextEdit(Input::from(key(KeyCode::Home)))
        );
        assert_eq!(
            map_form_key(key(KeyCode::End), FormField::Body),
            FormInput::TextEdit(Input::from(key(KeyCode::End)))
        );
        assert_eq!(
            map_form_key(key(KeyCode::Up), FormField::Body),
            FormInput::TextEdit(Input::from(key(KeyCode::Up)))
        );
        assert_eq!(
            map_form_key(key(KeyCode::Down), FormField::Body),
            FormInput::TextEdit(Input::from(key(KeyCode::Down)))
        );
        assert_eq!(
            map_form_key(key(KeyCode::Up), FormField::Labels),
            FormInput::MoveUp,
            "Labels keeps its own Up/Down for list navigation, unaffected by text routing"
        );
    }

    #[test]
    fn form_key_mapping_routes_delete_and_submit_shortcuts() {
        let ctrl_w = key_with(KeyCode::Char('w'), KeyModifiers::CONTROL);
        let ctrl_u = key_with(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let ctrl_s = key_with(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(
            map_form_key(ctrl_w, FormField::Title),
            FormInput::TextEdit(Input::from(ctrl_w))
        );
        assert_eq!(
            map_form_key(ctrl_u, FormField::Body),
            FormInput::TextEdit(Input::from(ctrl_u))
        );
        assert_eq!(
            map_form_key(ctrl_s, FormField::Labels),
            FormInput::SubmitNow
        );
    }

    #[test]
    fn form_key_mapping_enter_and_space_on_submit_both_confirm() {
        assert_eq!(
            map_form_key(key(KeyCode::Enter), FormField::Submit),
            FormInput::Enter
        );
        assert_eq!(
            map_form_key(key(KeyCode::Char(' ')), FormField::Submit),
            FormInput::Enter
        );
    }

    #[test]
    fn confirm_key_mapping() {
        assert_eq!(map_confirm_key(key(KeyCode::Char('y'))), ConfirmInput::Yes);
        assert_eq!(map_confirm_key(key(KeyCode::Char('n'))), ConfirmInput::No);
        assert_eq!(map_confirm_key(key(KeyCode::Esc)), ConfirmInput::No);
    }

    #[test]
    fn maps_h_to_toggle_pane() {
        assert_eq!(map_list_key(key(KeyCode::Char('h'))), ListInput::TogglePane);
    }

    #[test]
    fn maps_enter_to_edit() {
        assert_eq!(map_list_key(key(KeyCode::Enter)), ListInput::Edit);
    }

    #[test]
    fn maps_lowercase_e_to_edit() {
        assert_eq!(map_list_key(key(KeyCode::Char('e'))), ListInput::Edit);
    }

    #[test]
    fn right_and_left_are_now_unbound() {
        assert_eq!(map_list_key(key(KeyCode::Right)), ListInput::None);
        assert_eq!(map_list_key(key(KeyCode::Left)), ListInput::None);
    }

    use crate::gh::StateFilter;
    use crate::model::{Issue, IssueState, Label, LoadingAnimation, PendingOperation};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::time::Instant;

    fn issue(number: u32, title: &str) -> Issue {
        Issue {
            number,
            title: title.into(),
            body: String::new(),
            labels: vec![],
            state: IssueState::Open,
            url: String::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn render_buffer(state: &AppState) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, state)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn render_to_string(state: &AppState) -> String {
        let buf = render_buffer(state);
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn find_in_buffer(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<(u16, u16)> {
        for y in 0..buf.area.height {
            let mut row = String::new();
            for x in 0..buf.area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if let Some(byte_idx) = row.find(needle) {
                let x = row[..byte_idx].chars().count() as u16;
                return Some((x, y));
            }
        }
        None
    }

    #[test]
    fn selected_row_uses_dark_gray_background_and_bold_not_reverse_video() {
        let state = AppState::new(vec![issue(1, "Fix bug")], vec![]);
        let buf = render_buffer(&state);
        let (x, y) = find_in_buffer(&buf, "#1").expect("issue row should render");
        let style = buf[(x, y)].style();
        assert_eq!(
            style.bg,
            Some(Color::DarkGray),
            "selected row should have a DarkGray background highlight"
        );
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "selected row should render bold"
        );
        assert!(
            !style.add_modifier.contains(Modifier::REVERSED),
            "selected row should not use REVERSED"
        );
    }

    #[test]
    fn issue_number_is_dim_when_not_selected_and_default_when_selected() {
        let state = AppState::new(vec![issue(1, "First"), issue(2, "Second")], vec![]);
        let buf = render_buffer(&state);
        // Cursor starts on row 0 (#1), so #1 is selected and #2 is not.
        let (sx, sy) = find_in_buffer(&buf, "#1").expect("#1 should render");
        assert_eq!(
            buf[(sx, sy)].style().fg,
            Some(Color::Reset),
            "issue number on the selected row should use the default (Reset) foreground, not DIM (ratatui's Cell::style() always reports a concrete fg, Some(Color::Reset) when nothing set it, never None)"
        );
        let (ux, uy) = find_in_buffer(&buf, "#2").expect("#2 should render");
        assert_eq!(
            buf[(ux, uy)].style().fg,
            Some(Color::DarkGray),
            "issue number on an unselected row should be DIM"
        );
    }

    #[test]
    fn list_header_uses_dim_color_not_dim_modifier() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let buf = render_buffer(&state);
        let (x, y) = find_in_buffer(&buf, "Issues (").expect("header should render");
        let style = buf[(x, y)].style();
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert!(!style.add_modifier.contains(Modifier::DIM));
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
    fn loading_state_renders_header_hint_and_animation_area() {
        let mut state = AppState::loading();
        state.loading.as_mut().unwrap().animation = LoadingAnimation::MatrixRain;
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Loading issues..."));
        assert!(rendered.contains("q quit"));
        assert!(
            !rendered.contains("No issues found"),
            "loading should use the animated body instead of the empty-list message"
        );
    }

    #[test]
    fn color_ripple_loading_animation_renders_bullseye_content() {
        let mut state = AppState::loading();
        let loading = state.loading.as_mut().expect("loading state");
        loading.animation = LoadingAnimation::ColorRipple;
        loading.started_at = Instant::now();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("●"));
        assert!(!rendered.contains("No issues found"));
    }

    #[test]
    fn pane_is_shown_by_default() {
        let state = AppState::new(vec![issue(1, "Fix bug")], vec![]);
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("opened"),
            "pane should render on a fresh AppState"
        );
    }

    #[test]
    fn hiding_pane_removes_it() {
        let mut state = AppState::new(vec![issue(1, "Fix bug")], vec![]);
        state.toggle_pane();
        let rendered = render_to_string(&state);
        assert!(
            !rendered.contains("opened"),
            "pane should not render once hidden"
        );
    }

    #[test]
    fn pane_shows_created_date_and_body() {
        let mut selected = issue(1, "Fix bug");
        selected.body = "steps to repro".into();
        selected.created_at = "2026-06-01T12:00:00Z".into();
        let state = AppState::new(vec![selected], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("opened 2026-06-01"));
        assert!(rendered.contains("steps to repro"));
    }

    #[test]
    fn pane_shows_placeholder_for_empty_body() {
        let state = AppState::new(vec![issue(1, "Fix bug")], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("(no description)"));
    }

    #[test]
    fn pane_border_title_shows_the_issue_number() {
        let state = AppState::new(vec![issue(42, "Fix bug")], vec![]);
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("‹ #42 ›"),
            "pane border title should show the selected issue's number"
        );
    }

    #[test]
    fn pane_metadata_line_has_no_duplicate_issue_number() {
        let mut selected = issue(42, "Fix bug");
        selected.created_at = "2026-06-01T12:00:00Z".into();
        let state = AppState::new(vec![selected], vec![]);
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("opened 2026-06-01"),
            "metadata line should show the opened date"
        );
        assert!(
            !rendered.contains("#42 · opened"),
            "issue number should live in the border title, not repeated in the metadata line"
        );
    }

    #[test]
    fn pane_has_a_rule_between_metadata_and_body() {
        let mut selected = issue(1, "Fix bug");
        selected.body = "steps to repro".into();
        let state = AppState::new(vec![selected], vec![]);
        let rendered = render_to_string(&state);
        let lines: Vec<&str> = rendered.lines().collect();
        let metadata_line = lines
            .iter()
            .position(|line| line.contains("opened"))
            .expect("metadata line should be present");
        let body_line = lines
            .iter()
            .position(|line| line.contains("steps to repro"))
            .expect("body line should be present");
        let has_rule_between = lines[metadata_line + 1..body_line].iter().any(|line| {
            let interior = line.trim_matches(|c: char| c == ' ' || c == '│');
            !interior.is_empty() && interior.chars().all(|c| c == '─')
        });
        assert!(
            has_rule_between,
            "expected a rule line made entirely of '─' between the metadata line and the body"
        );
    }

    #[test]
    fn cursor_on_far_down_issue_stays_visible_in_small_viewport() {
        let issues: Vec<Issue> = (1..=50)
            .map(|n| issue(n, &format!("Issue number {n}")))
            .collect();
        let mut state = AppState::new(issues, vec![]);
        for _ in 0..40 {
            state.move_cursor(1);
        }
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("Issue number 41"),
            "selecting a far-down issue should scroll the viewport to keep its row visible"
        );
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
        let mut state = AppState::new(
            vec![],
            vec![Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
        );
        state.enter_big_create();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Title"));
        assert!(rendered.contains("Body"));
        assert!(rendered.contains("bug"));
    }

    #[test]
    fn form_renders_submit_control_and_footer_shortcuts() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Submit"));
        assert!(rendered.contains("ctrl+s submit"));
        assert!(rendered.contains("ctrl+w delete word"));
        assert!(rendered.contains("ctrl+u"));
        assert!(rendered.contains("clear"));
    }

    #[test]
    fn confirm_close_renders_issue_title_and_prompt() {
        let mut state = AppState::new(vec![issue(9, "Close me")], vec![]);
        state.request_close();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Close me"));
        assert!(rendered.contains("(y/n)"));
    }

    fn labels(names: &[&str]) -> Vec<Label> {
        names
            .iter()
            .map(|n| Label {
                name: n.to_string(),
                color: String::new(),
            })
            .collect()
    }

    #[test]
    fn label_palette_color_is_deterministic_and_within_palette() {
        let all = labels(&["bug", "enhancement"]);
        let first = label_palette_color(&all, "bug");
        let second = label_palette_color(&all, "bug");
        assert_eq!(
            first, second,
            "same label name must always map to the same color for a given label list"
        );
        assert!(LABEL_PALETTE.contains(&first));
    }

    #[test]
    fn distinct_labels_get_distinct_colors_within_palette_size() {
        // GitHub's own common defaults, the exact set a plain byte-sum hash
        // used to collide on ("bug", "enhancement", "question" all landed in
        // the same bucket).
        let all = labels(&[
            "bug",
            "enhancement",
            "question",
            "documentation",
            "good first issue",
        ]);
        let colors: Vec<Color> = all
            .iter()
            .map(|l| label_palette_color(&all, &l.name))
            .collect();
        let unique: std::collections::HashSet<_> = colors.iter().collect();
        assert_eq!(
            unique.len(),
            colors.len(),
            "with fewer labels than palette colors, none should collide"
        );
    }

    #[test]
    fn label_style_uses_colored_italic_text_with_no_background() {
        let style = label_style(Color::Cyan);
        assert_eq!(style.fg, Some(Color::Cyan));
        assert_eq!(
            style.bg, None,
            "label style must not set a background color"
        );
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn format_repo_title_is_a_passthrough_today() {
        assert_eq!(
            format_repo_title("jeffdt/issue-browser"),
            "jeffdt/issue-browser"
        );
    }

    #[test]
    fn border_title_shows_repo_name_when_available() {
        let mut state = AppState::new(vec![issue(1, "a")], vec![]);
        state.repo_name_with_owner = Some("jeffdt/issue-browser".to_string());
        let rendered = render_to_string(&state);
        assert!(rendered.contains("‹ jeffdt/issue-browser ›"));
    }

    #[test]
    fn border_title_falls_back_to_app_name_when_repo_unknown() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("‹ issue-browser ›"));
    }

    #[test]
    fn renders_outer_rounded_frame_with_title() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains('╭'));
        assert!(rendered.contains('╮'));
        assert!(rendered.contains('╰'));
        assert!(rendered.contains('╯'));
        assert!(rendered.contains("issue-browser"));
    }

    #[test]
    fn issue_titles_align_regardless_of_number_width() {
        let short = AppState::new(vec![issue(1, "Short number title")], vec![]);
        let long = AppState::new(vec![issue(123, "Long number title")], vec![]);
        let short_rendered = render_to_string(&short);
        let long_rendered = render_to_string(&long);
        let short_col = short_rendered
            .find("Short number title")
            .expect("short title rendered");
        let long_col = long_rendered
            .find("Long number title")
            .expect("long title rendered");
        let short_row_start = short_rendered[..short_col]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let long_row_start = long_rendered[..long_col]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        assert_eq!(short_col - short_row_start, long_col - long_row_start);
    }

    #[test]
    fn list_has_no_inner_border_around_state_filter_header() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let rendered = render_to_string(&state);
        let header_line = rendered
            .lines()
            .find(|line| line.contains("Issues ("))
            .expect("header line rendered");
        assert!(
            !header_line.contains('┌') && !header_line.contains('┐') && !header_line.contains('─'),
            "state filter header should be plain text with no surrounding inner box-drawing characters, got: {header_line:?}"
        );

        let mut box_glyph_lines = 0;
        for line in rendered.lines() {
            if line.contains('╭') || line.contains('╰') {
                box_glyph_lines += 1;
            }
        }
        // 4 = 2 corner rows for the outer app frame + 2 for the detail
        // pane's own border (shown by default), not a stray box around the
        // list header.
        assert_eq!(
            box_glyph_lines, 4,
            "expected exactly two boxes (outer frame + detail pane border), found evidence of an unexpected extra box"
        );
    }

    #[test]
    fn labels_right_align_to_same_column_regardless_of_title_length() {
        let label = Label {
            name: "bug".into(),
            color: "d73a4a".into(),
        };
        let mut short_issue = issue(1, "Short");
        short_issue.labels = vec![label.clone()];
        let mut long_issue = issue(2, "A very long issue title that takes up a lot of space");
        long_issue.labels = vec![label];

        let short_state = AppState::new(vec![short_issue], vec![]);
        let long_state = AppState::new(vec![long_issue], vec![]);

        let short_rendered = render_to_string(&short_state);
        let long_rendered = render_to_string(&long_state);

        let short_col = short_rendered
            .find("bug")
            .expect("label rendered for short title");
        let long_col = long_rendered
            .find("bug")
            .expect("label rendered for long title");

        let short_row_start = short_rendered[..short_col]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let long_row_start = long_rendered[..long_col]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        assert_eq!(
            short_col - short_row_start,
            long_col - long_row_start,
            "label badge should be right-aligned to the same column regardless of title length"
        );
    }

    #[test]
    fn form_body_field_wraps_long_text() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        state.form_next_field(); // Move to Body field
        let long_text = "word ".repeat(40);
        type_into_form(&mut state, &long_text);
        let rendered = render_to_string(&state);
        let body_section = rendered
            .lines()
            .skip_while(|line| !line.contains("Body"))
            .take_while(|line| !line.contains("Labels"))
            .collect::<Vec<_>>();
        assert!(
            body_section.len() > 3,
            "form body field with long unwrapped text should span multiple rows"
        );
    }

    #[test]
    fn long_status_message_renders_without_clipping() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        let long_message = "gh error: ".to_string() + &"x".repeat(100);
        state.set_status(long_message.clone());
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("gh error:"),
            "status message should be visible in rendered output"
        );
    }

    #[test]
    fn pending_create_renders_spinner_and_wait_hint() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        state.begin_pending(PendingOperation::CreateIssue);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Creating issue..."));
        assert!(rendered.contains("q quit"));
    }

    #[test]
    fn pending_create_spinner_is_visible_in_list_header() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        state.begin_pending(PendingOperation::CreateIssue);
        let rendered = render_to_string(&state);
        let header = rendered
            .lines()
            .find(|line| line.contains("Issues (Open)"))
            .expect("list header rendered");
        assert!(header.contains("Creating issue..."));
    }

    #[test]
    fn pending_edit_spinner_is_visible_in_list_header() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        state.begin_pending(PendingOperation::EditIssue);
        let rendered = render_to_string(&state);
        let header = rendered
            .lines()
            .find(|line| line.contains("Issues (Open)"))
            .expect("list header rendered");
        assert!(header.contains("Updating issue..."));
    }

    #[test]
    fn pending_close_shows_spinner_in_list_header_and_toast_only() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        state.begin_pending(PendingOperation::CloseIssue);
        let rendered = render_to_string(&state);
        assert_eq!(
            rendered.matches("Closing issue...").count(),
            2,
            "expected the spinner text once in the list header and once in the toast row, got: {rendered:?}"
        );
        let shortcuts_line = rendered
            .lines()
            .find(|line| line.contains("q quit"))
            .expect("shortcuts hint rendered");
        assert!(
            !shortcuts_line.contains("Closing issue..."),
            "shortcuts hint row should not repeat the spinner text"
        );
    }

    #[test]
    fn pending_edit_in_form_mode_shows_spinner_only_once() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        state.enter_edit();
        state.begin_pending(PendingOperation::EditIssue);
        let rendered = render_to_string(&state);
        let occurrences = rendered.matches("Updating issue...").count();
        assert_eq!(
            occurrences, 1,
            "spinner text should render exactly once, got: {rendered:?}"
        );
    }

    #[test]
    fn form_mode_renders_error_status_for_failed_submission() {
        let mut state = AppState::new(vec![issue(1, "Test issue")], vec![]);
        state.enter_big_create();
        state.set_status("gh error: create failed".to_string());
        let rendered = render_to_string(&state);
        assert!(rendered.contains("gh error: create failed"));
    }

    #[test]
    fn confirm_discard_renders_for_dirty_new_issue_form() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_big_create();
        type_into_form(&mut state, "T");
        state.cancel_form_or_create();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Discard this new issue?"));
    }

    #[test]
    fn confirm_discard_renders_issue_number_for_dirty_edit_form() {
        let mut state = AppState::new(vec![issue(9, "Fix bug")], vec![]);
        state.enter_edit();
        type_into_form(&mut state, "!");
        state.cancel_form_or_create();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Discard unsaved changes to #9?"));
    }

    #[test]
    fn confirm_close_with_long_title_shows_yn_prompt() {
        let long_title = "A very long issue title that might exceed the available width ".repeat(3);
        let mut state = AppState::new(vec![issue(42, &long_title)], vec![]);
        state.request_close();
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("(y/n)"),
            "confirm close dialog should show (y/n) prompt even with long title"
        );
    }

    #[test]
    fn truncate_title_returns_unchanged_when_it_fits() {
        assert_eq!(truncate_title("Short title", 20), "Short title");
    }

    #[test]
    fn truncate_title_returns_unchanged_at_exact_width() {
        assert_eq!(truncate_title("Exactly ten", 11), "Exactly ten");
    }

    #[test]
    fn truncate_title_appends_ellipsis_when_too_long() {
        let title = "abcdefghijklmnopqrstuvwxyz";
        assert_eq!(truncate_title(title, 10), "abcdefg...");
    }

    #[test]
    fn truncate_title_handles_tiny_width_without_panicking() {
        assert_eq!(truncate_title("Anything", 2), "..");
        assert_eq!(truncate_title("Anything", 0), "");
    }

    #[test]
    fn long_titles_are_truncated_with_ellipsis_in_the_list() {
        let long_title = "This issue title is intentionally far too long to fit inside the available list width without any truncation applied to it at all";
        let state = AppState::new(vec![issue(1, long_title)], vec![]);
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("..."),
            "long title should be truncated with an ellipsis, got: {rendered:?}"
        );
        assert!(
            !rendered.contains(long_title),
            "the full untruncated title should not appear in the list row"
        );
    }

    #[test]
    fn short_titles_are_not_truncated_in_the_list() {
        let state = AppState::new(vec![issue(1, "Short title")], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("Short title"));
        assert!(!rendered.contains("Short title..."));
    }

    #[test]
    fn detail_pane_shows_full_title_as_an_additional_occurrence() {
        let mut state = AppState::new(vec![issue(1, "Fix bug")], vec![]);
        let open_rendered = render_to_string(&state);
        assert_eq!(
            open_rendered.matches("Fix bug").count(),
            2,
            "title should appear twice (list + pane) by default"
        );
        state.toggle_pane();
        let closed_rendered = render_to_string(&state);
        assert_eq!(
            closed_rendered.matches("Fix bug").count(),
            1,
            "title should appear exactly once (in the list) once the pane is hidden"
        );
    }

    #[test]
    fn detail_pane_shows_untruncated_title_even_when_list_would_truncate_it() {
        // 70 chars: longer than the list's title budget at this fixed 80x24
        // TestBackend size (74-char inner width minus the 6-char number
        // column = 68), but short enough to fit on one line at the detail
        // pane's full width, so it renders unwrapped and unclipped.
        let long_title = "a".repeat(70);
        let mut state = AppState::new(vec![issue(1, &long_title)], vec![]);
        let pane_rendered = render_to_string(&state);
        assert!(
            pane_rendered.contains(&long_title),
            "detail pane should show the full untruncated title"
        );
        state.toggle_pane();
        let list_rendered = render_to_string(&state);
        assert!(
            !list_rendered.contains(&long_title),
            "list should truncate this title once the pane is hidden"
        );
    }

    #[test]
    fn list_mode_hint_mentions_h_to_hide_and_enter_or_e_to_edit() {
        let state = AppState::new(vec![issue(1, "Fix bug")], vec![]);
        let rendered = render_to_string(&state);
        assert!(rendered.contains("h hide pane"));
        assert!(rendered.contains("enter/e edit"));
    }

    #[test]
    fn borders_use_accent_color() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let buf = render_buffer(&state);
        let mut found_corner = false;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                if cell.symbol() == "╭" {
                    found_corner = true;
                    assert_eq!(
                        cell.style().fg,
                        Some(Color::Cyan),
                        "border corner at ({x}, {y}) should use the ACCENT color"
                    );
                }
            }
        }
        assert!(
            found_corner,
            "expected at least one rounded corner in the rendered frame"
        );
    }

    #[test]
    fn blank_spacer_row_appears_between_border_and_header() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let rendered = render_to_string(&state);
        let lines: Vec<&str> = rendered.lines().collect();
        let top_border_row = lines
            .iter()
            .position(|line| line.contains('╭'))
            .expect("outer frame's top border should render");
        let header_row = lines
            .iter()
            .position(|line| line.contains("Issues ("))
            .expect("list header should render");
        assert_eq!(
            header_row,
            top_border_row + 2,
            "expected exactly one blank row between the top border ({top_border_row}) and the header ({header_row})"
        );
        let spacer_row = lines[top_border_row + 1];
        let interior = spacer_row.trim_matches(|c: char| c == ' ' || c == '│');
        assert!(
            interior.is_empty(),
            "expected the row between the top border and the header to be blank, got: {spacer_row:?}"
        );
    }

    #[test]
    fn pane_metadata_and_rule_use_dim_color_not_dim_modifier() {
        let mut selected = issue(1, "Fix bug");
        selected.created_at = "2026-06-01T12:00:00Z".into();
        let state = AppState::new(vec![selected], vec![]);
        let buf = render_buffer(&state);

        let (mx, my) =
            find_in_buffer(&buf, "opened 2026-06-01").expect("metadata line should render");
        let meta_style = buf[(mx, my)].style();
        assert_eq!(meta_style.fg, Some(Color::DarkGray));
        assert!(!meta_style.add_modifier.contains(Modifier::DIM));

        let rule_y = my + 1;
        let mut rule_x = None;
        for x in 0..buf.area.width {
            if buf[(x, rule_y)].symbol() == "─" {
                rule_x = Some(x);
                break;
            }
        }
        let rx = rule_x.expect("pane rule row should contain '─' characters");
        let rule_style = buf[(rx, rule_y)].style();
        assert_eq!(rule_style.fg, Some(Color::DarkGray));
        assert!(!rule_style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn footer_hint_keys_render_gray_and_descriptions_stay_dim() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let buf = render_buffer(&state);
        let (kx, ky) = find_in_buffer(&buf, "j/k").expect("footer hint should render");
        let key_style = buf[(kx, ky)].style();
        assert_eq!(
            key_style.fg,
            Some(Color::Gray),
            "key token should render Gray"
        );
        assert!(
            !key_style.add_modifier.contains(Modifier::BOLD),
            "key token should not be bold"
        );
        let (dx, dy) = find_in_buffer(&buf, "move").expect("footer hint description should render");
        let desc_style = buf[(dx, dy)].style();
        assert_eq!(
            desc_style.fg,
            Some(Color::DarkGray),
            "description should stay dim"
        );
    }

    #[test]
    fn footer_hint_segments_join_with_middot() {
        let state = AppState::new(vec![issue(1, "a")], vec![]);
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("j/k move · h hide pane"),
            "footer hint segments should join with a middot separator, got: {rendered:?}"
        );
    }

    #[test]
    fn little_create_shows_new_issue_title_with_repo_when_known() {
        let mut state = AppState::new(vec![], vec![]);
        state.repo_name_with_owner = Some("jeffdt/issue-browser".to_string());
        state.enter_little_create();
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("New issue in jeffdt/issue-browser"),
            "title should include the known repo, got: {rendered:?}"
        );
    }

    #[test]
    fn little_create_falls_back_to_plain_title_when_repo_unknown() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("New issue"),
            "title should fall back to a plain label, got: {rendered:?}"
        );
        assert!(
            !rendered.contains("New issue in"),
            "title should not claim a repo it doesn't have, got: {rendered:?}"
        );
    }

    #[test]
    fn little_create_border_is_rounded_and_accent_colored() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        let buf = render_buffer(&state);
        let (x, y) = find_in_buffer(&buf, "New issue").expect("little-create title should render");
        let mut found = false;
        for cx in 0..=x {
            let cell = &buf[(cx, y)];
            if cell.symbol() == "╭" || cell.symbol() == "─" {
                assert_eq!(cell.style().fg, Some(Color::Cyan));
                found = true;
            }
        }
        assert!(
            found,
            "expected the little-create block's rounded top border to render"
        );
    }

    #[test]
    fn little_create_box_is_four_rows_tall_regardless_of_popup_height() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        let buf = render_buffer(&state);
        // TestBackend is 80x24; the box+footer area must not stretch past 4 rows.
        // No top margin, and the hint/toast rows are merged into one footer row:
        // content occupies rows 0-3 (4 rows), leaving rows 4+ blank.
        for y in 4..buf.area.height {
            for x in 0..buf.area.width {
                assert_eq!(
                    buf[(x, y)].symbol(),
                    " ",
                    "row {y} col {x} should be blank below the 4-row quick-create area, found {:?}",
                    buf[(x, y)].symbol()
                );
            }
        }
    }

    #[test]
    fn little_create_hint_row_shows_enter_and_esc() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        let rendered = render_to_string(&state);
        assert!(rendered.contains("enter create · esc cancel"));
    }

    #[test]
    fn little_create_footer_shows_pending_message_instead_of_hint() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        state.begin_pending(crate::model::PendingOperation::CreateIssue);
        let rendered = render_to_string(&state);
        assert!(
            rendered.contains("Creating issue"),
            "footer should show the pending message, got: {rendered:?}"
        );
        assert!(
            !rendered.contains("enter create"),
            "footer should not show the hint while a message is pending, got: {rendered:?}"
        );
    }

    #[test]
    fn little_create_sits_flush_at_the_top_with_no_margin() {
        let mut state = AppState::new(vec![], vec![]);
        state.enter_little_create();
        let buf = render_buffer(&state);
        let (_, y) =
            find_in_buffer(&buf, "New issue").expect("little-create title should render");
        assert_eq!(
            y, 0,
            "quick-create is a deliberately distinct compact screen and should sit flush \
             at the top with no margin row above it, unlike the rest of the app"
        );
    }

    #[test]
    fn confirm_close_border_uses_accent_color() {
        let mut state = AppState::new(vec![issue(9, "Close me")], vec![]);
        state.request_close();
        let buf = render_buffer(&state);
        let (x, y) = find_in_buffer(&buf, "Confirm").expect("confirm dialog title should render");
        let mut found = false;
        for cx in 0..=x {
            let cell = &buf[(cx, y)];
            if cell.symbol() == "┌" || cell.symbol() == "─" {
                assert_eq!(cell.style().fg, Some(Color::Cyan));
                found = true;
            }
        }
        assert!(found, "expected the confirm dialog's top border to render");
    }
}
