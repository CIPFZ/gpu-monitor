// Generated from gpu-monitor-core Rust types; do not edit.
// cargo run -p gpu-monitor-core --features typescript --example generate_types

export const SCHEMA_VERSION = 1 as const;

export type ErrorKind = "not_supported" | "permission_denied" | "device_lost" | "uninitialized" | "no_devices" | "unknown";

export type SampleError = { kind: ErrorKind, message: string, };

export type MetricIssue = { metric: string, error: SampleError, };

export type MemoryInfo = {
/**
 * Total memory in bytes
 */
total: number,
/**
 * Used memory in bytes
 */
used: number,
/**
 * Free memory in bytes
 */
free: number, };

export type GpuMetrics = {
/**
 * GPU utilization percentage (0-100)
 */
gpu_utilization: number | null,
/**
 * Memory controller busy percentage (0-100), not capacity usage
 */
memory_utilization: number | null,
/**
 * Encoder utilization percentage (0-100)
 */
encoder_utilization: number | null,
/**
 * Decoder utilization percentage (0-100)
 */
decoder_utilization: number | null,
/**
 * Current temperature in Celsius
 */
temperature: number | null,
/**
 * Current power usage in milliwatts
 */
power_usage: number | null,
/**
 * Fan speed percentage (0-100), None if not available
 */
fan_speed: number | null,
/**
 * Current graphics clock in MHz
 */
clock_graphics: number | null,
/**
 * Current memory clock in MHz
 */
clock_memory: number | null,
/**
 * Current SM clock in MHz
 */
clock_sm: number | null,
/**
 * NVML performance state, P0 (highest performance) through P15.
 */
performance_state: string | null,
/**
 * Active NVML clock-limiting reasons. Empty means no reason is active.
 */
throttle_reasons: Array<string> | null,
/**
 * Currently negotiated PCIe generation and lane count.
 */
pcie_generation: number | null, pcie_width: number | null,
/**
 * PCIe receive throughput in NVML's documented KB/s, over a 20 ms window.
 */
pcie_rx_kb_per_second: number | null,
/**
 * PCIe transmit throughput in NVML's documented KB/s, over a 20 ms window.
 */
pcie_tx_kb_per_second: number | null, };

export type DeviceInfo = {
/**
 * Device index (0-based)
 */
index: number,
/**
 * Device name (e.g., "NVIDIA GeForce RTX 4060 Ti")
 */
name: string,
/**
 * Unique device identifier
 */
uuid: string,
/**
 * PCI bus ID
 */
pci_bus_id: string,
/**
 * Driver version
 */
driver_version: string,
/**
 * CUDA version (if available)
 */
cuda_version: string | null,
/**
 * Power limit in watts
 */
power_limit: number | null,
/**
 * Maximum power limit in watts
 */
power_limit_max: number | null, };

export type ProcessType = "Graphics" | "Compute" | "Mixed" | "Unknown";

export type GpuProcess = {
/**
 * Process ID
 */
pid: number,
/**
 * Process name (executable name)
 */
name: string,
/**
 * GPU memory used by this process in bytes
 */
gpu_memory: number | null,
/**
 * Process type
 */
process_type: ProcessType,
/**
 * Local account name from /etc/passwd; remote-only accounts retain the UID.
 */
user: string | null,
/**
 * Real UID reported by the operating system.
 */
uid: number | null,
/**
 * Argument boundaries are preserved; restricted or over-1-MiB values are unknown.
 */
command: Array<string> | null,
/**
 * Unix start time in milliseconds. Combine with PID to identify a process.
 */
started_at_ms: number | null,
/**
 * Age at sampling time, measured against system uptime.
 */
elapsed_seconds: number | null, };

export type GpuInfo = { device: DeviceInfo, metrics: GpuMetrics, memory: MemoryInfo | null,
/**
 * Results from successful process queries; consult `issues` for partial failures.
 */
processes: Array<GpuProcess>, sampled_at_ms: number, issues: Array<MetricIssue>, };

export type DeviceFailure = { index: number, uuid: string | null, error: SampleError, };

export type MonitorSnapshot = {
/**
 * Wire format version. Omitted by older recordings and defaulted to version 1.
 */
schema_version: number,
/**
 * Unix timestamp in milliseconds. Unchanged cached responses have the same timestamp.
 */
sampled_at_ms: number, gpus: Array<GpuInfo>, failures: Array<DeviceFailure>,
/**
 * Initialization/enumeration error, independent of per-device failures.
 */
error: SampleError | null, };
