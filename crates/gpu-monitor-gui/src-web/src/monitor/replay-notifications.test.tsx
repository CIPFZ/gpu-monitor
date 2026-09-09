import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import AlertPanel from '../components/AlertPanel';
import { buildReplayIndex, readReplayState } from './replay';
import { gpu, snapshot, TIME } from '../test/fixtures';
import type { AlertConfig, AlertEvent } from './runtime-wire';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
const config: AlertConfig = { enabled: true, temperature_threshold: 85, temperature_recovery: 80,
    memory_threshold: 95, memory_recovery: 90, duration_ms: 10_000, cooldown_ms: 60_000 };
const event = (id: number): AlertEvent => ({ id, at_ms: TIME + id, gpu_uuid: null,
    kind: 'monitor_unavailable', state: 'firing', message: `Offline ${id}`, value: null });
const panel = (events: AlertEvent[]) => <AlertPanel config={config} events={events} busy={false} save={async () => undefined} />;
beforeEach(() => mockedInvoke.mockReset());

describe('notification delivery bounds', () => {
    it('skips pre-existing events and bounds a stalled delivery to one in flight plus fifty queued', async () => {
        let release!: () => void;
        mockedInvoke.mockImplementationOnce(() => new Promise<void>(resolve => { release = resolve; })).mockResolvedValue(undefined);
        const { rerender } = render(panel([event(1), event(2)]));
        fireEvent.click(screen.getByLabelText(/Desktop notifications/));
        expect(mockedInvoke).not.toHaveBeenCalled();
        rerender(panel([event(1), event(2), event(3)]));
        expect(mockedInvoke).toHaveBeenCalledExactlyOnceWith('notify_event', { eventId: 3 });
        const many = Array.from({ length: 123 }, (_, index) => event(index + 1));
        rerender(panel(many));
        expect(mockedInvoke).toHaveBeenCalledTimes(1);
        expect(screen.getByText(/70 desktop notifications skipped/)).toBeInTheDocument();
        await act(async () => { release(); });
        await waitFor(() => expect(mockedInvoke).toHaveBeenCalledTimes(51));
        expect(mockedInvoke.mock.calls[1]).toEqual(['notify_event', { eventId: 74 }]);
        expect(mockedInvoke.mock.calls[50]).toEqual(['notify_event', { eventId: 123 }]);
        rerender(panel([...many]));
        expect(mockedInvoke).toHaveBeenCalledTimes(51);
    });

    it('disabling notifications cancels queued deliveries after the current request completes', async () => {
        let release!: () => void;
        mockedInvoke.mockImplementationOnce(() => new Promise<void>(resolve => { release = resolve; })).mockResolvedValue(undefined);
        const { rerender } = render(panel([]));
        fireEvent.click(screen.getByLabelText(/Desktop notifications/));
        rerender(panel([event(1)]));
        rerender(panel([event(1), event(2), event(3)]));
        fireEvent.click(screen.getByLabelText(/Desktop notifications/));
        await act(async () => { release(); });
        expect(mockedInvoke).toHaveBeenCalledTimes(1);
    });
});

describe('recording seek boundaries', () => {
    it('keeps device appearance and outage boundaries correct when seeking in either direction', () => {
        const frames = [snapshot(TIME, [gpu(0, TIME)]),
            snapshot(TIME + 1000, [gpu(0, TIME + 1000), gpu(1, TIME + 1000)]),
            snapshot(TIME + 2000, []), snapshot(TIME + 3000, [gpu(0, TIME + 3000)])];
        const before = JSON.stringify(frames);
        const index = buildReplayIndex(frames);
        const end = readReplayState(index, 3, 60_000);
        expect(end.devices['GPU-0'].failure).toBeNull();
        expect(end.devices['GPU-1'].failure?.kind).toBe('device_lost');
        expect(end.devices['GPU-0'].history.map(point => [point.at, point.load])).toEqual([
            [TIME, 50], [TIME + 1000, 50], [TIME + 2000, null], [TIME + 3000, 50]]);
        const missing = readReplayState(index, 2, 60_000);
        expect(Object.values(missing.devices).every(entry => entry.failure?.kind === 'device_lost')).toBe(true);
        expect(Object.keys(readReplayState(index, 0, 60_000).devices)).toEqual(['GPU-0']);
        expect(JSON.stringify(frames)).toBe(before);
    });

    it('seeks a long recording through its index without rescanning the source prefix', () => {
        const frames = Array.from({ length: 10_000 }, (_, index) => snapshot(TIME + index * 1000));
        const index = buildReplayIndex(frames);
        Object.defineProperty(frames[0], 'gpus', { get() { throw new Error('Full source prefix was revisited'); } });
        const state = readReplayState(index, 9999, 60_000);
        expect(state.devices['GPU-0'].gpu.sampled_at_ms).toBe(TIME + 9_999_000);
        expect(state.devices['GPU-0'].history).toHaveLength(61);
        expect(state.devices['GPU-0'].history[0].at).toBe(TIME + 9_939_000);
    });

    it('treats recording device identifiers as plain keys even when they match object prototype names', () => {
        const identifiers = ['__proto__', 'constructor', 'toString'];
        const devices = identifiers.map((uuid, index) => ({ ...gpu(index), device: { ...gpu(index).device, uuid } }));
        const state = readReplayState(buildReplayIndex([snapshot(TIME, devices)]), 0, 60_000);
        expect(Object.keys(state.devices)).toEqual(identifiers);
        for (const uuid of identifiers) expect(state.devices[uuid].gpu.device.uuid).toBe(uuid);
    });
});
