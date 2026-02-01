import { useState, useEffect } from 'react';
import { GpuInfo } from '../App';
import Sparkline from './Sparkline';
import ProcessModal from './ProcessModal';
import ProcessList from './ProcessList';

interface GpuCardProps {
    gpu: GpuInfo;
    mode?: 'compact' | 'expanded';
}

function GpuCard({ gpu, mode = 'compact' }: GpuCardProps) {
    const { device, metrics, memory, processes } = gpu;
    const [showDetails, setShowDetails] = useState(false);
    const [searchTerm, setSearchTerm] = useState('');
    
    // History state for charts
    const [loadHistory, setLoadHistory] = useState<number[]>([]);
    const [memHistory, setMemHistory] = useState<number[]>([]);

    // Update history
    useEffect(() => {
        setLoadHistory(prev => {
            const next = [...prev, metrics.gpu_utilization];
            return next.slice(-60); // Keep last 60 samples
        });
        
        // Store memory usage as percentage for the chart
        const memPercent = (memory.used / memory.total) * 100;
        setMemHistory(prev => {
            const next = [...prev, memPercent];
            return next.slice(-60);
        });
    }, [metrics.gpu_utilization, memory.used, memory.total]);

    // Calculate display values
    const memoryUsedGiB = (memory.used / (1024 * 1024 * 1024)).toFixed(1);
    const memoryTotalGiB = (memory.total / (1024 * 1024 * 1024)).toFixed(1);
    const powerWatts = (metrics.power_usage / 1000).toFixed(0);
    
    const getTempColor = (temp: number) => {
        if (temp > 85) return 'var(--accent-red)';
        if (temp > 70) return 'var(--accent-orange)';
        return 'var(--accent-green)';
    };

    // Filter processes for expanded mode
    const filteredProcesses = processes.filter(p => 
        p.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
        p.pid.toString().includes(searchTerm)
    );

    // --- Expanded Mode (Single GPU) ---
    if (mode === 'expanded') {
        return (
            <div className="gpu-expanded">
                <div className="expanded-header">
                    <div className="gpu-name-large">{device.name}</div>
                    <div className="gpu-meta-large">
                        <span className="meta-tag">GPU {device.index}</span>
                        <span className="meta-tag">Driver {device.driver_version}</span>
                        <span className="meta-tag">PCI {device.pci_bus_id}</span>
                        <span className="meta-tag">CUDA {device.cuda_version || 'N/A'}</span>
                        <span className="meta-tag">Power Limit {device.power_limit}W</span>
                    </div>
                </div>

                <div className="expanded-metrics-grid">
                    {/* GPU Load Section */}
                    <div className="expanded-metric-card">
                        <div className="metric-header">
                            <span className="metric-label-large">GPU Load</span>
                            <span className="metric-value-xl">{metrics.gpu_utilization}%</span>
                        </div>
                        <div className="chart-container-large">
                            <Sparkline 
                                data={loadHistory} 
                                color="var(--accent-blue)" 
                                height={120}
                                max={100}
                            />
                        </div>
                    </div>

                    {/* Memory Section */}
                    <div className="expanded-metric-card">
                        <div className="metric-header">
                            <span className="metric-label-large">Memory</span>
                            <div>
                                <span className="metric-value-xl">{memoryUsedGiB}</span>
                                <span className="metric-unit-large">/ {memoryTotalGiB} GiB</span>
                            </div>
                        </div>
                        <div className="chart-container-large">
                            <Sparkline 
                                data={memHistory} 
                                color="var(--accent-purple)" 
                                height={120}
                                max={100}
                            />
                        </div>
                    </div>
                </div>

                <div className="secondary-metrics-row">
                    <div className="stat-box">
                        <div className="stat-label">Temperature</div>
                        <div className="stat-value" style={{ color: getTempColor(metrics.temperature) }}>
                            {metrics.temperature}°C
                        </div>
                    </div>
                    <div className="stat-box">
                        <div className="stat-label">Power Usage</div>
                        <div className="stat-value">{powerWatts}W</div>
                    </div>
                    <div className="stat-box">
                        <div className="stat-label">Fan Speed</div>
                        <div className="stat-value">
                            {metrics.fan_speed !== null ? `${metrics.fan_speed}%` : 'N/A'}
                        </div>
                    </div>
                    <div className="stat-box">
                        <div className="stat-label">Clock (Graphics)</div>
                        <div className="stat-value">{metrics.clock_graphics} MHz</div>
                    </div>
                    <div className="stat-box">
                        <div className="stat-label">Clock (Memory)</div>
                        <div className="stat-value">{metrics.clock_memory} MHz</div>
                    </div>
                </div>

                <div className="expanded-process-section">
                    <div className="process-toolbar">
                        <div className="section-title-large">Active Processes ({processes.length})</div>
                        <input 
                            type="text" 
                            className="search-input"
                            placeholder="Search processes..."
                            value={searchTerm}
                            onChange={(e) => setSearchTerm(e.target.value)}
                        />
                    </div>
                    <div className="expanded-table-wrapper">
                        <ProcessList processes={filteredProcesses} />
                    </div>
                </div>
            </div>
        );
    }

    // --- Compact Mode (Multi GPU) ---
    return (
        <>
            <div className="gpu-card">
                <div className="gpu-header">
                    <div>
                        <div className="gpu-name" title={device.name}>{device.name}</div>
                        <div className="gpu-meta">
                            <span className="gpu-index">GPU {device.index}</span>
                            <span>{device.pci_bus_id}</span>
                        </div>
                    </div>
                </div>

                <div className="compact-metrics">
                    {/* GPU Load Row */}
                    <div className="metric-row">
                        <div className="metric-info">
                            <div className="metric-label">GPU Load</div>
                            <div>
                                <span className="metric-value-large">{metrics.gpu_utilization}</span>
                                <span className="metric-unit-small">%</span>
                            </div>
                        </div>
                        <div className="metric-chart">
                            <Sparkline 
                                data={loadHistory} 
                                color="var(--accent-blue)" 
                                height={40}
                                max={100}
                            />
                        </div>
                    </div>

                    {/* Memory Row */}
                    <div className="metric-row">
                        <div className="metric-info">
                            <div className="metric-label">Memory</div>
                            <div>
                                <span className="metric-value-large">{memoryUsedGiB}</span>
                                <span className="metric-unit-small">GiB</span>
                            </div>
                        </div>
                        <div className="metric-chart">
                            <Sparkline 
                                data={memHistory} 
                                color="var(--accent-purple)" 
                                height={40}
                                max={100}
                            />
                        </div>
                    </div>
                </div>

                <div className="secondary-metrics">
                    <div className="mini-metric">
                        <span className="mini-label">Temp</span>
                        <span className="mini-value" style={{ color: getTempColor(metrics.temperature) }}>
                            {metrics.temperature}°C
                        </span>
                    </div>
                    <div className="mini-metric">
                        <span className="mini-label">Power</span>
                        <span className="mini-value">{powerWatts}W</span>
                    </div>
                    <div className="mini-metric">
                        <span className="mini-label">Fan</span>
                        <span className="mini-value">
                            {metrics.fan_speed !== null ? `${metrics.fan_speed}%` : '-'}
                        </span>
                    </div>
                </div>

                <div className="card-action">
                    <button 
                        className="btn-details"
                        onClick={() => setShowDetails(true)}
                    >
                        View Processes ({processes.length})
                    </button>
                </div>
            </div>

            {showDetails && (
                <ProcessModal 
                    gpu={gpu} 
                    onClose={() => setShowDetails(false)} 
                />
            )}
        </>
    );
}

export default GpuCard;
