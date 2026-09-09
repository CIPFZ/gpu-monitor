import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AlertConfig, AlertEvent } from '../monitor/runtime-wire';
import { errorMessage } from '../monitor/useMonitorTools';

export default function AlertPanel({ config, events, save, busy }: {
    config: AlertConfig; events: AlertEvent[]; save: (config: AlertConfig) => Promise<unknown>; busy: boolean;
}) {
    const [draft, setDraft] = useState(config);
    const [notifications, setNotifications] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const lastNotified = useRef(0);
    const queue = useRef<number[]>([]);
    const notifying = useRef(false);
    const [skipped, setSkipped] = useState(0);
    const enabled = useRef(false);
    useEffect(() => {
        if (!notifications) return;
        const pending = events.filter(event => event.id > lastNotified.current);
        if (pending.length) lastNotified.current = Math.max(...pending.map(event => event.id));
        const combined = [...queue.current, ...pending.map(event => event.id)];
        if (combined.length > 50) setSkipped(count => count + combined.length - 50);
        queue.current = combined.slice(-50);
        const drain = async () => {
            if (notifying.current) return;
            notifying.current = true;
            try {
                while (enabled.current && queue.current.length) {
                    const eventId = queue.current.shift()!;
                    try { await invoke('notify_event', { eventId }); }
                    catch (reason) { if (enabled.current) setError(errorMessage(reason)); }
                }
            } finally { notifying.current = false; }
        };
        void drain();
    }, [events, notifications]);
    useEffect(() => () => { enabled.current = false; }, []);
    const fields: [keyof AlertConfig, string, string][] = [
        ['temperature_threshold', 'Temperature threshold (°C)', '1'], ['temperature_recovery', 'Temperature recovery (°C)', '1'],
        ['memory_threshold', 'Memory threshold (%)', '0.1'], ['memory_recovery', 'Memory recovery (%)', '0.1'],
        ['duration_ms', 'Required duration (seconds)', '1'], ['cooldown_ms', 'Cooldown (seconds)', '1'],
    ];
    return <details className="tool-panel"><summary>Alerts and events ({events.length})</summary>
        <form onSubmit={event => { event.preventDefault(); void save(draft); }} className="tool-form">
            <label><input type="checkbox" checked={draft.enabled} onChange={e => setDraft({ ...draft, enabled: e.target.checked })} /> Enable monitoring alerts</label>
            <div className="form-grid">{fields.map(([key, label, step]) => <label key={key}>{label}<input type="number" required min={0} step={step}
                value={Number(draft[key]) / (key.endsWith('_ms') ? 1000 : 1)}
                onChange={e => setDraft({ ...draft, [key]: Number(e.target.value) * (key.endsWith('_ms') ? 1000 : 1) })} /></label>)}</div>
            <button className="btn-retry" type="submit" disabled={busy}>Apply alert rules</button>
        </form>
        <label className="notification-toggle"><input type="checkbox" checked={notifications} onChange={event => {
            lastNotified.current = Math.max(0, ...events.map(item => item.id));
            queue.current = [];
            enabled.current = event.target.checked; setNotifications(event.target.checked); setError(null);
        }} /> Desktop notifications for new events while this window is open</label>
        {skipped > 0 && <p className="sample-status">{skipped} desktop notifications skipped while delivery was busy. See the event list for retained events.</p>}
        {error && <p className="data-notice" role="alert">{error}</p>}
        <ol className="event-list">{[...events].reverse().map(event => <li key={event.id}>
            <time>{new Date(event.at_ms).toLocaleTimeString()}</time> · <strong>{event.state === 'firing' ? 'Alert' : 'Recovered'}</strong> · {event.message}
        </li>)}</ol>{!events.length && <p className="sample-status">No events in this session.</p>}
    </details>;
}
