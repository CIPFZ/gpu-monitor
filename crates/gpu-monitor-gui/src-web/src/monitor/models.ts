// Wire types shared with gpu-monitor-core. Missing measurements are never zero.
export interface SampleError {
    kind: 'not_supported' | 'permission_denied' | 'device_lost' | 'uninitialized' | 'no_devices' | 'unknown';
    message: string;
}
export interface MetricIssue { metric: string; error: SampleError }
export interface MemoryInfo { total: number; used: number; free: number }
export interface GpuMetrics {
    gpu_utilization: number | null;
    memory_utilization: number | null;
    encoder_utilization: number | null;
    decoder_utilization: number | null;
    temperature: number | null;
    power_usage: number | null;
    fan_speed: number | null;
    clock_graphics: number | null;
    clock_memory: number | null;
    clock_sm: number | null;
}
export interface DeviceInfo {
    index: number;
    name: string;
    uuid: string;
    pci_bus_id: string;
    driver_version: string;
    cuda_version: string | null;
    power_limit: number | null;
    power_limit_max: number | null;
}
export interface GpuProcess {
    pid: number;
    name: string;
    gpu_memory: number | null;
    process_type: 'Graphics' | 'Compute' | 'Mixed' | 'Unknown';
}
export interface GpuInfo {
    device: DeviceInfo;
    metrics: GpuMetrics;
    memory: MemoryInfo | null;
    processes: GpuProcess[];
    sampled_at_ms: number;
    issues: MetricIssue[];
}
export interface DeviceFailure { index: number; uuid: string | null; error: SampleError }
export interface MonitorSnapshot {
    sampled_at_ms: number;
    gpus: GpuInfo[];
    failures: DeviceFailure[];
    error: SampleError | null;
}
export function memoryPercent(memory: MemoryInfo | null): number | null {
    return memory && memory.total > 0 ? memory.used / memory.total * 100 : null;
}
export function processIssues(gpu: GpuInfo): MetricIssue[] {
    return gpu.issues.filter(issue => issue.metric.startsWith('processes'));
}
