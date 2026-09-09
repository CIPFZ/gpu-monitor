use gpu_monitor_core::{GpuMetrics, GpuProcess, MonitorSnapshot, SCHEMA_VERSION};

#[test]
fn old_snapshots_and_processes_remain_readable_and_reserialize_current_contract() {
    let snapshot: MonitorSnapshot =
        serde_json::from_str(r#"{"sampled_at_ms":100,"gpus":[],"failures":[],"error":null}"#)
            .unwrap();
    assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
    assert_eq!(serde_json::to_value(snapshot).unwrap()["schema_version"], 1);
    let process: GpuProcess = serde_json::from_str(
        r#"{"pid":10,"name":"python","gpu_memory":0,"process_type":"Compute"}"#,
    )
    .unwrap();
    assert_eq!(process.uid, None);
    assert_eq!(process.user, None);
    assert_eq!(process.command, None);
    assert_eq!(process.started_at_ms, None);
    assert_eq!(process.elapsed_seconds, None);
    assert_eq!(process.gpu_memory, Some(0));
    let encoded = serde_json::to_value(process).unwrap();
    assert!(encoded["command"].is_null());
    assert!(encoded["started_at_ms"].is_null());
}

#[test]
fn older_metrics_default_new_diagnostics_to_unknown() {
    let metrics: GpuMetrics = serde_json::from_str("{}").unwrap();
    assert_eq!(metrics.performance_state, None);
    assert_eq!(metrics.throttle_reasons, None);
    assert_eq!(metrics.pcie_generation, None);
    assert_eq!(metrics.pcie_rx_kb_per_second, None);
    let metrics = GpuMetrics {
        throttle_reasons: Some(Vec::new()),
        pcie_rx_kb_per_second: Some(0),
        ..Default::default()
    };
    let encoded = serde_json::to_value(metrics).unwrap();
    assert_eq!(encoded["throttle_reasons"], serde_json::json!([]));
    assert_eq!(encoded["pcie_rx_kb_per_second"], 0);
}
