import { DeviceFailure, GpuInfo, MonitorSnapshot, SampleError, memoryPercent } from './models';

export const HISTORY_WINDOW_MS = 60_000;
export const STALE_AFTER_MS = 3_000;
export const MAX_HISTORY_MS = 3_600_000;
export interface HistoryPoint { at: number; load: number | null; memory: number | null }
export interface DeviceState { gpu: GpuInfo; history: HistoryPoint[]; failure: SampleError | null }
export interface MonitorState {
    devices: Record<string, DeviceState>;
    sampledAt: number;
    received: boolean;
    error: SampleError | null;
    failures: DeviceFailure[];
}
export const initialState: MonitorState = {
    devices: Object.create(null), sampledAt: 0, received: false, error: null, failures: [],
};
export type MonitorAction =
    | { type: 'snapshot'; snapshot: MonitorSnapshot }
    | { type: 'error'; error: SampleError; at: number };

function append(history: HistoryPoint[], point: HistoryPoint): HistoryPoint[] {
    // Cached snapshots and React StrictMode may deliver the same sample twice.
    if (history.length && point.at <= history[history.length - 1].at) return history;
    return [...history.filter(item => item.at >= point.at - MAX_HISTORY_MS), point].slice(-36_001);
}
export function monitorReducer(state: MonitorState, action: MonitorAction): MonitorState {
    if (action.type === 'error') {
        const devices = Object.fromEntries(Object.entries(state.devices).map(([uuid, entry]) => [uuid, {
            ...entry, history: append(entry.history, { at: action.at, load: null, memory: null }),
        }]));
        return { ...state, devices, received: true, error: action.error };
    }
    const snapshot = action.snapshot;
    // Requests are single-flight and ordered. A lower wall-clock timestamp is
    // a clock adjustment, not an out-of-order response; restart the time axis.
    if (snapshot.sampled_at_ms < state.sampledAt) return monitorReducer(initialState, action);
    const devices: MonitorState['devices'] = Object.assign(Object.create(null), state.devices);
    const successful = new Set(snapshot.gpus.map(gpu => gpu.device.uuid));
    for (const [uuid, entry] of Object.entries(devices)) {
        if (successful.has(uuid)) continue;
        const failure = snapshot.failures.find(f => f.uuid === uuid ||
            (f.uuid === null && f.index === entry.gpu.device.index));
        devices[uuid] = {
            ...entry,
            failure: failure?.error ?? snapshot.error ?? { kind: 'device_lost', message: 'Device is no longer available' },
            history: append(entry.history, { at: snapshot.sampled_at_ms, load: null, memory: null }),
        };
    }
    for (const gpu of snapshot.gpus) {
        const previous = devices[gpu.device.uuid];
        if (previous && gpu.sampled_at_ms < previous.gpu.sampled_at_ms) continue;
        devices[gpu.device.uuid] = {
            gpu, failure: null,
            history: append(previous?.history ?? [], {
                at: gpu.sampled_at_ms, load: gpu.metrics.gpu_utilization, memory: memoryPercent(gpu.memory),
            }),
        };
    }
    return { devices, sampledAt: snapshot.sampled_at_ms, received: true, error: snapshot.error, failures: snapshot.failures };
}
export function isDeviceStale(entry: DeviceState, state: MonitorState, now: number, intervalMs = 1000): boolean {
    return !!(state.error || entry.failure) || Math.abs(now - entry.gpu.sampled_at_ms) > Math.max(STALE_AFTER_MS, intervalMs * 3);
}
export function monitorStatus(state: MonitorState, now: number, intervalMs = 1000): 'Connecting' | 'Offline' | 'Stale' | 'Live' {
    if (!state.received) return 'Connecting';
    const entries = Object.values(state.devices);
    if (!entries.length && (state.error || state.failures.length)) return 'Offline';
    if (state.error || state.failures.length || now - state.sampledAt > Math.max(STALE_AFTER_MS, intervalMs * 3) ||
        entries.some(entry => isDeviceStale(entry, state, now, intervalMs))) return 'Stale';
    return 'Live';
}
