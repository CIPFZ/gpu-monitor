import { useEffect, useRef, useState } from 'react';
import GpuCard from './components/GpuCard';
import { useMonitor } from './monitor/useMonitor';
import { isDeviceStale } from './monitor/state';

export default function App() {
    const { state, now, status, retry, retrying } = useMonitor();
    const [query, setQuery] = useState('');
    const [selectedUuid, setSelectedUuid] = useState<string | null>(null);
    const detailContainer = useRef<HTMLDivElement>(null);
    const entries = Object.values(state.devices).sort((a, b) => a.gpu.device.index - b.gpu.device.index);
    const selected = selectedUuid ? state.devices[selectedUuid] : entries.length === 1 ? entries[0] : undefined;
    const filtered = entries.filter(({ gpu }) => `${gpu.device.name} ${gpu.device.index} ${gpu.device.uuid}`.toLowerCase().includes(query.toLowerCase()));
    const unknownFailures = state.failures.filter(failure => !entries.some(({ gpu }) => failure.uuid === gpu.device.uuid || (failure.uuid === null && failure.index === gpu.device.index)));
    useEffect(() => { if (selectedUuid) detailContainer.current?.focus(); }, [selectedUuid]);

    return <div className="app-container">
        <header className="app-header">
            <div className="app-icon" aria-hidden="true">G</div>
            <div className="header-content"><h1 className="app-title">GPU Monitor</h1><div className="app-subtitle">Real-time Performance</div></div>
            <div className="header-controls">
                {entries.length > 1 && !selected && <div className="gpu-search">
                    <input type="search" aria-label="Filter GPUs" placeholder={`Filter ${entries.length} GPUs...`} value={query} onChange={event => setQuery(event.target.value)} />
                </div>}
                <div className={`status-badge status-${status.toLowerCase()}`} role="status"><span className="status-dot" /><span>{status}</span></div>
                {status !== 'Live' && <button className="btn-retry" onClick={() => void retry()} disabled={retrying}>{retrying ? 'Retrying…' : 'Retry'}</button>}
            </div>
        </header>
        {state.error && <p className="data-notice" role="alert">{state.error.message}{entries.length > 0 ? ' · Showing the last successful samples.' : ''}</p>}
        {unknownFailures.map(failure => <p className="data-notice" role="alert" key={failure.uuid ?? failure.index}>GPU {failure.index}: {failure.error.message}</p>)}
        {selected ? <div ref={detailContainer} tabIndex={-1} className="gpu-expanded-container" aria-label="GPU details">
            {entries.length > 1 && <button className="btn-back" onClick={() => setSelectedUuid(null)}>← All GPUs</button>}
            <GpuCard key={selected.gpu.device.uuid} entry={selected} now={now} stale={isDeviceStale(selected, state, now)} mode="expanded" />
        </div> : <main className="gpu-grid">
            {filtered.map(entry => <GpuCard key={entry.gpu.device.uuid} entry={entry} now={now} stale={isDeviceStale(entry, state, now)} onDetails={() => setSelectedUuid(entry.gpu.device.uuid)} />)}
            {!filtered.length && <p className="empty-message">{!state.received ? 'Connecting to GPU monitor…' : entries.length ? `No GPUs matching “${query}”` : state.error || state.failures.length ? 'GPU data is unavailable. Monitoring will retry automatically.' : 'No GPUs detected.'}</p>}
        </main>}
    </div>;
}
