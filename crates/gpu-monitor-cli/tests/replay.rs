//! Exercise the actual executable and recording format without loading NVML.
use serde_json::{json, Value};
use std::{
    fs,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gpu-monitor"))
        .args(args)
        .output()
        .unwrap()
}
fn frame(at: u64) -> Value {
    json!({"schema_version":1,"sampled_at_ms":at,"failures":[],"error":null,"gpus":[{
        "device":{"index":0,"uuid":"GPU-recorded","name":"Recorded GPU","pci_bus_id":"bus","driver_version":"driver"},
        "metrics":{"gpu_utilization":30,"temperature":65},"memory":{"used":2,"total":10,"free":8},
        "sampled_at_ms":at,"issues":[],"processes":[{"pid":99,"name":"python","process_type":"Compute","gpu_memory":1,
        "user":"alice","uid":1000,"elapsed_seconds":61,"command":["python","--token=secret"]}]
    }]})
}
fn recording(frames: &[Value]) -> (TempDir, String) {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("session.jsonl");
    fs::write(
        &path,
        frames
            .iter()
            .map(|frame| format!("{frame}\n"))
            .collect::<String>(),
    )
    .unwrap();
    (directory, path.to_str().unwrap().into())
}
#[test]
fn executable_replays_private_arguments_only_when_explicitly_enabled() {
    let (_directory, path) = recording(&[frame(1000)]);
    let output = run(&["replay", &path, "--once", "--json", "--user", "1000"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["gpus"][0]["processes"][0]["user"], "alice");
    assert_eq!(data["gpus"][0]["processes"][0]["elapsed_seconds"], 61);
    assert!(data["gpus"][0]["processes"][0]["command"].is_null());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
    let explicit = run(&["replay", &path, "--once", "--json", "--include-command"]);
    assert!(explicit.status.success());
    assert!(String::from_utf8_lossy(&explicit.stdout).contains("--token=secret"));
}
#[test]
fn executable_replay_filters_have_useful_empty_selection_status() {
    let (_directory, path) = recording(&[frame(1000)]);
    let output = run(&["replay", &path, "--once", "--json", "--gpu", "missing"]);
    assert!(!output.status.success());
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["gpus"], json!([]));
    assert!(String::from_utf8_lossy(&output.stderr).contains("No GPUs match"));
    let output = run(&["replay", &path, "--once", "--json", "--user", "bob"]);
    assert!(output.status.success());
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["gpus"][0]["processes"], json!([]));
}
#[test]
fn executable_replay_streams_every_frame_and_rejects_invalid_files() {
    let (_directory, path) = recording(&[frame(1000), frame(1200), frame(2500)]);
    let output = run(&["replay", &path, "--json", "--speed", "1000"]);
    assert!(output.status.success());
    let frames = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame["sampled_at_ms"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1000, 1200, 2500]
    );
    fs::write(&path, "{incomplete\n").unwrap();
    assert!(!run(&["replay", &path, "--json"]).status.success());
    let mut unsupported = frame(1000);
    unsupported["schema_version"] = json!(999);
    fs::write(&path, format!("{unsupported}\n")).unwrap();
    assert!(!run(&["replay", &path, "--json"]).status.success());
}
#[test]
fn record_refuses_to_overwrite_existing_data() {
    let (_directory, path) = recording(&[frame(1000)]);
    let original = fs::read(&path).unwrap();
    let output = run(&["record", &path, "--duration", "1"]);
    assert!(!output.status.success());
    assert_eq!(fs::read(path).unwrap(), original);
}
