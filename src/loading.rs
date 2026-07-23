use crate::model::AppState;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::time::Duration;

pub fn draw(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
    let (prefix, repo) = state.issues_header_parts();
    let mut header = match repo {
        Some(repo) => format!("{prefix} in {repo}"),
        None => prefix,
    };
    if let Some(loading) = state.loading_message() {
        header.push_str("  ");
        header.push_str(&loading);
    }
    frame.render_widget(
        Paragraph::new(header).style(Style::default().add_modifier(Modifier::DIM)),
        chunks[0],
    );
    draw_animation(frame, chunks[1], state);
    frame.render_widget(
        Paragraph::new("q quit").wrap(Wrap { trim: false }),
        chunks[2],
    );
}

const PULSE_FRAMES: [&str; 12] = ["▁", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃"];
const PULSE_FRAME_INTERVAL_MS: u128 = 80;

fn draw_animation(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(loading) = state.loading.as_ref() else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    draw_pulse_spinner(frame, area, loading.started_at.elapsed());
}

fn draw_pulse_spinner(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let glyph = PULSE_FRAMES[pulse_frame_index(elapsed)];
    let line = Line::from(Span::styled(glyph, Style::default().fg(Color::Cyan)));
    let centered_row = Rect {
        x: area.x,
        y: area.y + area.height / 2,
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(line).alignment(Alignment::Center),
        centered_row,
    );
}

fn pulse_frame_index(elapsed: Duration) -> usize {
    ((elapsed.as_millis() / PULSE_FRAME_INTERVAL_MS) as usize) % PULSE_FRAMES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_frame_index_advances_one_frame_per_interval() {
        let indices: Vec<usize> = (0..PULSE_FRAMES.len())
            .map(|i| {
                pulse_frame_index(Duration::from_millis(
                    i as u64 * PULSE_FRAME_INTERVAL_MS as u64,
                ))
            })
            .collect();
        assert_eq!(indices, (0..PULSE_FRAMES.len()).collect::<Vec<_>>());
    }

    #[test]
    fn pulse_frame_index_wraps_after_a_full_cycle() {
        let cycle_ms = PULSE_FRAME_INTERVAL_MS as u64 * PULSE_FRAMES.len() as u64;
        assert_eq!(
            pulse_frame_index(Duration::from_millis(cycle_ms)),
            pulse_frame_index(Duration::ZERO)
        );
    }

    #[test]
    fn pulse_frame_index_holds_steady_within_an_interval() {
        let start = pulse_frame_index(Duration::from_millis(10));
        let later = pulse_frame_index(Duration::from_millis(PULSE_FRAME_INTERVAL_MS as u64 - 1));
        assert_eq!(start, later);
    }
}
