import { useState } from 'react';
import type { MonitorSnapshot } from '../monitor/models';
import type { RecordingStatus } from '../monitor/runtime-wire';

export default function RecordingPanel({ recording, busy, run, onReplay, onExport }: {
    recording: RecordingStatus | undefined; busy: boolean;
    run: <T>(command: string, args?: Record<string, unknown>) => Promise<T | undefined>;
    onReplay: (frames: MonitorSnapshot[]) => void; onExport: (includeCommands: boolean) => void;
}) {
    const [path, setPath] = useState('');
    const [includeCommands, setIncludeCommands] = useState(false);
    return <details className="tool-panel"><summary>Recording, replay and export{recording?.active ? ' · Recording' : recording?.finishing ? ' · Finishing' : ''}</summary>
        <div className="tool-form">
            <label>Recording file path<input type="text" value={path} placeholder="/home/user/gpu-session.jsonl" onChange={e => setPath(e.target.value)} /></label>
            <label><input type="checkbox" checked={includeCommands} onChange={e => setIncludeCommands(e.target.checked)} /> Include full process arguments in recording and snapshot export</label>
            <p className="sample-status">Arguments may contain credentials. Files keep all GPUs; display filters do not change a recording. Maximum 64 MiB or 24 hours per file.</p>
            <div className="toolbar">
                <button className="btn-retry" disabled={busy || !path.trim() || recording?.active || recording?.finishing}
                    onClick={() => void run('start_recording', { path: path.trim(), includeCommands })}>Start recording</button>
                <button className="btn-retry" disabled={busy || !recording?.active} onClick={() => void run('stop_recording')}>Stop recording</button>
                <button className="btn-retry" disabled={busy || !path.trim() || recording?.active || recording?.finishing}
                    onClick={async () => { const frames = await run<MonitorSnapshot[]>('load_recording', { path: path.trim() }); if (frames?.length) onReplay(frames); }}>Open replay</button>
                <button className="btn-retry" onClick={() => onExport(includeCommands)}>Export snapshot</button>
            </div>
            {recording?.path && <p className="sample-status">{recording.path} · {recording.samples_written} samples · {(recording.bytes_written / 1024 ** 2).toFixed(2)} MiB · {recording.dropped_samples} dropped{recording.finishing ? ' · Finishing writes…' : ''}</p>}
            {recording?.error && <p className="data-notice" role="alert">{recording.error}</p>}
        </div>
    </details>;
}
