import { useState } from 'react';
import { memoryPercent, processIssues } from '../monitor/models';
import { DeviceState } from '../monitor/state';
import Sparkline from './Sparkline';
import ProcessModal from './ProcessModal';
import ProcessPanel from './ProcessPanel';

interface GpuCardProps {
    entry: DeviceState;
    now: number;
    stale: boolean;
    mode?: 'compact' | 'expanded';
    onDetails?: () => void;
    windowMs?: number;
    intervalMs?: number;
}
function measurement(value: number | null, unit: string, divisor = 1): string {
    return value === null ? 'N/A' : `${(value / divisor).toFixed(0)}${unit}`;
}
export default function GpuCard({ entry, now, stale, mode = 'compact', onDetails, windowMs, intervalMs }: GpuCardProps) {
    const { gpu, history, failure } = entry;
    const { device, metrics, memory } = gpu;
    const expanded = mode === 'expanded';
    const [showProcesses, setShowProcesses] = useState(false);
    const percent = memoryPercent(memory);
    const capacity = memory ? `${(memory.used / 1024 ** 3).toFixed(1)} / ${(memory.total / 1024 ** 3).toFixed(1)} GiB` : 'N/A';
    const secondary = [
        ['Temperature', measurement(metrics.temperature, '°C')],
        ['Power Usage', measurement(metrics.power_usage, ' W', 1000)],
        ['Fan Speed', measurement(metrics.fan_speed, '%')],
        ...(expanded ? [
            ['Clock (Graphics)', measurement(metrics.clock_graphics, ' MHz')],
            ['Clock (Memory)', measurement(metrics.clock_memory, ' MHz')],
            ['Clock (SM)', measurement(metrics.clock_sm, ' MHz')],
            ['Memory I/O Busy', measurement(metrics.memory_utilization, '%')],
            ['Encoder Busy', measurement(metrics.encoder_utilization, '%')],
            ['Decoder Busy', measurement(metrics.decoder_utilization, '%')],
            ['Performance State', metrics.performance_state ?? 'N/A'],
            ['Clock Limit Reasons', metrics.throttle_reasons?.join(', ') || (metrics.throttle_reasons ? 'None reported' : 'N/A')],
            ['PCIe Generation', measurement(metrics.pcie_generation, '')],
            ['PCIe Width', measurement(metrics.pcie_width, ' lanes')],
            ['PCIe Receive', measurement(metrics.pcie_rx_kb_per_second, ' KB/s')],
            ['PCIe Transmit', measurement(metrics.pcie_tx_kb_per_second, ' KB/s')],
        ] : []),
    ];
    return <article className={expanded ? 'gpu-expanded' : 'gpu-card'} aria-label={`GPU ${device.index}: ${device.name}`}>
        <div className={expanded ? 'expanded-header' : 'gpu-header'}>
            <div>
                <h2 className={expanded ? 'gpu-name-large' : 'gpu-name'} title={device.name}>{device.name}</h2>
                <div className={expanded ? 'gpu-meta-large' : 'gpu-meta'}>
                    <span className="meta-tag">GPU {device.index}</span><span className="meta-tag">PCI {device.pci_bus_id}</span>
                    {expanded && <>
                        <span className="meta-tag">Driver {device.driver_version}</span>
                        <span className="meta-tag">CUDA {device.cuda_version ?? 'N/A'}</span>
                        <span className="meta-tag">Power Limit {measurement(device.power_limit, ' W')}</span>
                        <span className="meta-tag">Maximum Power Limit {measurement(device.power_limit_max, ' W')}</span>
                        <span className="meta-tag">{device.uuid}</span>
                    </>}
                </div>
            </div>
        </div>
        <p className={`sample-status${stale ? ' is-stale' : ''}`}>
            {stale ? 'Stale · Last successful sample ' : 'Updated '}
            <time dateTime={new Date(gpu.sampled_at_ms).toISOString()}>{new Date(gpu.sampled_at_ms).toLocaleTimeString()}</time>
            {failure && ` · ${failure.message}`}
        </p>
        {gpu.issues.length > 0 && <details className="data-notice"><summary>{gpu.issues.length} measurement issue{gpu.issues.length === 1 ? '' : 's'} · N/A means unavailable</summary>
            <ul>{gpu.issues.map((issue, index) => <li key={`${issue.metric}-${index}`}>{issue.metric}: {issue.error.message}</li>)}</ul>
        </details>}
        <div className={expanded ? 'expanded-metrics-grid' : 'compact-metrics'}>
            {(['load', 'memory'] as const).map(metric => <div key={metric} className={expanded ? 'expanded-metric-card' : 'metric-row'}>
                <div className={expanded ? 'metric-header' : 'metric-info'}>
                    <span className="metric-label">{metric === 'load' ? 'GPU Load' : 'Memory Capacity'}</span>
                    <div className={expanded ? 'metric-value-xl' : 'metric-value-large'}>
                        {metric === 'load' ? measurement(metrics.gpu_utilization, '%') : capacity}
                    </div>
                    {metric === 'memory' && <span className="metric-unit-small">{measurement(percent, '%')} used</span>}
                </div>
                <div className={expanded ? 'chart-container-large' : 'metric-chart'}>
                    <Sparkline history={history} metric={metric} now={now} windowMs={windowMs} intervalMs={intervalMs}
                        color={metric === 'load' ? 'var(--accent-blue)' : 'var(--accent-purple)'} height={expanded ? 120 : 40} />
                </div>
            </div>)}
        </div>
        <div className={expanded ? 'secondary-metrics-row' : 'secondary-metrics'}>{secondary.map(([label, value]) =>
            <div className={expanded ? 'stat-box' : 'mini-metric'} key={label}>
                <span className={expanded ? 'stat-label' : 'mini-label'}>{label}</span>
                <span className={expanded ? 'stat-value' : 'mini-value'}>{value}</span>
            </div>)}</div>
        {expanded ? <ProcessPanel gpu={gpu} /> : <div className="card-action">
            <button className="btn-details" onClick={onDetails}>View Details</button>
            <button className="btn-details" onClick={() => setShowProcesses(true)}>View Processes ({gpu.processes.length}{processIssues(gpu).length ? ', incomplete' : ''})</button>
        </div>}
        {showProcesses && <ProcessModal gpu={gpu} stale={stale} onClose={() => setShowProcesses(false)} />}
    </article>;
}
