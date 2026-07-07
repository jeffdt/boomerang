use crate::model::{AppState, LoadingAnimation};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::time::Duration;

const MATRIX_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#@$%&?";

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    style: Style,
}

impl Cell {
    fn blank() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
    let mut header = format!("Issues ({:?})", state.state_filter);
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

fn draw_animation(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(loading) = state.loading.as_ref() else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    let elapsed = loading.started_at.elapsed();
    match loading.animation {
        LoadingAnimation::MatrixRain => draw_matrix_rain(frame, area, elapsed),
        LoadingAnimation::ColorRipple => draw_color_ripple(frame, area, elapsed),
    }
}

fn draw_matrix_rain(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 70) as usize;
    let height = area.height as usize;
    let width = area.width as usize;
    let mut canvas = blank_canvas(width, height);
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = matrix_rain_cell(x, y, width, height, tick) {
                put_cell(&mut canvas, x as isize, y as isize, cell.ch, cell.style);
            }
        }
    }
    render_canvas(frame, area, canvas);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MatrixColumn {
    speed: usize,
    base_tail: usize,
    gap: usize,
    phase: usize,
}

impl MatrixColumn {
    fn cycle_len(&self, height: usize) -> usize {
        height + self.base_tail + self.gap
    }
}

fn matrix_rain_cell(x: usize, y: usize, width: usize, height: usize, tick: usize) -> Option<Cell> {
    let column = matrix_column(x, width, height)?;
    let local_tick = tick / column.speed + column.phase;
    let generation = local_tick / column.cycle_len(height);
    let tail = matrix_tail_len(x, height, generation, column.base_tail);
    let head = (local_tick % column.cycle_len(height)) as isize - tail as isize;
    let distance = head - y as isize;
    if !(0..=tail as isize).contains(&distance) {
        return None;
    }
    let char_tick = tick / (column.speed + 1);
    let index = matrix_hash(x as u64, y as u64, char_tick as u64, generation as u64)
        % MATRIX_CHARS.len() as u64;
    Some(Cell {
        ch: MATRIX_CHARS[index as usize] as char,
        style: matrix_rain_style(distance as usize, tail),
    })
}

fn matrix_column(x: usize, width: usize, height: usize) -> Option<MatrixColumn> {
    if width == 0 || height == 0 {
        return None;
    }
    let seed = matrix_hash(x as u64, width as u64, height as u64, 0);
    let density = (width / 20).clamp(4, 7) as u64;
    if seed % 10 >= density {
        return None;
    }
    let max_tail = height.clamp(4, 14);
    let base_tail = 4 + ((seed >> 8) as usize % max_tail);
    let speed = 1 + ((seed >> 20) as usize % 3);
    let gap = height / 2 + ((seed >> 32) as usize % height.max(1)) + speed * 2;
    Some(MatrixColumn {
        speed,
        base_tail,
        gap,
        phase: (seed >> 44) as usize % (height + base_tail + gap),
    })
}

fn matrix_tail_len(x: usize, height: usize, generation: usize, base_tail: usize) -> usize {
    let max_tail = height.clamp(4, 14);
    let variation = matrix_hash(x as u64, generation as u64, height as u64, 1) as usize % max_tail;
    (base_tail / 2 + variation).clamp(3, height + 6)
}

fn matrix_rain_style(distance: usize, tail: usize) -> Style {
    if distance == 0 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else if distance <= 2 {
        Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD)
    } else if distance <= tail / 2 {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    }
}

fn draw_color_ripple(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 60) as usize;
    let height = area.height as usize;
    let width = area.width as usize;
    let mut canvas = blank_canvas(width, height);
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = ripple_cell(x, y, width, height, tick) {
                put_cell(&mut canvas, x as isize, y as isize, cell.ch, cell.style);
            }
        }
    }
    render_canvas(frame, area, canvas);
}

fn ripple_cell(x: usize, y: usize, width: usize, height: usize, tick: usize) -> Option<Cell> {
    if width == 0 || height == 0 {
        return None;
    }
    let max_radius = ripple_max_radius(width, height);
    let radius = tick % (max_radius + 8);
    if radius > max_radius + 2 {
        return None;
    }
    let center_x = width as isize / 2;
    let center_y = height as isize / 2;
    let dx = x as isize - center_x;
    let dy = (y as isize - center_y) * 2;
    let distance = (((dx * dx + dy * dy) as f64).sqrt().round()) as isize;
    if radius <= 1 && distance <= 1 {
        return Some(Cell {
            ch: '●',
            style: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        });
    }
    let delta = (distance - radius as isize).abs();
    match delta {
        0 => Some(Cell {
            ch: '●',
            style: Style::default()
                .fg(ripple_color(radius))
                .add_modifier(Modifier::BOLD),
        }),
        1 => Some(Cell {
            ch: '·',
            style: Style::default().fg(Color::DarkGray),
        }),
        _ => None,
    }
}

fn ripple_max_radius(width: usize, height: usize) -> usize {
    let half_width = width as f64 / 2.0;
    let half_height = height as f64;
    (half_width
        .mul_add(half_width, half_height * half_height)
        .sqrt()
        .ceil()) as usize
}

