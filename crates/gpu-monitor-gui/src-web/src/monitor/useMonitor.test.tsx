import { StrictMode } from 'react';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { MonitorSnapshot } from './models';
import { useMonitor } from './useMonitor';
import { snapshot, TIME } from '../test/fixtures';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(TIME); mockedInvoke.mockReset(); });
afterEach(() => vi.useRealTimers());

describe('monitor polling (#9)', () => {
    it('shares the pending request under StrictMode, never overlaps slow calls, and stops after unmount', async () => {
        let resolve!: (value: MonitorSnapshot) => void;
        mockedInvoke.mockImplementation(() => new Promise<MonitorSnapshot>(done => { resolve = done; }) as ReturnType<typeof invoke>);
        const { result, unmount } = renderHook(useMonitor, { wrapper: StrictMode });
        expect(mockedInvoke).toHaveBeenCalledTimes(1);
        await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
        expect(mockedInvoke).toHaveBeenCalledTimes(1);
        await act(async () => { resolve(snapshot(TIME + 5000)); });
        expect(result.current.state.devices['GPU-0'].history).toHaveLength(1);
        await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
        expect(mockedInvoke).toHaveBeenCalledTimes(2);
        unmount();
        await act(async () => { resolve(snapshot(TIME + 6000)); await vi.advanceTimersByTimeAsync(10000); });
        expect(mockedInvoke).toHaveBeenCalledTimes(2);
        expect(vi.getTimerCount()).toBe(0);
    });
    it('ages cached data and exposes string errors without dropping successful history', async () => {
        mockedInvoke.mockResolvedValue(snapshot());
        const { result } = renderHook(useMonitor);
        await act(async () => {});
        expect(result.current.status).toBe('Live');
        await act(async () => { await vi.advanceTimersByTimeAsync(4000); });
        expect(result.current.status).toBe('Stale');
        expect(result.current.state.devices['GPU-0'].history).toHaveLength(1);
        mockedInvoke.mockRejectedValue('driver disconnected');
        await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
        expect(result.current.state.error?.message).toBe('driver disconnected');
        expect(result.current.state.devices['GPU-0'].gpu.device.name).toBe('Test GPU 0');
    });
    it('preserves message-only command errors on initial connection and retry', async () => {
        mockedInvoke.mockRejectedValue({ message: 'Waiting for first GPU sample' });
        const { result } = renderHook(useMonitor);
        await act(async () => {});
        expect(result.current.status).toBe('Offline');
        expect(result.current.state.error?.message).toBe('Waiting for first GPU sample');
        mockedInvoke.mockRejectedValue({ message: 'Monitor worker unavailable' });
        await act(async () => { await result.current.retry(); });
        expect(result.current.state.error?.message).toBe('Monitor worker unavailable');
        expect(result.current.retrying).toBe(false);
    });
    it('requests backend reinitialization once for repeated retry clicks (#10)', async () => {
        let resolveRetry!: () => void;
        mockedInvoke.mockImplementation(command => command === 'get_gpu_info' ? Promise.resolve(snapshot()) as ReturnType<typeof invoke>
            : new Promise<void>(resolve => { resolveRetry = resolve; }) as ReturnType<typeof invoke>);
        const { result } = renderHook(useMonitor);
        await act(async () => {});
        act(() => { void result.current.retry(); void result.current.retry(); });
        expect(result.current.retrying).toBe(true);
        expect(mockedInvoke.mock.calls.filter(([command]) => command === 'retry_gpu_monitor')).toHaveLength(1);
        await act(async () => { resolveRetry(); });
        expect(result.current.retrying).toBe(false);
    });
});
