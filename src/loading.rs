use crate::model::{AppState, LoadingAnimation};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
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
        Paragraph::new("q quit").wrap(ratatui::widgets::Wrap { trim: false }),
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
        LoadingAnimation::Pipes => draw_pipes(frame, area, elapsed),
        LoadingAnimation::Starfield => draw_starfield(frame, area, elapsed),
        LoadingAnimation::BlackHole => draw_black_hole(frame, area, elapsed),
        LoadingAnimation::BonsaiSprout => draw_bonsai_sprout(frame, area, elapsed),
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

fn draw_pipes(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 80) as usize;
    let height = area.height as usize;
    let width = area.width as usize;
    let pipe_count = (width / 12).clamp(4, 9);
    let segment_count = ((width + height) / 2).clamp(24, 64);
    let mut canvas = blank_canvas(width, height);
    for pipe in 0..pipe_count {
        let mut x = ((pipe * 11 + tick / 2) % width) as isize;
        let mut y = ((pipe * 7 + tick / 3) % height) as isize;
        let mut direction = (pipe + tick / 17) % 4;
        let style = pipe_style(pipe);
        for step in 0..segment_count {
            let previous = direction;
            let turn = (pipe * 3 + step * 5 + tick / 7) % 10;
            if turn == 0 {
                direction = (direction + 1) % 4;
            } else if turn == 1 {
                direction = (direction + 3) % 4;
            }
            put_cell(&mut canvas, x, y, pipe_glyph(previous, direction), style);
            match direction {
                0 => y = (y + height as isize - 1) % height as isize,
                1 => x = (x + 1) % width as isize,
                2 => y = (y + 1) % height as isize,
                _ => x = (x + width as isize - 1) % width as isize,
            }
        }
    }
    render_canvas(frame, area, canvas);
}

fn draw_starfield(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 70) as isize;
    let height = area.height as usize;
    let width = area.width as usize;
    let center_x = width as isize / 2;
    let center_y = height as isize / 2;
    let mut canvas = blank_canvas(width, height);
    for y in 0..height {
        for x in 0..width {
            let dx = x as isize - center_x;
            let dy = y as isize - center_y;
            let distance = dx * dx + dy * dy;
            let shimmer = (distance + tick * 9 + x as isize * 3 + y as isize * 11).rem_euclid(61);
            if x as isize == center_x && y as isize == center_y {
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    '✦',
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            } else if shimmer < 3 {
                let (ch, color) = if distance < 30 {
                    ('·', Color::White)
                } else if distance < 160 {
                    ('*', Color::Cyan)
                } else {
                    ('+', Color::Blue)
                };
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    ch,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                );
            } else if shimmer < 6 {
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    '.',
                    Style::default().fg(Color::DarkGray),
                );
            }
        }
    }
    render_canvas(frame, area, canvas);
}

fn draw_black_hole(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 80) as isize;
    let height = area.height as usize;
    let width = area.width as usize;
    let center_x = width as isize / 2;
    let center_y = height as isize / 2;
    let mut canvas = blank_canvas(width, height);
    for y in 0..height {
        for x in 0..width {
            let dx = x as isize - center_x;
            let dy = y as isize - center_y;
            let distance = dx * dx + dy * dy;
            let swirl =
                (distance + tick * 5 + x as isize * y as isize + dx * 7 - dy * 3).rem_euclid(67);
            if distance < 7 {
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    '@',
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            } else if distance < 18 {
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    '#',
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                );
            } else if swirl < 4 {
                let index = (x * 13 + y * 5 + tick as usize) % MATRIX_CHARS.len();
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    MATRIX_CHARS[index] as char,
                    Style::default().fg(Color::Magenta),
                );
            } else if swirl < 8 {
                put_cell(
                    &mut canvas,
                    x as isize,
                    y as isize,
                    '.',
                    Style::default().fg(Color::Blue),
                );
            }
        }
    }
    render_canvas(frame, area, canvas);
}

