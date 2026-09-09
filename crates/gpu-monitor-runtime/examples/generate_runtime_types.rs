//! Generate the frontend runtime contract from Rust models.
use gpu_monitor_runtime::{
    AlertConfig, AlertEvent, AlertKind, AlertState, HistoryFrame, HistoryGpu, HistoryResponse,
    RecordingStatus,
};
use std::path::PathBuf;
use ts_rs::TS;

fn main() {
    let declarations = [
        AlertConfig::decl(),
        AlertKind::decl(),
        AlertState::decl(),
        AlertEvent::decl(),
        HistoryGpu::decl(),
        HistoryFrame::decl(),
        HistoryResponse::decl(),
        RecordingStatus::decl(),
    ];
    let mut output = String::from("// Generated from gpu-monitor-runtime Rust types; do not edit.\n// cargo run -p gpu-monitor-runtime --features typescript --example generate_runtime_types\n\n");
    for declaration in declarations {
        output.push_str("export ");
        output.push_str(
            &declaration
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        output.push_str("\n\n");
    }
    let output = format!("{}\n", output.trim_end());
    let destination = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gpu-monitor-gui/src-web/src/monitor/runtime-wire.ts");
    if std::env::args().any(|arg| arg == "--check") {
        let existing = std::fs::read_to_string(&destination).unwrap_or_default();
        assert_eq!(
            existing, output,
            "runtime-wire.ts is stale; run the generator without --check"
        );
    } else {
        std::fs::write(destination, output).expect("write generated TypeScript contract");
    }
}
