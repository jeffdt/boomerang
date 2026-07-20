use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use std::time::Duration;

pub const SHIMMER_SWEEP: Duration = Duration::from_millis(600);

pub fn shimmer_spans(text: &str, elapsed: Duration, base_style: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() || elapsed >= SHIMMER_SWEEP {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    let progress = elapsed.as_secs_f64() / SHIMMER_SWEEP.as_secs_f64();
    let peak = ((chars.len() - 1) as f64 * progress).round() as usize;

    let shoulder_style = base_style.fg(brighten(base_style.fg));
    let peak_style = shoulder_style.add_modifier(Modifier::BOLD);

    chars
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let style = if i == peak {
                peak_style
            } else if i.abs_diff(peak) == 1 {
                shoulder_style
            } else {
                base_style
            };
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

fn brighten(color: Option<Color>) -> Color {
    match color {
        Some(Color::Cyan) => Color::LightCyan,
        Some(Color::Blue) => Color::LightBlue,
        Some(Color::Green) => Color::LightGreen,
        Some(Color::Red) => Color::LightRed,
        Some(Color::Yellow) => Color::LightYellow,
        Some(Color::Magenta) => Color::LightMagenta,
        Some(Color::DarkGray) => Color::Gray,
        Some(Color::Gray) | None => Color::White,
        Some(other) => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak_index(spans: &[Span]) -> Option<usize> {
        spans
            .iter()
            .position(|s| s.style.add_modifier.contains(Modifier::BOLD))
    }

    #[test]
    fn brighten_maps_named_colors_to_light_variants() {
        assert_eq!(brighten(Some(Color::Cyan)), Color::LightCyan);
        assert_eq!(brighten(Some(Color::DarkGray)), Color::Gray);
        assert_eq!(brighten(Some(Color::Gray)), Color::White);
        assert_eq!(brighten(None), Color::White);
        assert_eq!(brighten(Some(Color::White)), Color::White);
    }

    #[test]
    fn sweep_starts_at_first_character() {
        let spans = shimmer_spans("done!", Duration::from_millis(0), Style::default());
        assert_eq!(peak_index(&spans), Some(0));
    }

    #[test]
    fn sweep_reaches_last_character_just_before_full_duration() {
        let spans = shimmer_spans(
            "done!",
            SHIMMER_SWEEP - Duration::from_millis(1),
            Style::default(),
        );
        assert_eq!(peak_index(&spans), Some(4));
    }

    #[test]
    fn sweep_finished_renders_single_plain_span() {
        let spans = shimmer_spans("done!", SHIMMER_SWEEP, Style::default());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, Style::default());
        assert_eq!(spans[0].content.as_ref(), "done!");
    }

    #[test]
    fn peak_brightens_default_style_to_white_and_bold() {
        let spans = shimmer_spans("d", Duration::from_millis(0), Style::default());
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn peak_brightens_cyan_base_to_light_cyan_and_keeps_other_modifiers() {
        let base = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::ITALIC);
        let spans = shimmer_spans("b", Duration::from_millis(0), base);
        assert_eq!(spans[0].style.fg, Some(Color::LightCyan));
        assert!(spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn shoulder_characters_are_brightened_without_bold() {
        // "abc" at 300ms of a 600ms sweep: progress=0.5, peak=round(2*0.5)=1 ('b').
        let spans = shimmer_spans("abc", Duration::from_millis(300), Style::default());
        assert_eq!(peak_index(&spans), Some(1));
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert!(!spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[2].style.fg, Some(Color::White));
        assert!(!spans[2].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn characters_far_from_peak_are_unmodified_base_style() {
        let spans = shimmer_spans("abcdef", Duration::from_millis(0), Style::default());
        // peak=0, shoulder=1, everything from index 2 on is untouched.
        assert_eq!(spans[3].style, Style::default());
    }
}
