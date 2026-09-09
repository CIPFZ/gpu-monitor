import { useState } from 'react';
import { GpuInfo, processIssues } from '../monitor/models';
import ProcessList from './ProcessList';

export default function ProcessPanel({ gpu }: { gpu: GpuInfo }) {
    const [query, setQuery] = useState('');
    const issues = processIssues(gpu);
    const processes = gpu.processes.filter(process => process.name.toLowerCase().includes(query.toLowerCase()) || String(process.pid).includes(query));
    return <section className="expanded-process-section" aria-label={`GPU ${gpu.device.index} processes`}>
        <div className="process-toolbar">
            <h3 className="section-title-large">Processes ({gpu.processes.length}{issues.length ? ', incomplete' : ''})</h3>
            <input type="search" className="search-input" aria-label="Search processes" placeholder="Search name or PID..."
                value={query} onChange={event => setQuery(event.target.value)} />
        </div>
        <ProcessList processes={processes} issues={issues} filtered={query.length > 0} />
    </section>;
}
