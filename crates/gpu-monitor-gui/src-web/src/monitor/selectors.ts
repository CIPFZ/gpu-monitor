import { GpuInfo, GpuProcess } from './models';
import { DeviceState, isDeviceStale, MonitorState } from './state';

export type GpuSort = 'index' | 'free-memory' | 'utilization' | 'temperature';
export function selectDevices(state: MonitorState, now: number, query: string, minimumGiB: string, sort: GpuSort, intervalMs = 1000): DeviceState[] {
    const minimum = minimumGiB.trim() === '' ? null : Number(minimumGiB);
    const metric = (entry: DeviceState): number | null => {
        if (isDeviceStale(entry, state, now, intervalMs)) return null;
        if (sort === 'free-memory') return entry.gpu.memory?.free ?? null;
        if (sort === 'utilization') return entry.gpu.metrics.gpu_utilization;
        return entry.gpu.metrics.temperature;
    };
    return Object.values(state.devices).filter(entry => {
        const { gpu } = entry;
        if (!`${gpu.device.name} ${gpu.device.index} ${gpu.device.uuid}`.toLowerCase().includes(query.trim().toLowerCase())) return false;
        return minimum === null || (Number.isFinite(minimum) && minimum >= 0 && !isDeviceStale(entry, state, now, intervalMs) &&
            gpu.memory !== null && gpu.memory.free >= minimum * 1024 ** 3);
    }).sort((a, b) => {
        if (sort !== 'index') {
            const left = metric(a), right = metric(b);
            if (left === null && right !== null) return 1;
            if (right === null && left !== null) return -1;
            if (left !== null && right !== null && left !== right) return right - left;
        }
        return a.gpu.device.index - b.gpu.device.index || a.gpu.device.uuid.localeCompare(b.gpu.device.uuid);
    });
}
export function processMatches(process: GpuProcess, query: string, user: string): boolean {
    const owner = user.trim().toLowerCase();
    return (!owner || process.user?.toLowerCase() === owner || String(process.uid ?? '') === owner) &&
        `${process.name} ${process.pid}`.toLowerCase().includes(query.trim().toLowerCase());
}
export function processIdentity(process: GpuProcess, uuid: string): string {
    return process.started_at_ms == null ? `${uuid}:${process.pid}:unknown` : `${process.pid}:${process.started_at_ms}`;
}
export interface ProcessGroup { process: GpuProcess; allocations: { uuid: string; index: number; memory: number | null; stale: boolean }[] }
export function groupProcesses(gpus: { gpu: GpuInfo; stale: boolean }[]): ProcessGroup[] {
    const groups = new Map<string, ProcessGroup>();
    const sources = new Map<string, { stale: boolean; at: number }>();
    for (const { gpu, stale } of gpus) for (const process of gpu.processes) {
        const key = processIdentity(process, gpu.device.uuid);
        const group = groups.get(key) ?? { process, allocations: [] };
        const source = sources.get(key);
        if (!source || (source.stale && !stale) || (source.stale === stale && gpu.sampled_at_ms > source.at)) {
            group.process = process;
            sources.set(key, { stale, at: gpu.sampled_at_ms });
        }
        group.allocations.push({ uuid: gpu.device.uuid, index: gpu.device.index, memory: process.gpu_memory, stale });
        groups.set(key, group);
    }
    return [...groups.values()].sort((a, b) => a.process.pid - b.process.pid || (a.process.started_at_ms ?? 0) - (b.process.started_at_ms ?? 0));
}
export function elapsed(seconds: number | null): string {
    if (seconds == null) return 'N/A';
    const hours = Math.floor(seconds / 3600), minutes = Math.floor(seconds % 3600 / 60);
    return hours ? `${hours}h ${minutes}m` : minutes ? `${minutes}m ${seconds % 60}s` : `${seconds}s`;
}
