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
const PULSE_BASE_INTERVAL_MS: u128 = 80;
const PULSE_INTERVAL_JITTER_MS: u64 = 90;
const PULSE_BAR_COUNT: usize = 7;

fn draw_animation(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(loading) = state.loading.as_ref() else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    draw_pulse_spinner(frame, area, loading.started_at.elapsed(), loading.seed);
}

fn draw_pulse_spinner(frame: &mut Frame, area: Rect, elapsed: Duration, seed: u64) {
    let mut spans = Vec::with_capacity(PULSE_BAR_COUNT * 2 - 1);
    for bar in 0..PULSE_BAR_COUNT {
        if bar > 0 {
            spans.push(Span::raw(" "));
        }
        let glyph = PULSE_FRAMES[pulse_frame_index(elapsed, seed, bar)];
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

/// A bar's randomized starting phase, speed, and direction through
/// `PULSE_FRAMES`, derived from the loading session's `seed` and the bar's
/// index so every bar moves independently but stays reproducible for a
/// given (seed, bar) pair (needed for deterministic tests).
struct BarMotion {
    phase: i64,
    direction: i64,
    interval_ms: u128,
}

fn bar_motion(seed: u64, bar: usize) -> BarMotion {
    let hash = mix64(seed ^ (bar as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let phase = (hash % PULSE_FRAMES.len() as u64) as i64;
    let direction = if (hash >> 16) & 1 == 0 { 1 } else { -1 };
    let interval_ms = PULSE_BASE_INTERVAL_MS + ((hash >> 32) % PULSE_INTERVAL_JITTER_MS) as u128;
    BarMotion {
        phase,
        direction,
        interval_ms,
    }
}

/// MurmurHash3's 64-bit finalizer, used only to spread a small integer seed
/// across a `u64` well enough to derive independent-looking bar motion.
/// Not cryptographic, just decorrelated - no crate needed for that.
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

fn pulse_frame_index(elapsed: Duration, seed: u64, bar: usize) -> usize {
    let motion = bar_motion(seed, bar);
    let ticks = (elapsed.as_millis() / motion.interval_ms) as i64;
    let raw = motion.phase + motion.direction * ticks;
    raw.rem_euclid(PULSE_FRAMES.len() as i64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 1_753_000_000_000;

    #[test]
    fn pulse_frame_index_holds_steady_within_an_interval() {
        let motion = bar_motion(SEED, 0);
        let start = pulse_frame_index(Duration::from_millis(10), SEED, 0);
        let later =
            pulse_frame_index(Duration::from_millis(motion.interval_ms as u64 - 1), SEED, 0);
        assert_eq!(start, later);
    }

    #[test]
    fn pulse_frame_index_advances_by_direction_each_interval() {
        let motion = bar_motion(SEED, 0);
        let interval = motion.interval_ms as u64;
        let first = pulse_frame_index(Duration::from_millis(0), SEED, 0);
        let second = pulse_frame_index(Duration::from_millis(interval), SEED, 0);
        let expected = (motion.phase + motion.direction).rem_euclid(PULSE_FRAMES.len() as i64);
        assert_eq!(first, motion.phase as usize);
        assert_eq!(second, expected as usize);
    }

    #[test]
    fn bar_motion_is_reproducible_for_the_same_seed_and_bar() {
        let a = bar_motion(SEED, 3);
        let b = bar_motion(SEED, 3);
        assert_eq!(a.phase, b.phase);
        assert_eq!(a.direction, b.direction);
        assert_eq!(a.interval_ms, b.interval_ms);
    }

    #[test]
    fn bar_motion_varies_across_bars_for_the_same_seed() {
        let motions: Vec<(i64, i64, u128)> = (0..PULSE_BAR_COUNT)
            .map(|bar| {
                let m = bar_motion(SEED, bar);
                (m.phase, m.direction, m.interval_ms)
            })
            .collect();
        let unique: std::collections::HashSet<_> = motions.iter().collect();
        assert!(
            unique.len() > 1,
            "bars should not all share identical phase/direction/speed, got: {motions:?}"
        );
    }

    #[test]
    fn bar_motion_varies_across_seeds_for_the_same_bar() {
        let a = bar_motion(SEED, 0);
        let b = bar_motion(SEED + 1, 0);
        assert!(
            a.phase != b.phase || a.direction != b.direction || a.interval_ms != b.interval_ms,
            "different loading sessions should not always produce identical bar motion"
        );
    }

    #[test]
    fn pulse_bars_desync_over_time() {
        let elapsed = Duration::from_millis(500);
        let indices: Vec<usize> = (0..PULSE_BAR_COUNT)
            .map(|bar| pulse_frame_index(elapsed, SEED, bar))
            .collect();
        let all_same = indices.iter().all(|&i| i == indices[0]);
        assert!(
            !all_same,
            "bars with independent motion should show different frames after 500ms, got: {indices:?}"
        );
    }
}
