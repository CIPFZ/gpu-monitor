// Generated from gpu-monitor-runtime Rust types; do not edit.
// cargo run -p gpu-monitor-runtime --features typescript --example generate_runtime_types

export type AlertConfig = { enabled: boolean, temperature_threshold: number, temperature_recovery: number, memory_threshold: number, memory_recovery: number, duration_ms: number, cooldown_ms: number, };

export type AlertKind = "temperature" | "memory" | "device_unavailable" | "monitor_unavailable";

export type AlertState = "firing" | "recovered";

export type AlertEvent = { id: number, at_ms: number, gpu_uuid: string | null, kind: AlertKind, state: AlertState, message: string, value: number | null, };

export type HistoryGpu = { uuid: string, index: number, gpu_utilization: number | null, memory_percent: number | null, temperature: number | null, power_watts: number | null, };

export type HistoryFrame = { sampled_at_ms: number, gpus: Array<HistoryGpu>, };

export type HistoryResponse = { window_ms: number, interval_ms: number, frames: Array<HistoryFrame>, };

export type RecordingStatus = { active: boolean, finishing: boolean, path: string | null, samples_written: number, dropped_samples: number, bytes_written: number, error: string | null, };
