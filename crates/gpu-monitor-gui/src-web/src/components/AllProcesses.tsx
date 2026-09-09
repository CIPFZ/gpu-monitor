import { useState } from 'react';
import { GpuInfo } from '../monitor/models';
import { elapsed, groupProcesses, processMatches, processIdentity } from '../monitor/selectors';

export default function AllProcesses({ devices }: { devices: { gpu: GpuInfo; stale: boolean }[] }) {
    const [query, setQuery] = useState(''), [user, setUser] = useState('');
    const groups = groupProcesses(devices).filter(group => processMatches(group.process, query, user));
    const incomplete = devices.some(({ gpu, stale }) => stale || gpu.issues.some(issue => issue.metric.startsWith('processes')));
    return <section className="expanded-process-section" aria-label="All GPU processes">
        <div className="process-toolbar"><h2 className="section-title-large">Processes across GPUs</h2>
            <input aria-label="Search all processes" placeholder="Name or PID" value={query} onChange={e => setQuery(e.target.value)} />
            <input aria-label="Filter all processes by user" placeholder="Exact user or UID" value={user} onChange={e => setUser(e.target.value)} />
        </div>
        {incomplete && <p className="data-notice">Some device or process data is incomplete or stale.</p>}
        <div className="table-container"><table className="process-table"><thead><tr><th>PID / Name</th><th>User</th><th>Running</th><th>GPU allocations</th><th>Arguments</th></tr></thead>
            <tbody>{groups.map(({ process, allocations }) => <tr key={processIdentity(process, allocations[0].uuid)}>
                <td>{process.pid} · {process.name}</td><td>{process.user ?? process.uid ?? 'N/A'}</td><td>{elapsed(process.elapsed_seconds)}</td>
                <td>{allocations.map(a => <div key={a.uuid}>GPU {a.index}: {a.memory == null ? 'N/A' : `${(a.memory / 1024 ** 2).toFixed(0)} MiB`}{a.stale ? ' · Stale' : ''}</div>)}</td>
                <td>{process.command == null ? 'N/A' : <details><summary>Show command</summary><pre className="command-line">{process.command.map(arg => JSON.stringify(arg)).join(' ')}</pre></details>}</td>
            </tr>)}</tbody></table>{!groups.length && <p className="empty-message">{incomplete ? 'No matching processes in the available data.' : 'No matching processes.'}</p>}</div>
    </section>;
}
