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
    let (prefix, label, repo) = state.issues_header_parts();
    let mut header = prefix;
    if let Some(name) = label {
        header.push_str(&format!(" · label: {name}"));
    }
    if let Some(repo) = repo {
        header.push_str(&format!(" in {repo}"));
    }
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
const PULSE_BAR_COUNT: usize = 7;
const PULSE_BAR_INTERVAL_STEP_MS: u128 = 15;

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
    let mut spans = Vec::with_capacity(PULSE_BAR_COUNT * 2 - 1);
    for bar in 0..PULSE_BAR_COUNT {
        if bar > 0 {
            spans.push(Span::raw(" "));
        }
        let glyph = PULSE_FRAMES[pulse_frame_index(elapsed, bar)];
        spans.push(Span::styled(glyph, Style::default().fg(Color::Cyan)));
    }
    let line = Line::from(spans);
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

fn pulse_bar_interval_ms(bar: usize) -> u128 {
    PULSE_FRAME_INTERVAL_MS + bar as u128 * PULSE_BAR_INTERVAL_STEP_MS
}

fn pulse_frame_index(elapsed: Duration, bar: usize) -> usize {
    ((elapsed.as_millis() / pulse_bar_interval_ms(bar)) as usize) % PULSE_FRAMES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_frame_index_advances_one_frame_per_interval() {
        let indices: Vec<usize> = (0..PULSE_FRAMES.len())
            .map(|i| {
                pulse_frame_index(
                    Duration::from_millis(i as u64 * PULSE_FRAME_INTERVAL_MS as u64),
                    0,
                )
            })
            .collect();
        assert_eq!(indices, (0..PULSE_FRAMES.len()).collect::<Vec<_>>());
    }

    #[test]
    fn pulse_frame_index_wraps_after_a_full_cycle() {
        let cycle_ms = PULSE_FRAME_INTERVAL_MS as u64 * PULSE_FRAMES.len() as u64;
        assert_eq!(
            pulse_frame_index(Duration::from_millis(cycle_ms), 0),
            pulse_frame_index(Duration::ZERO, 0)
        );
    }

    #[test]
    fn pulse_frame_index_holds_steady_within_an_interval() {
        let start = pulse_frame_index(Duration::from_millis(10), 0);
        let later = pulse_frame_index(
            Duration::from_millis(PULSE_FRAME_INTERVAL_MS as u64 - 1),
            0,
        );
        assert_eq!(start, later);
    }

    #[test]
    fn pulse_bars_run_at_distinct_frequencies() {
        let intervals: Vec<u128> = (0..PULSE_BAR_COUNT).map(pulse_bar_interval_ms).collect();
        let mut sorted = intervals.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            PULSE_BAR_COUNT,
            "every bar should have a unique frequency, got: {intervals:?}"
        );
    }

    #[test]
    fn pulse_bars_desync_over_time() {
        let elapsed = Duration::from_millis(500);
        let indices: Vec<usize> = (0..PULSE_BAR_COUNT)
            .map(|bar| pulse_frame_index(elapsed, bar))
            .collect();
        let all_same = indices.iter().all(|&i| i == indices[0]);
        assert!(
            !all_same,
            "bars with different frequencies should show different frames after 500ms, got: {indices:?}"
        );
    }
}
