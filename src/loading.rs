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
    let mut header = state.issues_header();
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
        LoadingAnimation::RainbowRipple => draw_rainbow_ripple(frame, area, elapsed),
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

const RIPPLE_COUNT: usize = 3;
const RIPPLE_CORE_WIDTH: isize = 1;
const RIPPLE_HALO_WIDTH: isize = 3;

fn ripple_cell(x: usize, y: usize, width: usize, height: usize, tick: usize) -> Option<Cell> {
    if width == 0 || height == 0 {
        return None;
    }
    let max_radius = ripple_max_radius(width, height);
    let center_x = width as isize / 2;
    let center_y = height as isize / 2;
    let dx = x as isize - center_x;
    let dy = (y as isize - center_y) * 2;
    let distance = (((dx * dx + dy * dy) as f64).sqrt().round()) as isize;
    let (delta, radius) = closest_ripple(distance, max_radius, tick)?;
    if radius <= 1 && distance <= RIPPLE_CORE_WIDTH {
        return Some(Cell {
            ch: '●',
            style: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        });
    }
    if delta <= RIPPLE_CORE_WIDTH {
        Some(Cell {
            ch: '●',
            style: Style::default()
                .fg(ripple_color(radius))
                .add_modifier(Modifier::BOLD),
        })
    } else if delta <= RIPPLE_HALO_WIDTH {
        Some(Cell {
            ch: '·',
            style: Style::default().fg(ripple_color(radius)),
        })
    } else {
        None
    }
}

fn closest_ripple(distance: isize, max_radius: usize, tick: usize) -> Option<(isize, usize)> {
    (0..RIPPLE_COUNT)
        .map(|index| ripple_radius(max_radius, tick, index))
        .map(|radius| ((distance - radius as isize).abs(), radius))
        .filter(|(delta, _)| *delta <= RIPPLE_HALO_WIDTH)
        .min_by_key(|(delta, radius)| (*delta, *radius))
}

fn ripple_radius(max_radius: usize, tick: usize, index: usize) -> usize {
    let cycle = ripple_cycle(max_radius);
    (tick + index * ripple_spacing(max_radius)) % cycle
}

fn ripple_cycle(max_radius: usize) -> usize {
    max_radius + 1
}

fn ripple_spacing(max_radius: usize) -> usize {
    (ripple_cycle(max_radius) / RIPPLE_COUNT).max(1)
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

const RAINBOW_RIPPLE_SPACING: usize = 8;
const RAINBOW_RIPPLE_CORE_WIDTH: isize = 2;
const RAINBOW_RIPPLE_HALO_WIDTH: isize = 4;

fn draw_rainbow_ripple(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let tick = (elapsed.as_millis() / 60) as usize;
    let height = area.height as usize;
    let width = area.width as usize;
    let mut canvas = blank_canvas(width, height);
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = rainbow_ripple_cell(x, y, width, height, tick) {
                put_cell(&mut canvas, x as isize, y as isize, cell.ch, cell.style);
            }
        }
    }
    render_canvas(frame, area, canvas);
}

fn rainbow_ripple_cell(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    tick: usize,
) -> Option<Cell> {
    if width == 0 || height == 0 {
        return None;
    }
    let center_x = width as isize / 2;
    let center_y = height as isize / 2;
    let dx = x as isize - center_x;
    let dy = (y as isize - center_y) * 2;
    let distance = (((dx * dx + dy * dy) as f64).sqrt().round()) as isize;
    let max_radius = ripple_max_radius(width, height);
    let (delta, ring_id) = closest_rainbow_ripple(distance, max_radius, tick)?;
    let color = rainbow_ripple_color(ring_id);
    if delta <= RAINBOW_RIPPLE_CORE_WIDTH {
        Some(Cell {
            ch: '●',
            style: Style::default().fg(color).add_modifier(Modifier::BOLD),
        })
    } else if delta <= RAINBOW_RIPPLE_HALO_WIDTH {
        Some(Cell {
            ch: '·',
            style: Style::default().fg(color),
        })
    } else {
        None
    }
}

fn closest_rainbow_ripple(
    distance: isize,
    max_radius: usize,
    tick: usize,
) -> Option<(isize, usize)> {
    let latest_ring_id = tick / RAINBOW_RIPPLE_SPACING;
    let progress = tick % RAINBOW_RIPPLE_SPACING;
    let max_age = max_radius / RAINBOW_RIPPLE_SPACING + 2;
    (0..=max_age)
        .filter_map(|age| {
            let ring_id = latest_ring_id.checked_sub(age)?;
            let radius = progress + age * RAINBOW_RIPPLE_SPACING;
            (radius <= max_radius + RAINBOW_RIPPLE_HALO_WIDTH as usize).then_some((radius, ring_id))
        })
        .map(|(radius, ring_id)| ((distance - radius as isize).abs(), ring_id))
        .filter(|(delta, _)| *delta <= RAINBOW_RIPPLE_HALO_WIDTH)
        .min_by_key(|(delta, ring_id)| (*delta, *ring_id))
}

