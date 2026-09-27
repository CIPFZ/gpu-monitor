//! Small composable pieces shared by the panes.
//!
//! A meter renders a reading, its bar and its severity together, so a number can
//! never disagree with the colour beside it, and an unavailable measurement is
//! drawn with a distinct fill instead of an empty bar that reads as zero.

use ratatui::text::{Line, Span};
use std::collections::VecDeque;

use crate::{
    device::HistoryPoint,
    format,
    theme::{self, Severity},
};

const FILLED: char = '█';
const EMPTY: char = '░';
/// Distinguishes "not measured" from "measured as zero".
const UNKNOWN: char = '·';
const PARTIAL: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

const LABEL_WIDTH: usize = 7;
const READING_WIDTH: usize = 17;
/// Below this a bar would be too short to read, so the meter shows text only.
const MIN_BAR_WIDTH: usize = 6;

/// Renders a proportional bar of exactly `width` characters.
pub fn bar(width: usize, ratio: Option<f64>) -> String {
    if width == 0 {
        return String::new();
    }
    let Some(ratio) = ratio.filter(|ratio| ratio.is_finite()) else {
        return UNKNOWN.to_string().repeat(width);
    };
    let eighths = (ratio.clamp(0.0, 100.0) / 100.0 * (width * 8) as f64).round() as usize;
    let full = (eighths / 8).min(width);
    let remainder = eighths % 8;
    let mut rendered = FILLED.to_string().repeat(full);
    if full < width && remainder > 0 {
        rendered.push(PARTIAL[remainder]);
    }
    let drawn = rendered.chars().count();
    rendered.push_str(&EMPTY.to_string().repeat(width.saturating_sub(drawn)));
    rendered
}

/// `Load    ███████░░░░              87%`
pub fn meter(
    width: u16,
    label: &str,
    ratio: Option<f64>,
    reading: &str,
    severity: Severity,
) -> Line<'static> {
    let width = width as usize;
    let text = format!("{reading:>READING_WIDTH$}");
    let label = format!("{:<LABEL_WIDTH$}", format::truncate_str(label, LABEL_WIDTH));
    let bar_width = width
        .saturating_sub(LABEL_WIDTH + READING_WIDTH + 2)
        .min(64);
    if bar_width < MIN_BAR_WIDTH {
        return Line::from(vec![
            Span::styled(label, theme::label()),
            Span::styled(text, severity.style()),
        ]);
    }
    Line::from(vec![
        Span::styled(label, theme::label()),
        Span::styled(bar(bar_width, ratio), severity.style()),
        Span::raw("  "),
        Span::styled(text, severity.style()),
    ])
}

/// A key and its value on one line, for facts that have no natural ratio.
pub fn field(label: &str, value: String) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{label} "), theme::label()),
        Span::raw(value),
    ]
}

/// Compact severity marker used where a full bar does not fit.
pub fn dot(severity: Severity) -> Span<'static> {
    Span::styled("●", severity.style())
}

/// One column of a chart. A span the session never covered is not the same as a
/// span whose acquisition failed, and neither is a zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bucket {
    /// No acquisition covers this span, so there is nothing to report yet.
    #[default]
    Unsampled,
    /// An acquisition covers this span but the metric was unavailable.
    Missing,
    Value(u64),
}