fn draw_bonsai_sprout(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 120) as usize;
    let height = area.height as usize;
    let width = area.width as usize;
    let mut canvas = blank_canvas(width, height);
    let base_y = height.saturating_sub(2) as isize;
    let trunk_x = width as isize / 2;
    draw_pot(&mut canvas, trunk_x, height.saturating_sub(1) as isize);
    let max_trunk = height.saturating_sub(4).min(12) as isize;
    let growth = tick % (max_trunk.max(1) as usize + 26);
    let trunk_height = growth.min(max_trunk.max(0) as usize) as isize;
    for step in 0..=trunk_height {
        put_cell(
            &mut canvas,
            trunk_x,
            base_y - step,
            '│',
            Style::default().fg(Color::Yellow),
        );
    }
    let branch_specs = [(2, -1, 5), (4, 1, 5), (7, -1, 7), (9, 1, 6)];
    for (start, direction, length) in branch_specs {
        draw_branch(
            &mut canvas,
            trunk_x,
            base_y,
            growth,
            start,
            direction,
            length,
        );
    }
    if trunk_height == max_trunk && max_trunk > 0 {
        draw_leaf_cluster(&mut canvas, trunk_x, base_y - max_trunk, tick, 0);
    }
    render_canvas(frame, area, canvas);
}

fn draw_pot(canvas: &mut [Vec<Cell>], center_x: isize, y: isize) {
    let style = Style::default().fg(Color::Blue);
    for (offset, ch) in [(-2, '╰'), (-1, '─'), (0, '┴'), (1, '─'), (2, '╯')] {
        put_cell(canvas, center_x + offset, y, ch, style);
    }
}

fn draw_branch(
    canvas: &mut [Vec<Cell>],
    trunk_x: isize,
    base_y: isize,
    growth: usize,
    start: isize,
    direction: isize,
    length: isize,
) {
    if growth < start as usize {
        return;
    }
    let branch_growth = (growth - start as usize).min(length as usize) as isize;
    let branch_y = base_y - start;
    let glyph = if direction < 0 { '╱' } else { '╲' };
    let style = Style::default().fg(Color::Yellow);
    for step in 1..=branch_growth {
        put_cell(
            canvas,
            trunk_x + direction * step,
            branch_y - step / 2,
            glyph,
            style,
        );
    }
    if growth > (start + length) as usize {
        draw_leaf_cluster(
            canvas,
            trunk_x + direction * length,
            branch_y - length / 2,
            growth,
            start as usize,
        );
    }
}

fn draw_leaf_cluster(
    canvas: &mut [Vec<Cell>],
    center_x: isize,
    center_y: isize,
    tick: usize,
    seed: usize,
) {
    let leaves: [(isize, isize, char); 6] = [
        (0, 0, '*'),
        (-1, 0, 'o'),
        (1, 0, 'o'),
        (0, -1, '*'),
        (-1, -1, '·'),
        (1, -1, '·'),
    ];
    for (index, (dx, dy, ch)) in leaves.into_iter().enumerate() {
        if (tick + seed + index) % 3 == 0 && ch == '·' {
            continue;
        }
        let color = match (tick + seed + index) % 4 {
            0 => Color::Green,
            1 => Color::Cyan,
            2 => Color::Magenta,
            _ => Color::Green,
        };
        put_cell(
            canvas,
            center_x + dx,
            center_y + dy,
            ch,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        );
    }
}

fn pipe_style(index: usize) -> Style {
    let color = match index % 6 {
        0 => Color::Cyan,
        1 => Color::Magenta,
        2 => Color::Yellow,
        3 => Color::Green,
        4 => Color::Blue,
        _ => Color::Red,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn pipe_glyph(previous: usize, next: usize) -> char {
    match (previous, next) {
        (0, 0) | (2, 2) => '│',
        (1, 1) | (3, 3) => '─',
        (0, 1) | (3, 2) => '┌',
        (1, 2) | (0, 3) => '┐',
        (2, 1) | (3, 0) => '└',
        (1, 0) | (2, 3) => '┘',
        _ => '┼',
    }
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
}
