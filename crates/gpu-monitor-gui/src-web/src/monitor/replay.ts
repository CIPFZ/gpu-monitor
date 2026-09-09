import { GpuInfo, MonitorSnapshot, memoryPercent } from './models';
import { HistoryPoint, initialState, MonitorState } from './state';
import { HistoryResponse } from './runtime-wire';

interface IndexedPoint extends HistoryPoint { frame: number }
interface DeviceSeries { samples: { frame: number; gpu: GpuInfo }[]; points: IndexedPoint[] }
export interface ReplayIndex { frames: MonitorSnapshot[]; devices: Record<string, DeviceSeries>; epochStarts: number[] }
function lowerBound<T>(items: T[], value: number, key: (item: T) => number, low = 0, high = items.length): number {
    while (low < high) { const mid = (low + high) >>> 1; if (key(items[mid]) < value) low = mid + 1; else high = mid; }
    return low;
}
export function buildReplayIndex(frames: MonitorSnapshot[]): ReplayIndex {
    const devices: ReplayIndex['devices'] = Object.create(null);
    const epochStarts = [0];
    frames.forEach((snapshot, frame) => {
        if (frame > 0 && snapshot.sampled_at_ms < frames[frame - 1].sampled_at_ms) epochStarts.push(frame);
        for (const gpu of snapshot.gpus) {
            const series = devices[gpu.device.uuid] ?? { samples: [], points: [] };
            const last = series.samples[series.samples.length - 1];
            if (last && last.frame < frame - 1) series.points.push({ frame: last.frame + 1, at: frames[last.frame + 1].sampled_at_ms, load: null, memory: null });
            if (!series.points.length || series.points[series.points.length - 1].frame < epochStarts[epochStarts.length - 1] || gpu.sampled_at_ms > series.points[series.points.length - 1].at) {
                series.points.push({ frame, at: gpu.sampled_at_ms, load: gpu.metrics.gpu_utilization, memory: memoryPercent(gpu.memory) });
            }
            series.samples.push({ frame, gpu });
            devices[gpu.device.uuid] = series;
        }
    });
    return { frames, devices, epochStarts };
}
export function readReplayState(index: ReplayIndex, frameIndex: number, windowMs: number): MonitorState {
    const current = index.frames[frameIndex];
    if (!current) return initialState;
    const devices: MonitorState['devices'] = Object.create(null);
    const epochStart = index.epochStarts[lowerBound(index.epochStarts, frameIndex + 1, frame => frame) - 1];
    for (const [uuid, series] of Object.entries(index.devices)) {
        const position = lowerBound(series.samples, frameIndex + 1, sample => sample.frame) - 1;
        if (position < 0) continue;
        const sample = series.samples[position];
        const end = lowerBound(series.points, frameIndex + 1, point => point.frame);
        const epochBegin = lowerBound(series.points, epochStart, point => point.frame);
        const begin = lowerBound(series.points, current.sampled_at_ms - windowMs, point => point.at, epochBegin, end);
        const history: HistoryPoint[] = series.points.slice(begin, end);
        let failure = null;
        if (sample.frame < frameIndex) {
            failure = current.failures.find(f => f.uuid === uuid || (f.uuid === null && f.index === sample.gpu.device.index))?.error
                ?? current.error ?? { kind: 'device_lost' as const, message: 'Device is no longer available' };
            const gapAt = index.frames[sample.frame + 1].sampled_at_ms;
            if (sample.frame + 1 >= epochStart && gapAt >= current.sampled_at_ms - windowMs && (!history.length || history[history.length - 1].at < gapAt)) history.push({ at: gapAt, load: null, memory: null });
        }
        devices[uuid] = { gpu: sample.gpu, history, failure };
    }
    return { devices, sampledAt: current.sampled_at_ms, received: true, error: current.error, failures: current.failures };
}
export function replayState(frames: MonitorSnapshot[], index: number, windowMs: number): MonitorState {
    return readReplayState(buildReplayIndex(frames), index, windowMs);
}
export function attachHistory(state: MonitorState, history: HistoryResponse | undefined): MonitorState {
    if (!history?.frames.length || history.frames[history.frames.length - 1].sampled_at_ms > state.sampledAt) return state;
    let epochStart = 0;
    history.frames.forEach((frame, index) => {
        if (index > 0 && frame.sampled_at_ms < history.frames[index - 1].sampled_at_ms) epochStart = index;
    });
    const frames = history.frames.slice(epochStart);
    const devices: MonitorState['devices'] = Object.assign(Object.create(null), state.devices);
    for (const [uuid, entry] of Object.entries(devices)) {
        const points = frames.map(frame => {
            const gpu = frame.gpus.find(item => item.uuid === uuid);
            return { at: frame.sampled_at_ms, load: gpu?.gpu_utilization ?? null, memory: gpu?.memory_percent ?? null };
        });
        const cutoff = points[points.length - 1]?.at ?? 0;
        devices[uuid] = { ...entry, history: [...points, ...entry.history.filter(point => point.at > cutoff)] };
    }
    return { ...state, devices };
}
export function exportSnapshot(state: MonitorState, includeCommands: boolean): string {
    const snapshot: MonitorSnapshot = { schema_version: 1, sampled_at_ms: state.sampledAt,
        gpus: Object.values(state.devices).filter(entry => !entry.failure && entry.gpu.sampled_at_ms === state.sampledAt).map(entry => ({ ...entry.gpu, processes: entry.gpu.processes.map(process => ({ ...process, command: includeCommands ? process.command : null })) })),
        failures: [...state.failures], error: state.error };
    // A retained device must not silently become a healthy live export.
    for (const entry of Object.values(state.devices)) if (entry.failure && !snapshot.failures.some(f => f.uuid === entry.gpu.device.uuid)) {
        snapshot.failures.push({ uuid: entry.gpu.device.uuid, index: entry.gpu.device.index, error: entry.failure });
    }
    return JSON.stringify(snapshot, null, 2);
}
export function snapshotHistory(frames: MonitorSnapshot[], windowMs: number): HistoryResponse {
    const gaps = frames.slice(1).map((frame, index) => frame.sampled_at_ms - frames[index].sampled_at_ms).filter(gap => gap > 0).sort((a, b) => a - b);
    return { interval_ms: gaps[Math.floor(gaps.length / 2)] ?? 1000, window_ms: windowMs, frames: frames.map(frame => ({ sampled_at_ms: frame.sampled_at_ms, gpus: frame.gpus.map(gpu => ({
        uuid: gpu.device.uuid, index: gpu.device.index, gpu_utilization: gpu.metrics.gpu_utilization,
        memory_percent: memoryPercent(gpu.memory), temperature: gpu.metrics.temperature, power_watts: gpu.metrics.power_usage === null ? null : gpu.metrics.power_usage / 1000,
    })) })) };
}
