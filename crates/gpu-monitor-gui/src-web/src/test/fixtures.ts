import { GpuInfo, MonitorSnapshot, SampleError } from '../monitor/models';
export const TIME = 1_800_000_000_000;
export const lost: SampleError = { kind: 'device_lost', message: 'GPU fell off the bus' };
export function gpu(index = 0, at = TIME): GpuInfo {
    return {
        device: { index, name: `Test GPU ${index}`, uuid: `GPU-${index}`, pci_bus_id: `0000:0${index}:00.0`, driver_version: '550', cuda_version: '12', power_limit: 300, power_limit_max: 350 },
        metrics: { gpu_utilization: 50, memory_utilization: 20, encoder_utilization: 0, decoder_utilization: 0, temperature: 65, power_usage: 125000, fan_speed: 35, clock_graphics: 1500, clock_memory: 7000, clock_sm: 1500 },
        memory: { used: 8 * 1024 ** 3, total: (index ? 48 : 24) * 1024 ** 3, free: (index ? 40 : 16) * 1024 ** 3 },
        processes: [], sampled_at_ms: at, issues: [],
    };
}
export function snapshot(at = TIME, gpus = [gpu(0, at)], error: SampleError | null = null): MonitorSnapshot {
    return { sampled_at_ms: at, gpus, failures: [], error };
}
