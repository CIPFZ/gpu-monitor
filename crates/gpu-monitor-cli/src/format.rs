//! Pure formatting helpers shared by the terminal interface and the reports.
//!
//! Every conversion keeps an unavailable measurement distinguishable from a real
//! zero, and every string that originates from a device, a process or a recording
//! passes through control-character sanitisation before reaching a terminal.

use gpu_monitor_core::{GpuProcess, MemoryInfo};
use std::fmt::Display;

/// Rendered placeholder for a measurement the driver did not provide.
pub const UNAVAILABLE: &str = "N/A";

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Formats an optional measurement, appending `suffix` only to a real reading.
pub fn value<T: Display>(value: Option<T>, suffix: &str) -> String {
    value.map_or_else(
        || UNAVAILABLE.to_owned(),
        |value| format!("{value}{suffix}"),
    )
}

pub fn gib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_GIB
}

/// `used/total GiB`, or the placeholder when capacity is unknown.
pub fn memory_capacity(memory: Option<&MemoryInfo>) -> String {
    memory.map_or_else(
        || UNAVAILABLE.to_owned(),
        |memory| format!("{:.1}/{:.1} GiB", gib(memory.used), gib(memory.total)),
    )
}

/// Capacity usage ratio. A zero-sized report cannot express a ratio.
pub fn memory_ratio(memory: Option<&MemoryInfo>) -> Option<f64> {
    memory
        .filter(|memory| memory.total > 0)
        .map(|memory| memory.used as f64 / memory.total as f64 * 100.0)
}

/// `310/450 W`; either side may be unavailable on its own.
pub fn power_capacity(watts: Option<f32>, limit: Option<u32>) -> String {
    match (watts, limit) {
        (None, None) => UNAVAILABLE.to_owned(),
        (Some(watts), None) => format!("{watts:.0}/{UNAVAILABLE} W"),
        (None, Some(limit)) => format!("{UNAVAILABLE}/{limit} W"),
        (Some(watts), Some(limit)) => format!("{watts:.0}/{limit} W"),
    }
}

