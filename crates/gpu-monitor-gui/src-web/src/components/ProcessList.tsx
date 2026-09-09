import { GpuProcess, MetricIssue } from '../monitor/models';
import { elapsed } from '../monitor/selectors';

interface ProcessListProps {
    processes: GpuProcess[];
    issues: MetricIssue[];
    filtered?: boolean;
}
export default function ProcessList({ processes, issues, filtered = false }: ProcessListProps) {
    return <>
        {issues.length > 0 && <div className="data-notice" role="status">
            Process data is incomplete. {issues.map(issue => `${issue.metric}: ${issue.error.message}`).join('; ')}
        </div>}
        <div className="table-container">
            <table className="process-table">
                <thead><tr><th scope="col">PID</th><th scope="col">Process Name</th><th scope="col">User</th><th scope="col">Running</th><th scope="col">Type</th><th scope="col">Memory</th><th scope="col">Arguments</th></tr></thead>
                <tbody>{processes.length === 0 ? <tr><td colSpan={7} className="empty-message">
                    {issues.length ? 'Process list unavailable or incomplete' : filtered ? 'No matching processes' : 'No active processes'}
                </td></tr> : processes.map(process => <tr key={`${process.pid}:${process.started_at_ms ?? "unknown"}`}>
                    <td>{process.pid}</td><td>{process.name}</td><td>{process.user ?? process.uid ?? 'N/A'}</td><td>{elapsed(process.elapsed_seconds)}</td><td>{process.process_type}</td>
                    <td>{process.gpu_memory === null ? 'N/A' : `${(process.gpu_memory / 1024 ** 2).toFixed(0)} MiB`}</td>
                    <td>{process.command == null ? 'N/A' : <details><summary>Show command</summary><pre className="command-line">{process.command.map(arg => JSON.stringify(arg)).join(' ')}</pre></details>}</td>
                </tr>)}</tbody>
            </table>
        </div>
    </>;
}