/// Highest charted value, rounded up so the axis label stays readable.
pub fn series_ceiling(buckets: &[Bucket], floor: u64) -> u64 {
    let peak = buckets
        .iter()
        .filter_map(|bucket| match bucket {
            Bucket::Value(value) => Some(*value),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let ceiling = peak.max(floor);
    let step = if ceiling > 500 { 100 } else { 10 };
    ceiling.div_ceil(step) * step
}

/// Drops the leading span that was never sampled, so a short session is drawn at
/// the right-hand edge instead of behind a wall of gap markers. A gap *inside*
/// the series is kept, because there the absence of a reading is real.
pub fn sampled_tail(buckets: &[Bucket]) -> (usize, Vec<Option<u64>>) {
    let leading = buckets
        .iter()
        .take_while(|bucket| **bucket == Bucket::Unsampled)
        .count();
    let tail = buckets[leading..]
        .iter()
        .map(|bucket| match bucket {
            Bucket::Value(value) => Some(*value),
            _ => None,
        })
        .collect();
    (leading, tail)
}

/// Each column represents equal elapsed time. Any explicit missing sample leaves
/// a gap in its bucket; populated buckets retain peaks so brief load is not
/// erased by averaging.
pub fn time_buckets(
    history: &VecDeque<HistoryPoint>,
    now_ms: u64,
    window_ms: u64,
    sample_interval_ms: u64,
    width: usize,
    metric: fn(&HistoryPoint) -> Option<u64>,
) -> Vec<Bucket> {
    if width == 0 || window_ms == 0 {
        return vec![];
    }
    let start = now_ms.saturating_sub(window_ms);
    let mut buckets = vec![Bucket::Unsampled; width];
    for point in history {
        if point.sampled_at_ms < start || point.sampled_at_ms > now_ms {
            continue;
        }
        let first = (((point.sampled_at_ms - start) as u128 * width as u128 / window_ms as u128)
            as usize)
            .min(width - 1);
        // A sampled value covers its acquisition interval, without extending over
        // missing acquisitions. This avoids fictitious gaps on wider terminals.
        let covered_until = point
            .sampled_at_ms
            .saturating_add(sample_interval_ms.saturating_sub(1))
            .min(now_ms);
        let last = (((covered_until - start) as u128 * width as u128 / window_ms as u128) as usize)
            .min(width - 1);
        let reading = metric(point);
        for slot in &mut buckets[first..=last] {
            match (reading, *slot) {
                // A failed acquisition is never overwritten by a later value in
                // the same bucket: the interruption must stay visible.
                (_, Bucket::Missing) => {}
                (None, _) => *slot = Bucket::Missing,
                (Some(value), Bucket::Value(previous)) => {
                    *slot = Bucket::Value(previous.max(value));
                }
                (Some(value), Bucket::Unsampled) => *slot = Bucket::Value(value),
            }
        }
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn a_bar_always_occupies_its_width_and_marks_unknown_readings() {
        for ratio in [Some(0.0), Some(50.0), Some(100.0), Some(-5.0), None] {
            assert_eq!(bar(10, ratio).chars().count(), 10, "ratio {ratio:?}");
        }
        assert_eq!(bar(4, Some(100.0)), "████");
        assert_eq!(bar(4, Some(0.0)), "░░░░");
        assert_eq!(bar(4, Some(150.0)), "████", "readings are clamped");
        assert_eq!(bar(4, None), "····");
        assert_eq!(bar(4, Some(f64::NAN)), "····");
        assert_eq!(bar(0, Some(50.0)), "");
    }

    #[test]
    fn a_zero_reading_is_visibly_different_from_an_unavailable_one() {
        assert_ne!(bar(8, Some(0.0)), bar(8, None));
    }

    #[test]
    fn a_meter_degrades_to_text_before_it_draws_an_unreadable_bar() {
        let wide = meter(80, "Load", Some(50.0), "50%", Severity::Ok);
        assert!(rendered(&wide).contains('█'));
        assert!(rendered(&wide).contains("50%"));
        let narrow = meter(28, "Load", Some(50.0), "50%", Severity::Ok);
        let narrow = rendered(&narrow);
        assert!(!narrow.contains('█') && !narrow.contains('░'));
        assert!(narrow.contains("50%"), "the reading survives a narrow pane");
    }

    #[test]
    fn a_long_label_cannot_push_the_reading_out_of_the_pane() {
        let line = meter(80, "Temperature", Some(10.0), "10°C", Severity::Ok);
        assert!(rendered(&line).starts_with("Temper…"));
        assert!(rendered(&line).ends_with("10°C"));
    }

    #[test]
    fn the_chart_ceiling_covers_the_peak_without_collapsing_on_an_empty_series() {
        assert_eq!(series_ceiling(&[], 100), 100);
        assert_eq!(
            series_ceiling(&[Bucket::Unsampled, Bucket::Missing], 100),
            100
        );
        assert_eq!(series_ceiling(&[Bucket::Value(45)], 100), 100);
        assert_eq!(series_ceiling(&[Bucket::Value(312)], 100), 320);
        assert_eq!(series_ceiling(&[Bucket::Value(640)], 100), 700);
    }

    #[test]
    fn a_short_session_is_charted_at_the_edge_not_behind_a_wall_of_gaps() {
        let buckets = [
            Bucket::Unsampled,
            Bucket::Unsampled,
            Bucket::Value(10),
            Bucket::Missing,
            Bucket::Value(20),
        ];
        let (leading, tail) = sampled_tail(&buckets);
        assert_eq!(leading, 2, "time before the first sample stays blank");
        assert_eq!(
            tail,
            [Some(10), None, Some(20)],
            "a gap inside the series is a real absence and is kept"
        );
        let (leading, tail) = sampled_tail(&[Bucket::Unsampled; 4]);
        assert_eq!(leading, 4);
        assert!(
            tail.is_empty(),
            "nothing is charted before the first sample"
        );
        assert_eq!(sampled_tail(&[]), (0, vec![]));
    }

    #[test]
    fn buckets_cover_real_windows_retain_peaks_and_do_not_bridge_missing_samples() {
        let history = VecDeque::from([
            HistoryPoint {
                sampled_at_ms: 10_000,
                gpu: Some(10),
                ..HistoryPoint::default()
            },
            HistoryPoint {
                sampled_at_ms: 11_000,
                gpu: Some(90),
                ..HistoryPoint::default()
            },
            HistoryPoint::missing(12_000),
            HistoryPoint {
                sampled_at_ms: 13_000,
                gpu: Some(20),
                ..HistoryPoint::default()
            },
        ]);
        let metric: fn(&HistoryPoint) -> Option<u64> = |point| point.gpu;
        let value = Bucket::Value;
        assert_eq!(
            time_buckets(&history, 14_000, 4_000, 1_000, 4, metric),
            [value(10), value(90), Bucket::Missing, value(20)]
        );
        assert_eq!(
            time_buckets(&history, 14_000, 4_000, 1_000, 8, metric),
            [
                value(10),
                value(10),
                value(90),
                value(90),
                Bucket::Missing,
                Bucket::Missing,
                value(20),
                value(20)
            ],
            "a sample covers its acquisition interval on a wider pane"
        );
        assert_eq!(
            time_buckets(&history, 14_000, 4_000, 1_000, 2, metric),
            [value(90), Bucket::Missing],
            "compression keeps the peak and the gap"
        );
        assert_eq!(
            time_buckets(&history, 74_000, 60_000, 1_000, 6, metric),
            [Bucket::Unsampled; 6],
            "samples outside the window are not carried forward, and the span \
             they left is reported as never sampled rather than as a failure"
        );
        assert!(time_buckets(&history, 14_000, 0, 1_000, 6, metric).is_empty());
    }
}
