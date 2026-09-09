import { describe, expect, it } from 'vitest';
import { gpu, snapshot, TIME, lost } from '../test/fixtures';
import { initialState, monitorReducer, isDeviceStale } from './state';
import { selectDevices, groupProcesses, processMatches } from './selectors';
import { attachHistory, exportSnapshot, replayState, snapshotHistory } from './replay';
import { chartSamples, chartPath } from '../components/Sparkline';
import type { GpuProcess } from './models';

export const process: GpuProcess = { pid: 10, name: 'python', gpu_memory: 1024, process_type: 'Compute', user: 'alice', uid: 1000,
    command: ['python', 'train.py', '--token=secret'], started_at_ms: TIME - 5000, elapsed_seconds: 5 };

describe('resource selection and process identity', () => {
    it('sorts real free capacity and excludes unknown and stale values from minimum filters', () => {
        const cards = [gpu(0), gpu(1), gpu(2)]; cards[2].memory = null;
        const state = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot(TIME, cards) });
        expect(selectDevices(state, TIME, '', '', 'free-memory').map(e => e.gpu.device.index)).toEqual([1, 0, 2]);
        expect(selectDevices(state, TIME, '', '20', 'index').map(e => e.gpu.device.index)).toEqual([1]);
        expect(selectDevices(state, TIME + 3001, '', '0', 'index')).toHaveLength(0);
        expect(selectDevices(state, TIME, '', 'NaN', 'index')).toHaveLength(0);
        expect(isDeviceStale(state.devices['GPU-0'], state, TIME + 8000, 5000)).toBe(false);
    });
    it('groups confirmed identities across cards and separates reused or unverified PIDs', () => {
        const first = gpu(0), second = gpu(1); first.processes = [{ ...process }]; second.processes = [{ ...process }];
        expect(groupProcesses([{ gpu: first, stale: false }, { gpu: second, stale: true }])[0].allocations).toHaveLength(2);
        second.processes[0].elapsed_seconds = 50;
        expect(groupProcesses([{ gpu: first, stale: true }, { gpu: second, stale: false }])[0].process.elapsed_seconds).toBe(50);
        second.processes[0].started_at_ms = TIME;
        expect(groupProcesses([{ gpu: first, stale: false }, { gpu: second, stale: false }])).toHaveLength(2);
        first.processes[0].started_at_ms = null; second.processes[0].started_at_ms = null;
        expect(groupProcesses([{ gpu: first, stale: false }, { gpu: second, stale: false }])).toHaveLength(2);
        expect(processMatches(process, 'py', '1000')).toBe(true);
        expect(processMatches(process, '', 'ali')).toBe(false);
    });
});
describe('history, replay and export', () => {
    it('retains peaks and gaps while reducing a long chart', () => {
        const history = Array.from({ length: 3600 }, (_, i) => ({ at: TIME - 3_599_000 + i * 1000, load: i === 7 ? 100 : i === 10 ? null : 20, memory: 50 }));
        const samples = chartSamples(history, 'load', TIME, 3_600_000, 2500);
        expect(samples.length).toBeLessThan(600);
        expect(samples.some(point => point.load === 100)).toBe(true);
        expect(chartPath(history, 'load', TIME, 100, 3_600_000).match(/M/g)).toHaveLength(2);
        expect(chartPath([{ at: TIME - 5000, load: 10, memory: 0 }, { at: TIME, load: 20, memory: 0 }], 'load', TIME, 100, 60_000, 5000).match(/M/g)).toHaveLength(1);
    });
    it('restores actual backend samples skipped by UI polling and seeks without future data', () => {
        const frames = [snapshot(TIME), snapshot(TIME + 1000), snapshot(TIME + 2000, [], lost), snapshot(TIME + 3000)];
        const end = replayState(frames, 3, 60_000);
        expect(end.devices['GPU-0'].history.map(p => p.load)).toEqual([50, 50, null, 50]);
        const start = replayState(frames, 0, 60_000);
        expect(start.devices['GPU-0'].history).toHaveLength(1);
        expect(start.devices['GPU-0'].failure).toBeNull();
        const live = monitorReducer(initialState, { type: 'snapshot', snapshot: frames[3] });
        expect(attachHistory(live, snapshotHistory(frames, 60_000)).devices['GPU-0'].history).toHaveLength(4);
    });
    it('redacts arguments by default, retains failure provenance and never mutates current state', () => {
        const card = gpu(); card.processes = [{ ...process }];
        let state = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot(TIME, [card]) });
        expect(JSON.parse(exportSnapshot(state, false)).gpus[0].processes[0].command).toBeNull();
        expect(JSON.parse(exportSnapshot(state, true)).gpus[0].processes[0].command).toContain('--token=secret');
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + 1000, []) });
        const output = JSON.parse(exportSnapshot(state, false));
        expect(output.gpus).toHaveLength(0);
        expect(output.failures[0].uuid).toBe('GPU-0');
        expect(state.failures).toHaveLength(0);
        expect(state.devices['GPU-0'].gpu.processes[0].command).toContain('--token=secret');
    });
});
