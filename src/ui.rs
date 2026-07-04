use crate::model::FormField;
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
    Exit,
    None,
}

pub fn map_search_key(key: KeyEvent) -> SearchInput {
    match key.code {
        KeyCode::Char(c) => SearchInput::Char(c),
        KeyCode::Backspace => SearchInput::Backspace,
        KeyCode::Enter | KeyCode::Esc => SearchInput::Exit,
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
}
