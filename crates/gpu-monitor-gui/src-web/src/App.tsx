import { useEffect, useMemo, useRef, useState } from 'react';
import GpuCard from './components/GpuCard';
import GpuTable from './components/GpuTable';
import AllProcesses from './components/AllProcesses';
import AlertPanel from './components/AlertPanel';
import RecordingPanel from './components/RecordingPanel';
import { useMonitor } from './monitor/useMonitor';
import { useMonitorTools } from './monitor/useMonitorTools';
import { isDeviceStale } from './monitor/state';
import { GpuSort, selectDevices } from './monitor/selectors';
import { attachHistory, buildReplayIndex, exportSnapshot, readReplayState, snapshotHistory } from './monitor/replay';
import type { MonitorSnapshot } from './monitor/models';

export default function App() {
    const live = useMonitor();
    const [windowMs, setWindowMs] = useState(60_000);
    const { tools, error, busy, run } = useMonitorTools(windowMs);
    const [query, setQuery] = useState(''), [minimum, setMinimum] = useState('');
    const [sort, setSort] = useState<GpuSort>('index');
    const [view, setView] = useState<'cards' | 'table' | 'processes'>('cards');
    const [selectedUuid, setSelectedUuid] = useState<string | null>(null);
    const [replay, setReplay] = useState<MonitorSnapshot[] | null>(null);
    const [frameIndex, setFrameIndex] = useState(0), [playing, setPlaying] = useState(false), [speed, setSpeed] = useState(1);
    const detailContainer = useRef<HTMLDivElement>(null);
    const replayInterval = useMemo(() => replay ? snapshotHistory(replay.slice(0, 100), windowMs).interval_ms : 1000, [replay, windowMs]);
    const indexedReplay = useMemo(() => replay ? buildReplayIndex(replay) : null, [replay]);
    const recordedState = useMemo(() => indexedReplay ? readReplayState(indexedReplay, frameIndex, windowMs) : null, [indexedReplay, frameIndex, windowMs]);
    const liveState = useMemo(() => attachHistory(live.state, tools?.history), [live.state, tools?.history]);
    const state = recordedState ?? liveState;
    const now = replay ? replay[frameIndex].sampled_at_ms : live.now;
    const intervalMs = replay ? replayInterval : tools?.history.interval_ms ?? 1000;
    const entries = Object.values(state.devices).sort((a, b) => a.gpu.device.index - b.gpu.device.index);
    const selected = selectedUuid ? state.devices[selectedUuid] : entries.length === 1 && view === 'cards' && !query && !minimum ? entries[0] : undefined;
    const filtered = selectDevices(state, now, query, minimum, sort, intervalMs);
    const unknownFailures = state.failures.filter(failure => !entries.some(({ gpu }) => failure.uuid === gpu.device.uuid || (failure.uuid === null && failure.index === gpu.device.index)));
    const status = replay ? 'Replay' : live.status;
    useEffect(() => { if (selectedUuid) detailContainer.current?.focus(); }, [selectedUuid]);
    useEffect(() => {
        if (!replay || !playing) return;
        if (frameIndex >= replay.length - 1) { setPlaying(false); return; }
        const delay = Math.max(1, (replay[frameIndex + 1].sampled_at_ms - replay[frameIndex].sampled_at_ms) / speed);
        const timer = setTimeout(() => setFrameIndex(index => index + 1), delay);
        return () => clearTimeout(timer);
    }, [replay, playing, frameIndex, speed]);
    const exportCurrent = (includeCommands: boolean) => {
        const url = URL.createObjectURL(new Blob([exportSnapshot(state, includeCommands)], { type: 'application/json' }));
        const link = document.createElement('a'); link.href = url; link.download = `gpu-snapshot-${state.sampledAt}.json`; link.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
    };
    return <div className="app-container">
        <header className="app-header">
            <div className="app-icon" aria-hidden="true">G</div>
            <div className="header-content"><h1 className="app-title">GPU Monitor</h1><div className="app-subtitle">{replay ? 'Recorded session' : 'Real-time Performance'}</div></div>
            <div className="header-controls">
                <div className={`status-badge status-${status.toLowerCase()}`} role="status"><span className="status-dot" /><span>{status}</span></div>
                {!replay && live.status !== 'Live' && <button className="btn-retry" onClick={() => void live.retry()} disabled={live.retrying}>{live.retrying ? 'Retrying…' : 'Retry'}</button>}
            </div>
        </header>
        <div className="toolbar" aria-label="Monitoring views">
            <label>View<select aria-label="View" value={view} onChange={event => { setView(event.target.value as typeof view); setSelectedUuid(null); }}>
                <option value="cards">Cards</option><option value="table">GPU table</option><option value="processes">All processes</option>
            </select></label>
            <label>History<select aria-label="History" value={windowMs} onChange={event => setWindowMs(Number(event.target.value))}>
                <option value={60_000}>1 minute</option><option value={300_000}>5 minutes</option><option value={3_600_000}>1 hour</option>
            </select></label>
            {!selected && <>
                <input type="search" aria-label="Filter GPUs" placeholder="GPU name, index or UUID" value={query} onChange={event => setQuery(event.target.value)} />
                <label>Minimum free GiB<input type="number" min={0} step="any" value={minimum} onChange={event => setMinimum(event.target.value)} /></label>
                <label>Sort GPUs<select aria-label="Sort GPUs" value={sort} onChange={event => setSort(event.target.value as GpuSort)}>
                    <option value="index">GPU index</option><option value="free-memory">Free memory ↓</option><option value="utilization">GPU load ↓</option><option value="temperature">Temperature ↓</option>
                </select></label>
            </>}
        </div>
        <RecordingPanel recording={tools?.recording} busy={busy} run={run} onExport={exportCurrent}
            onReplay={frames => { setReplay(frames); setFrameIndex(0); setPlaying(false); setSelectedUuid(null); }} />
        {replay ? <section className="replay-toolbar toolbar" aria-label="Replay controls">
            <button className="btn-retry" disabled={frameIndex === replay.length - 1 && !playing} onClick={() => setPlaying(!playing)}>{playing ? 'Pause' : 'Play'}</button>
            <label>Frame<input aria-label="Replay frame" type="range" min={0} max={replay.length - 1} value={frameIndex}
                onChange={event => { setPlaying(false); setFrameIndex(Number(event.target.value)); }} /></label>
            <span>{frameIndex + 1} / {replay.length} · {new Date(now).toLocaleString()}</span>
            <label>Speed<select aria-label="Speed" value={speed} onChange={event => setSpeed(Number(event.target.value))}>{[0.5, 1, 2, 4, 8].map(value => <option key={value} value={value}>{value}×</option>)}</select></label>
            <button className="btn-retry" onClick={() => { setReplay(null); setPlaying(false); setSelectedUuid(null); }}>Return to live</button>
        </section> : tools && <AlertPanel config={tools.alert_config} events={tools.events} busy={busy} save={config => run('configure_alerts', { config })} />}
        {error && <p className="data-notice" role="alert">{error}</p>}
        {state.error && <p className="data-notice" role="alert">{state.error.message}{entries.length > 0 ? ' · Showing the last successful samples.' : ''}</p>}
        {unknownFailures.map(failure => <p className="data-notice" role="alert" key={failure.uuid ?? failure.index}>GPU {failure.index}: {failure.error.message}</p>)}
        {selected ? <div ref={detailContainer} tabIndex={-1} className="gpu-expanded-container" aria-label="GPU details">
            {entries.length > 1 && <button className="btn-back" onClick={() => setSelectedUuid(null)}>← All GPUs</button>}
            <GpuCard key={selected.gpu.device.uuid} entry={selected} now={now} windowMs={windowMs} intervalMs={intervalMs} stale={isDeviceStale(selected, state, now, intervalMs)} mode="expanded" />
        </div> : view === 'table' ? <GpuTable entries={filtered} stale={entry => isDeviceStale(entry, state, now, intervalMs)} onDetails={setSelectedUuid} />
            : view === 'processes' ? <AllProcesses devices={filtered.map(entry => ({ gpu: entry.gpu, stale: isDeviceStale(entry, state, now, intervalMs) }))} />
            : <main className="gpu-grid">
                {filtered.map(entry => <GpuCard key={entry.gpu.device.uuid} entry={entry} now={now} windowMs={windowMs} intervalMs={intervalMs} stale={isDeviceStale(entry, state, now, intervalMs)} onDetails={() => setSelectedUuid(entry.gpu.device.uuid)} />)}
                {!filtered.length && <p className="empty-message">{!state.received ? 'Connecting to GPU monitor…' : entries.length ? 'No GPUs match these filters.' : state.error || state.failures.length ? 'GPU data is unavailable. Monitoring will retry automatically.' : 'No GPUs detected.'}</p>}
            </main>}
    </div>;
}