fn rainbow_ripple_color(ring_id: usize) -> Color {
    match ring_id % 3 {
        0 => Color::Blue,
        1 => Color::Green,
        _ => Color::Red,
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
    fn ripple_emits_multiple_thick_centered_rings() {
        let center = ripple_cell(40, 9, 80, 18, 0).expect("center ripple cell");
        assert_eq!(center.ch, '●');
        assert_eq!(center.style.fg, Some(Color::White));
        assert!(center.style.add_modifier.contains(Modifier::BOLD));

        let first_outer_ring = ripple_cell(55, 9, 80, 18, 0).expect("first outer ripple ring");
        assert_eq!(first_outer_ring.ch, '●');
        assert_eq!(first_outer_ring.style.fg, Some(Color::Cyan));

        let first_outer_halo = ripple_cell(57, 9, 80, 18, 0).expect("thick ripple halo");
        assert_eq!(first_outer_halo.ch, '·');
        assert_eq!(first_outer_halo.style.fg, Some(Color::Cyan));

        let second_outer_ring = ripple_cell(70, 9, 80, 18, 0).expect("second outer ripple ring");
        assert_eq!(second_outer_ring.ch, '●');
        assert_eq!(second_outer_ring.style.fg, Some(Color::Blue));

        assert_eq!(
            ripple_cell(63, 9, 80, 18, 0).map(|cell| cell.ch),
            None,
            "there should still be quiet space between the thicker rings"
        );
    }

    #[test]
    fn ripple_rings_are_evenly_spaced_around_the_cycle() {
        let max_radius = ripple_max_radius(80, 18);
        assert_eq!(max_radius, 44);
        assert_eq!(ripple_spacing(max_radius), 15);
        assert_eq!(ripple_radius(max_radius, 0, 0), 0);
        assert_eq!(ripple_radius(max_radius, 0, 1), 15);
        assert_eq!(ripple_radius(max_radius, 0, 2), 30);
        assert_eq!(ripple_radius(max_radius, 40, 0), 40);
        assert_eq!(ripple_radius(max_radius, 40, 1), 10);
        assert_eq!(ripple_radius(max_radius, 40, 2), 25);
    }

    #[test]
    fn ripple_rings_move_outward_over_time() {
        let first = ripple_cell(40, 9, 80, 18, 0).expect("center ripple cell");
        let later = ripple_cell(40, 9, 80, 18, 2);
        assert!(
            later.is_none_or(|cell| cell.ch != first.ch || cell.style.fg != first.style.fg),
            "ripple center should change as the wave passes"
        );
        let moved_ring = ripple_cell(42, 9, 80, 18, 2).expect("moved ripple ring");
        assert_eq!(moved_ring.ch, '●');
        assert_eq!(moved_ring.style.fg, Some(Color::Cyan));
    }

    #[test]
    fn rainbow_ripple_emits_thick_color_locked_bands() {
        let blue_center = rainbow_ripple_cell(40, 9, 80, 18, 0).expect("blue center ring");
        assert_eq!(blue_center.ch, '●');
        assert_eq!(blue_center.style.fg, Some(Color::Blue));

        let green_center = rainbow_ripple_cell(40, 9, 80, 18, 8).expect("green center ring");
        assert_eq!(green_center.ch, '●');
        assert_eq!(green_center.style.fg, Some(Color::Green));

        let blue_expanded = rainbow_ripple_cell(48, 9, 80, 18, 8).expect("expanded blue ring");
        assert_eq!(blue_expanded.ch, '●');
        assert_eq!(blue_expanded.style.fg, Some(Color::Blue));

        let red_center = rainbow_ripple_cell(40, 9, 80, 18, 16).expect("red center ring");
        assert_eq!(red_center.ch, '●');
        assert_eq!(red_center.style.fg, Some(Color::Red));
    }

    #[test]
    fn rainbow_ripple_bands_stay_evenly_spaced() {
        let tick = 24;
        let centers = [
            (40, Color::Blue),
            (48, Color::Red),
            (56, Color::Green),
            (64, Color::Blue),
        ];
        for (x, color) in centers {
            let cell = rainbow_ripple_cell(x, 9, 80, 18, tick).expect("rainbow band center");
            assert_eq!(cell.ch, '●');
            assert_eq!(cell.style.fg, Some(color));
        }
    }
}
