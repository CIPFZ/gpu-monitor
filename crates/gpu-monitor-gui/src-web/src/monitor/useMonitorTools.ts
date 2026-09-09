import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AlertConfig, AlertEvent, HistoryResponse, RecordingStatus } from './runtime-wire';

export interface MonitorTools { history: HistoryResponse; events: AlertEvent[]; alert_config: AlertConfig; recording: RecordingStatus }
export function errorMessage(error: unknown): string {
    return typeof error === 'object' && error !== null && 'message' in error ? String(error.message) : String(error);
}
export function useMonitorTools(windowMs: number) {
    const [tools, setTools] = useState<MonitorTools | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const running = useRef(false);
    const request = useRef<Promise<MonitorTools> | null>(null);
    const window = useRef(windowMs);
    window.current = windowMs;
    useEffect(() => {
        let active = true;
        let timer: ReturnType<typeof setTimeout>;
        const poll = async () => {
            const pending = request.current ?? invoke<MonitorTools>('get_monitor_tools', { windowMs: window.current });
            request.current = pending;
            try { const next = await pending; if (active) setTools(next); }
            catch (reason) { if (active) setError(errorMessage(reason)); }
            finally {
                if (request.current === pending) request.current = null;
                if (active) timer = setTimeout(poll, 2000);
            }
        };
        void poll();
        return () => { active = false; clearTimeout(timer); };
    }, []);
    const run = useCallback(async <T,>(command: string, args?: Record<string, unknown>): Promise<T | undefined> => {
        if (running.current) return;
        running.current = true; setBusy(true); setError(null);
        try {
            const result = await invoke<T>(command, args);
            if (command === 'start_recording' || command === 'stop_recording') setTools(current => current ? { ...current, recording: result as RecordingStatus } : current);
            if (command === 'configure_alerts') setTools(current => current ? { ...current, alert_config: args!.config as AlertConfig } : current);
            return result;
        }
        catch (reason) { setError(errorMessage(reason)); }
        finally { running.current = false; setBusy(false); }
    }, []);
    return { tools, error, busy, run };
}
