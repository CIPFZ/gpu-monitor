interface GpuProcess {
    pid: number;
    name: string;
    gpu_memory: number;
    process_type: 'Graphics' | 'Compute' | 'Mixed' | 'Unknown';
}

interface ProcessListProps {
    processes: GpuProcess[];
}

function ProcessList({ processes }: ProcessListProps) {
    const formatMemory = (bytes: number) => {
        const mib = bytes / (1024 * 1024);
        return `${mib.toFixed(0)} MiB`;
    };

    const getTypeTag = (type: string) => {
        switch (type) {
            case 'Graphics': return <span className="tag gfx">Graphics</span>;
            case 'Compute': return <span className="tag comp">Compute</span>;
            case 'Mixed': return <span className="tag">Mixed</span>;
            default: return <span className="tag">Unknown</span>;
        }
    };

    return (
        <div className="table-container">
            <table className="process-table">
                <thead>
                <tr>
                    <th className="col-pid">PID</th>
                    <th>Process Name</th>
                    <th className="col-type">Type</th>
                    <th className="col-mem">Memory</th>
                </tr>
                </thead>
                <tbody>
                {processes.length === 0 ? (
                    <tr>
                        <td colSpan={4} style={{ textAlign: 'center', color: 'var(--text-secondary)', padding: '32px' }}>
                            No processes found
                        </td>
                    </tr>
                ) : (
                    processes.map((proc) => (
                        <tr key={proc.pid}>
                            <td className="col-pid">{proc.pid}</td>
                            <td>{proc.name}</td>
                            <td className="col-type">{getTypeTag(proc.process_type)}</td>
                            <td className="col-mem">{formatMemory(proc.gpu_memory)}</td>
                        </tr>
                    ))
                )}
                </tbody>
            </table>
        </div>
    );
}

export default ProcessList;
