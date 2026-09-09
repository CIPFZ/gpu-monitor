import { describe, expect, it } from 'vitest';
import { chartPath } from '../components/Sparkline';
import { gpu, lost, snapshot, TIME } from '../test/fixtures';
import { initialState, monitorReducer, monitorStatus } from './state';

describe('sample history (#3)', () => {
    it('records every stable sample, deduplicates cached timestamps, and rejects old snapshots', () => {
        let state = initialState;
        for (let second = 0; second < 5; second++) state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + second * 1000) });
        expect(state.devices['GPU-0'].history.map(point => point.load)).toEqual([50, 50, 50, 50, 50]);
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + 4000) });
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME) });
        expect(state.devices['GPU-0'].history).toHaveLength(5);
        expect(state.devices['GPU-0'].gpu.sampled_at_ms).toBe(TIME + 4000);
    });
    it('bounds history by elapsed time instead of value changes or sample count', () => {
        let state = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot() });
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + 61000) });
        expect(state.devices['GPU-0'].history).toHaveLength(1);
    });
    it('uses actual timestamp positions and breaks paths on unknown samples and long gaps', () => {
        const points = [
            { at: TIME - 60000, load: 10, memory: 10 },
            { at: TIME - 59000, load: 20, memory: 10 },
            { at: TIME - 58000, load: null, memory: 10 },
            { at: TIME - 57000, load: 30, memory: 10 },
            { at: TIME, load: 50, memory: 10 },
        ];
        expect(chartPath(points, 'load', TIME, 100)).toBe('M 0.000,90.000 L 1.667,80.000 M 5.000,70.000 M 100.000,50.000');
        expect(chartPath(points, 'load', TIME + 61000, 100)).toBe('');
    });
});
describe('device isolation and recovery (#8)', () => {
    it('preserves UUID history and good cards through device/global failures, then recovers', () => {
        let state = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot(TIME, [gpu(0), gpu(1)]) });
        const partial = snapshot(TIME + 1000, [gpu(1, TIME + 1000)]);
        partial.failures.push({ index: 0, uuid: 'GPU-0', error: lost });
        state = monitorReducer(state, { type: 'snapshot', snapshot: partial });
        expect(state.devices['GPU-0'].gpu.sampled_at_ms).toBe(TIME);
        expect(state.devices['GPU-0'].failure).toEqual(lost);
        expect(state.devices['GPU-1'].history).toHaveLength(2);
        expect(monitorStatus(state, TIME + 1000)).toBe('Stale');
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + 2000, [], lost) });
        expect(Object.keys(state.devices)).toHaveLength(2);
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + 3000, [gpu(0, TIME + 3000), gpu(1, TIME + 3000)]) });
        expect(state.devices['GPU-0'].history.map(point => point.load)).toEqual([50, null, null, 50]);
        expect(state.devices['GPU-0'].failure).toBeNull();
        expect(monitorStatus(state, TIME + 3000)).toBe('Live');
    });
    it('uses UUID when devices reorder and does not give a replacement card the old history', () => {
        let state = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot() });
        const replacement = gpu(0, TIME + 1000); replacement.device.uuid = 'replacement';
        state = monitorReducer(state, { type: 'snapshot', snapshot: snapshot(TIME + 1000, [replacement]) });
        expect(state.devices.replacement.history).toHaveLength(1);
        expect(state.devices['GPU-0'].failure?.kind).toBe('device_lost');
    });
    it('distinguishes connecting, offline, live and aged cached data', () => {
        expect(monitorStatus(initialState, TIME)).toBe('Connecting');
        const offline = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot(TIME, [], lost) });
        expect(monitorStatus(offline, TIME)).toBe('Offline');
        const live = monitorReducer(initialState, { type: 'snapshot', snapshot: snapshot() });
        expect(monitorStatus(live, TIME)).toBe('Live');
        expect(monitorStatus(live, TIME + 3001)).toBe('Stale');
    });
});