/// An empty reason list means the driver reported no active limiter, which is
/// different from being unable to read the reasons at all.
pub fn throttle(reasons: Option<&Vec<String>>) -> String {
    reasons.map_or_else(
        || UNAVAILABLE.to_owned(),
        |reasons| {
            if reasons.is_empty() {
                "none".to_owned()
            } else {
                reasons
                    .iter()
                    .map(|reason| safe_text(reason))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        },
    )
}

pub fn pcie_link(generation: Option<u32>, width: Option<u32>) -> String {
    match (generation, width) {
        (None, None) => UNAVAILABLE.to_owned(),
        (generation, width) => format!("Gen {} ×{}", value(generation, ""), value(width, "")),
    }
}

pub fn elapsed(seconds: Option<u64>) -> String {
    seconds.map_or_else(
        || UNAVAILABLE.to_owned(),
        |seconds| {
            let days = seconds / 86_400;
            let hours = seconds / 3_600 % 24;
            let minutes = seconds / 60 % 60;
            let seconds = seconds % 60;
            if days > 0 {
                format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
            } else {
                format!("{hours:02}:{minutes:02}:{seconds:02}")
            }
        },
    )
}

/// Compact label for a history window, used in titles where space is scarce.
pub fn window_label(window_ms: u64) -> String {
    let seconds = window_ms / 1000;
    match seconds {
        0 => "0s".to_owned(),
        seconds if seconds % 3_600 == 0 => format!("{}h", seconds / 3_600),
        seconds if seconds % 60 == 0 => format!("{}m", seconds / 60),
        seconds => format!("{seconds}s"),
    }
}

/// Device names repeat the vendor, which wastes the width a compact list has.
/// The model is what distinguishes one device from another.
pub fn device_model(name: &str) -> String {
    let name = safe_text(name);
    let trimmed = name.trim();
    for vendor in ["NVIDIA ", "Nvidia "] {
        if let Some(model) = trimmed.strip_prefix(vendor) {
            if !model.trim().is_empty() {
                return model.trim().to_owned();
            }
        }
    }
    trimmed.to_owned()
}

/// Local account name when resolvable, otherwise the numeric UID.
pub fn process_owner(process: &GpuProcess) -> String {
    process
        .user
        .as_ref()
        .map(|name| safe_text(name))
        .or_else(|| process.uid.map(|uid| uid.to_string()))
        .unwrap_or_else(|| UNAVAILABLE.to_owned())
}

/// Full argument vector when it was collected and the caller opted in.
pub fn process_command(process: &GpuProcess, include_command: bool) -> String {
    if !include_command {
        return safe_text(&process.name);
    }
    process
        .command
        .as_ref()
        .filter(|command| !command.is_empty())
        .map(|command| {
            command
                .iter()
                .map(|argument| safe_text(argument))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| safe_text(&process.name))
}

/// Replaces control characters while preserving line structure.
pub fn safe_lines(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() && character != '\n' {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Replaces every control character, including newlines, for single-line cells.
pub fn safe_text(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Character-boundary-safe truncation with an ellipsis marker.
pub fn truncate_str(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    if max_chars == 0 {
        return String::new();
    }
    format!(
        "{}…",
        text.chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
    )
}

/// Wraps sanitised text to `width`, returning the exact lines that will be
/// drawn. Scroll limits are derived from this result rather than estimated, so a
/// pane can never scroll past its own content.
pub fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for source in safe_lines(text).lines() {
        if source.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0;
        for word in source.split_inclusive(' ') {
            let word_width = word.chars().count();
            if current_width > 0 && current_width + word_width > width {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
            }
            // A single word longer than the pane is split instead of hidden.
            if word_width > width {
                for character in word.chars() {
                    if current_width == width {
                        lines.push(std::mem::take(&mut current));
                        current_width = 0;
                    }
                    current.push(character);
                    current_width += 1;
                }
                continue;
            }
            current.push_str(word);
            current_width += word_width;
        }
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_reports_the_exact_rendered_line_count() {
        assert_eq!(wrap_lines("", 10), [""]);
        assert_eq!(wrap_lines("short", 10), ["short"]);
        assert_eq!(wrap_lines("a\n\nb", 10), ["a", "", "b"]);
        let wrapped = wrap_lines("alpha beta gamma", 11);
        assert_eq!(wrapped, ["alpha beta ", "gamma"]);
        assert!(wrapped.iter().all(|line| line.chars().count() <= 11));
    }

    #[test]
    fn an_oversized_word_is_split_rather_than_hidden() {
        let wrapped = wrap_lines("aaaaaaaa", 3);
        assert_eq!(wrapped, ["aaa", "aaa", "aa"]);
        assert_eq!(wrapped.concat(), "aaaaaaaa");
    }

    #[test]
    fn wrapping_sanitises_control_characters_but_keeps_line_structure() {
        let wrapped = wrap_lines("clear\u{1b}[2J\nnext", 40);
        assert_eq!(wrapped, ["clear [2J", "next"]);
    }

    #[test]
    fn unavailable_readings_never_collapse_into_zero() {
        assert_eq!(value(None::<u32>, "%"), "N/A");
        assert_eq!(value(Some(0), "%"), "0%");
        assert_eq!(memory_capacity(None), "N/A");
        assert_eq!(memory_ratio(None), None);
        assert_eq!(
            memory_ratio(Some(&MemoryInfo {
                total: 0,
                used: 0,
                free: 0
            })),
            None,
            "a zero-sized report cannot express a ratio"
        );
        assert_eq!(power_capacity(None, Some(450)), "N/A/450 W");
        assert_eq!(power_capacity(Some(12.4), None), "12/N/A W");
        assert_eq!(pcie_link(None, None), "N/A");
    }

    #[test]
    fn empty_throttle_list_differs_from_an_unreadable_one() {
        assert_eq!(throttle(None), "N/A");
        assert_eq!(throttle(Some(&vec![])), "none");
        assert_eq!(
            throttle(Some(&vec!["sw_power_cap\u{1b}[2J".into()])),
            "sw_power_cap [2J"
        );
    }

    #[test]
    fn a_compact_list_shows_the_model_rather_than_the_vendor() {
        assert_eq!(device_model("NVIDIA GeForce RTX 4090"), "GeForce RTX 4090");
        assert_eq!(device_model("NVIDIA A100-SXM4-80GB"), "A100-SXM4-80GB");
        assert_eq!(device_model("Some Other GPU"), "Some Other GPU");
        assert_eq!(
            device_model("NVIDIA "),
            "NVIDIA",
            "a name that is only a vendor is still shown"
        );
        assert_eq!(device_model("NVIDIA A\x1b[2J"), "A [2J");
    }

    #[test]
    fn labels_truncate_on_character_boundaries_and_windows_stay_compact() {
        assert_eq!(truncate_str("GPU训练进程", 6), "GPU训练…");
        assert_eq!(truncate_str("short", 5), "short");
        assert_eq!(truncate_str("abc", 0), "");
        assert_eq!(window_label(60_000), "1m");
        assert_eq!(window_label(3_600_000), "1h");
        assert_eq!(window_label(45_000), "45s");
        assert_eq!(elapsed(Some(90_061)), "1d 01:01:01");
        assert_eq!(elapsed(None), "N/A");
    }
}
