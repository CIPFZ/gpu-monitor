//! Single source of colour and severity semantics for the terminal interface.
//!
//! Thresholds live here instead of inside render functions so that a reading and
//! its colour can never disagree between two views. Temperature reuses the shared
//! classification from `gpu-monitor-core` rather than restating its limits.

use gpu_monitor_core::{metrics::TemperatureStatus, GpuMetrics};
use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::Cyan;
pub const MUTED: Color = Color::DarkGray;
pub const STALE: Color = Color::Yellow;
pub const FAILED: Color = Color::Red;
pub const SELECTION: Color = Color::Indexed(24);

pub const LOAD_SERIES: Color = Color::Green;
pub const MEMORY_SERIES: Color = Color::Cyan;
pub const TEMPERATURE_SERIES: Color = Color::Magenta;
pub const POWER_SERIES: Color = Color::Yellow;

/// How far a reading has moved towards its operational limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// No measurement is available; never rendered as a healthy zero.
    Unknown,
    Ok,
    Notice,
    Warn,
    Critical,
}

impl Severity {
    pub fn color(self) -> Color {
        match self {
            Self::Unknown => MUTED,
            Self::Ok => Color::Green,
            Self::Notice => Color::Cyan,
            Self::Warn => Color::Yellow,
            Self::Critical => Color::Red,
        }
    }

    pub fn style(self) -> Style {
        Style::default().fg(self.color())
    }
}

/// Utilisation is a workload signal, so a busy device is informative, not unhealthy.
pub fn from_load(percent: Option<f64>) -> Severity {
    classify(percent, 60.0, 85.0, 98.0)
}

/// Capacity pressure matters well before the device is completely full.
pub fn from_memory(percent: Option<f64>) -> Severity {
    classify(percent, 70.0, 85.0, 95.0)
}

pub fn from_temperature(celsius: Option<u32>) -> Severity {
    let metrics = GpuMetrics {
        temperature: celsius,
        ..GpuMetrics::default()
    };
    match metrics.temperature_status() {
        None => Severity::Unknown,
        Some(TemperatureStatus::Cool) => Severity::Ok,
        Some(TemperatureStatus::Normal) => Severity::Notice,
        Some(TemperatureStatus::Warm) => Severity::Warn,
        Some(TemperatureStatus::Hot) => Severity::Critical,
    }
}

/// Without a reported limit the draw cannot be expressed as a ratio.
pub fn from_power(watts: Option<f32>, limit: Option<u32>) -> Severity {
    let Some(ratio) = power_ratio(watts, limit) else {
        return Severity::Unknown;
    };
    classify(Some(ratio), 70.0, 90.0, 100.0)
}

pub fn power_ratio(watts: Option<f32>, limit: Option<u32>) -> Option<f64> {
    let watts = watts? as f64;
    let limit = limit.filter(|limit| *limit > 0)? as f64;
    Some(watts / limit * 100.0)
}

fn classify(percent: Option<f64>, notice: f64, warn: f64, critical: f64) -> Severity {
    match percent {
        None => Severity::Unknown,
        Some(percent) if !percent.is_finite() => Severity::Unknown,
        Some(percent) if percent >= critical => Severity::Critical,
        Some(percent) if percent >= warn => Severity::Warn,
        Some(percent) if percent >= notice => Severity::Notice,
        Some(_) => Severity::Ok,
    }
}

pub fn header() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn footer() -> Style {
    Style::default().fg(MUTED)
}

pub fn tab_active() -> Style {
    Style::default()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

pub fn tab_inactive() -> Style {
    Style::default().fg(MUTED)
}

pub fn table_header() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn selected_row() -> Style {
    Style::default().bg(SELECTION).add_modifier(Modifier::BOLD)
}

pub fn label() -> Style {
    Style::default().fg(MUTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_readings_are_unknown_rather_than_healthy() {
        assert_eq!(from_load(None), Severity::Unknown);
        assert_eq!(from_memory(None), Severity::Unknown);
        assert_eq!(from_temperature(None), Severity::Unknown);
        assert_eq!(from_power(None, Some(450)), Severity::Unknown);
        assert_eq!(from_load(Some(f64::NAN)), Severity::Unknown);
        assert_eq!(Severity::Unknown.color(), MUTED);
    }

    #[test]
    fn a_real_zero_is_a_healthy_reading() {
        assert_eq!(from_load(Some(0.0)), Severity::Ok);
        assert_eq!(from_memory(Some(0.0)), Severity::Ok);
        assert_eq!(from_temperature(Some(0)), Severity::Ok);
    }

    #[test]
    fn severity_escalates_monotonically_with_the_reading() {
        assert_eq!(from_load(Some(59.9)), Severity::Ok);
        assert_eq!(from_load(Some(60.0)), Severity::Notice);
        assert_eq!(from_load(Some(85.0)), Severity::Warn);
        assert_eq!(from_load(Some(98.0)), Severity::Critical);
        assert_eq!(from_memory(Some(95.0)), Severity::Critical);
    }

    #[test]
    fn temperature_matches_the_shared_core_classification() {
        assert_eq!(from_temperature(Some(50)), Severity::Ok);
        assert_eq!(from_temperature(Some(65)), Severity::Notice);
        assert_eq!(from_temperature(Some(80)), Severity::Warn);
        assert_eq!(from_temperature(Some(90)), Severity::Critical);
    }

    #[test]
    fn power_without_a_limit_cannot_be_expressed_as_a_ratio() {
        assert_eq!(power_ratio(Some(100.0), None), None);
        assert_eq!(power_ratio(Some(100.0), Some(0)), None);
        assert_eq!(power_ratio(Some(225.0), Some(450)), Some(50.0));
        assert_eq!(from_power(Some(450.0), Some(450)), Severity::Critical);
    }
}
