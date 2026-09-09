import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MonitorSnapshot, SampleError } from './models';
import { initialState, monitorReducer, monitorStatus } from './state';

function asError(error: unknown): SampleError {
    if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') {
        return { kind: 'kind' in error ? (error as SampleError).kind : 'unknown', message: error.message };
    }
    return { kind: 'unknown', message: error instanceof Error ? error.message : String(error) };
}

export function useMonitor() {
    const [state, dispatch] = useReducer(monitorReducer, initialState);
    const [now, setNow] = useState(Date.now);
    const [retrying, setRetrying] = useState(false);
    const mounted = useRef(false);
    const inFlight = useRef<Promise<MonitorSnapshot> | null>(null);
    const retryInFlight = useRef(false);

    useEffect(() => {
        let active = true;
        let timer: ReturnType<typeof setTimeout> | undefined;
        mounted.current = true;
        const poll = async () => {
            // Retain the promise through StrictMode's effect cleanup/setup cycle.
            // Tauri invoke has no cancellation API; cleanup ignores its result.
            const request = inFlight.current ?? invoke<MonitorSnapshot>('get_gpu_info');
            inFlight.current = request;
            try {
                const snapshot = await request;
                if (active) dispatch({ type: 'snapshot', snapshot });
            } catch (error) {
                if (active) dispatch({ type: 'error', error: asError(error), at: Date.now() });
            } finally {
                if (inFlight.current === request) inFlight.current = null;
                if (active) timer = setTimeout(poll, 1000);
            }
        };
        void poll();
        // Advance freshness and the time axis even while a request is stalled.
        const clock = setInterval(() => setNow(Date.now()), 1000);
        return () => {
            active = false;
            mounted.current = false;
            clearTimeout(timer);
            clearInterval(clock);
        };
    }, []);

    const retry = useCallback(async () => {
        if (retryInFlight.current) return;
        retryInFlight.current = true;
        setRetrying(true);
        try {
            await invoke('retry_gpu_monitor');
        } catch (error) {
            if (mounted.current) dispatch({ type: 'error', error: asError(error), at: Date.now() });
        } finally {
            retryInFlight.current = false;
            if (mounted.current) setRetrying(false);
        }
    }, []);
    return { state, now, status: monitorStatus(state, now), retry, retrying };
}
