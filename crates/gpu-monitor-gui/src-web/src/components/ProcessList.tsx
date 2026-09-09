import { GpuProcess, MetricIssue } from '../monitor/models';

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
                <thead><tr><th scope="col">PID</th><th scope="col">Process Name</th><th scope="col">Type</th><th scope="col">Memory</th></tr></thead>
                <tbody>{processes.length === 0 ? <tr><td colSpan={4} className="empty-message">
                    {issues.length ? 'Process list unavailable or incomplete' : filtered ? 'No matching processes' : 'No active processes'}
                </td></tr> : processes.map(process => <tr key={process.pid}>
                    <td>{process.pid}</td><td>{process.name}</td><td>{process.process_type}</td>
                    <td>{process.gpu_memory === null ? 'N/A' : `${(process.gpu_memory / 1024 ** 2).toFixed(0)} MiB`}</td>
                </tr>)}</tbody>
            </table>
        </div>
    </>;
}
