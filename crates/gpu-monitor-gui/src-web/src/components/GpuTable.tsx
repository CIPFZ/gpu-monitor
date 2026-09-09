import { DeviceState } from '../monitor/state';

export default function GpuTable({ entries, stale, onDetails }: {
    entries: DeviceState[]; stale: (entry: DeviceState) => boolean; onDetails: (uuid: string) => void;
}) {
    const value = (n: number | null | undefined, suffix: string) => n == null ? 'N/A' : `${n}${suffix}`;
    return <div className="table-container"><table className="process-table gpu-comparison" aria-label="GPU comparison">
        <thead><tr><th>GPU</th><th>Model</th><th>Free / Total</th><th>Load</th><th>Temperature</th><th>Power</th><th>Status</th></tr></thead>
        <tbody>{entries.map(entry => { const { device, metrics, memory } = entry.gpu; return <tr key={device.uuid}>
            <td><button className="btn-retry" onClick={() => onDetails(device.uuid)} title={device.uuid}>GPU {device.index}</button></td>
            <td>{device.name}</td><td>{memory ? `${(memory.free / 1024 ** 3).toFixed(1)} / ${(memory.total / 1024 ** 3).toFixed(1)} GiB` : 'N/A'}</td>
            <td>{value(metrics.gpu_utilization, '%')}</td><td>{value(metrics.temperature, '°C')}</td>
            <td>{value(metrics.power_usage == null ? null : Math.round(metrics.power_usage / 1000), ' W')}</td><td>{stale(entry) ? 'Stale' : 'Live'}</td>
        </tr>; })}</tbody>
    </table>{!entries.length && <p className="empty-message">No GPUs match these filters.</p>}</div>;
}