fn ripple_color(radius: usize) -> Color {
    match (radius / 4) % 3 {
        0 => Color::Cyan,
        1 => Color::Blue,
        _ => Color::Magenta,
    }
}

fn matrix_hash(a: u64, b: u64, c: u64, d: u64) -> u64 {
    let mut value = a.wrapping_mul(0x9E37_79B1_85EB_CA87)
        ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ c.wrapping_mul(0x1656_67B1_9E37_79F9)
        ^ d.wrapping_mul(0x85EB_CA77_C2B2_AE63);
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^ (value >> 33)
}

fn blank_canvas(width: usize, height: usize) -> Vec<Vec<Cell>> {
    vec![vec![Cell::blank(); width]; height]
}

fn put_cell(canvas: &mut [Vec<Cell>], x: isize, y: isize, ch: char, style: Style) {
    let Some(row) = usize::try_from(y).ok().and_then(|row| canvas.get_mut(row)) else {
        return;
    };
    let Some(cell) = usize::try_from(x).ok().and_then(|col| row.get_mut(col)) else {
        return;
    };
    *cell = Cell { ch, style };
}

fn render_canvas(frame: &mut Frame, area: Rect, canvas: Vec<Vec<Cell>>) {
    let lines = canvas
        .into_iter()
        .map(|row| {
            Line::from(
                row.into_iter()
                    .map(|cell| Span::styled(cell.ch.to_string(), cell.style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_columns_vary_speed_tail_and_gap() {
        let columns = (0..80)
            .filter_map(|x| matrix_column(x, 80, 18))
            .collect::<Vec<_>>();
        assert!(
            columns.len() >= 25,
            "matrix rain should keep a rich column density at normal popup width"
        );
        assert!(
            columns.iter().any(|column| column.speed == 1)
                && columns.iter().any(|column| column.speed > 1),
            "columns should not all fall at the same speed"
        );
        assert!(
            columns
                .windows(2)
                .any(|pair| pair[0].base_tail != pair[1].base_tail),
            "columns should not all share one tail length"
        );
        assert!(
            columns.windows(2).any(|pair| pair[0].gap != pair[1].gap),
            "columns should not all share one respawn gap"
        );
    }

    #[test]
    fn matrix_tail_length_changes_between_generations() {
        let x = (0..80)
            .find(|&x| matrix_column(x, 80, 18).is_some())
            .expect("active matrix column");
        let column = matrix_column(x, 80, 18).expect("active matrix column");
        let first = matrix_tail_len(x, 18, 0, column.base_tail);
        assert!(
            (1..20).any(|generation| matrix_tail_len(x, 18, generation, column.base_tail) != first),
            "respawned matrix columns should vary tail length across generations"
        );
    }

    #[test]
    fn matrix_cells_churn_glyphs_while_using_head_and_tail_styles() {
        let mut saw_head = false;
        let mut saw_tail = false;
        let mut saw_churn = false;
        for x in 0..80 {
            for y in 0..18 {
                let first = matrix_rain_cell(x, y, 80, 18, 120);
                let later = matrix_rain_cell(x, y, 80, 18, 126);
                if let Some(cell) = first {
                    saw_head |= cell.style.fg == Some(Color::White)
                        && cell.style.add_modifier.contains(Modifier::BOLD);
                    saw_tail |= cell.style.fg == Some(Color::DarkGray)
                        && cell.style.add_modifier.contains(Modifier::DIM);
                    if let Some(later) = later {
                        saw_churn |= cell.ch != later.ch;
                    }
                }
            }
        }
        assert!(saw_head, "matrix rain should render bright heads");
        assert!(saw_tail, "matrix rain should render dim fading tails");
        assert!(
            saw_churn,
            "matrix rain glyphs should mutate inside existing trails"
        );
    }

    #[test]
    fn ripple_emits_one_clean_centered_ring() {
        let center = ripple_cell(40, 9, 80, 18, 0).expect("center ripple cell");
        assert_eq!(center.ch, '●');
        assert_eq!(center.style.fg, Some(Color::White));
        assert!(center.style.add_modifier.contains(Modifier::BOLD));

        let ring = ripple_cell(46, 9, 80, 18, 6).expect("single ripple ring");
        assert_eq!(ring.ch, '●');
        assert_eq!(ring.style.fg, Some(Color::Blue));

        assert_eq!(
            ripple_cell(60, 9, 80, 18, 6).map(|cell| cell.ch),
            None,
            "ripple should render one ring, not repeated bullseye bands"
        );
    }

    #[test]
    fn ripple_ring_moves_outward_over_time() {
        let first = ripple_cell(40, 9, 80, 18, 0).expect("center ripple cell");
        let later = ripple_cell(40, 9, 80, 18, 2);
        assert!(
            later.is_none_or(|cell| cell.ch != first.ch || cell.style.fg != first.style.fg),
            "ripple center should change as the wave passes"
        );
        assert!(
            ripple_cell(42, 9, 80, 18, 2).is_some(),
            "ripple should move outward from the center"
        );
    }
}
