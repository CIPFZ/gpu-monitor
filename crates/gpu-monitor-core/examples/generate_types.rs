//! Generate the frontend wire contract from the public Rust data model.
use gpu_monitor_core::{
    DeviceFailure, DeviceInfo, ErrorKind, GpuInfo, GpuMetrics, GpuProcess, MemoryInfo, MetricIssue,
    MonitorSnapshot, ProcessType, SampleError,
};
use std::path::PathBuf;
use ts_rs::TS;

fn main() {
    let declarations = [
        ErrorKind::decl(),
        SampleError::decl(),
        MetricIssue::decl(),
        MemoryInfo::decl(),
        GpuMetrics::decl(),
        DeviceInfo::decl(),
        ProcessType::decl(),
        GpuProcess::decl(),
        GpuInfo::decl(),
        DeviceFailure::decl(),
        MonitorSnapshot::decl(),
    ];
    let mut output = String::from("// Generated from gpu-monitor-core Rust types; do not edit.\n// cargo run -p gpu-monitor-core --features typescript --example generate_types\n\n");
    output.push_str(&format!(
        "export const SCHEMA_VERSION = {} as const;\n\n",
        gpu_monitor_core::SCHEMA_VERSION
    ));
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
        .join("../gpu-monitor-gui/src-web/src/monitor/wire.ts");
    if std::env::args().any(|arg| arg == "--check") {
        let existing = std::fs::read_to_string(&destination).unwrap_or_default();
        assert_eq!(
            existing, output,
            "wire.ts is stale; run the generator without --check"
        );
    } else {
        std::fs::write(destination, output).expect("write generated TypeScript contract");
    }
}
