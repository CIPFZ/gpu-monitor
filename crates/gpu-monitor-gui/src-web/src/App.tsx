import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import GpuCard from './components/GpuCard';

// Types matching Rust structures
interface MemoryInfo {
    total: number;
    used: number;
    free: number;
}

interface GpuMetrics {
    gpu_utilization: number;
    memory_utilization: number;
    encoder_utilization: number;
    decoder_utilization: number;
    temperature: number;
    power_usage: number;
    fan_speed: number | null;
    clock_graphics: number;
    clock_memory: number;
    clock_sm: number;
}

interface DeviceInfo {
    index: number;
    name: string;
    uuid: string;
    pci_bus_id: string;
    driver_version: string;
    cuda_version: string | null;
    power_limit: number;
    power_limit_max: number;
}

interface GpuProcess {
    pid: number;
    name: string;
    gpu_memory: number;
    process_type: 'Graphics' | 'Compute' | 'Mixed' | 'Unknown';
}

export interface GpuInfo {
    device: DeviceInfo;
    metrics: GpuMetrics;
    memory: MemoryInfo;
    processes: GpuProcess[];
}

function App() {
    const [gpus, setGpus] = useState<GpuInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [searchTerm, setSearchTerm] = useState('');

    useEffect(() => {
        const fetchGpuInfo = async () => {
            try {
                const data = await invoke<GpuInfo[]>('get_gpu_info');
                setGpus(data);
                setError(null);
            } catch (err: any) {
                setError(err.message || 'Failed to get GPU info');
            } finally {
                setLoading(false);
            }
        };

        // Initial fetch
        fetchGpuInfo();

        // Refresh every second
        const interval = setInterval(fetchGpuInfo, 1000);

        return () => clearInterval(interval);
    }, []);

    // Filter GPUs based on search term
    const filteredGpus = gpus.filter(gpu => 
        gpu.device.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
        gpu.device.index.toString().includes(searchTerm) ||
        gpu.device.uuid.toLowerCase().includes(searchTerm.toLowerCase())
    );

    if (loading) {
        return (
            <div className="app-container" style={{ justifyContent: 'center', alignItems: 'center' }}>
                <div className="status-badge" style={{ background: 'transparent', border: 'none', color: 'var(--text-secondary)' }}>
                    Loading GPU info...
                </div>
            </div>
        );
    }

    if (error) {
        return (
            <div className="app-container" style={{ justifyContent: 'center', alignItems: 'center' }}>
                <div style={{ textAlign: 'center' }}>
                    <div style={{ fontSize: '48px', marginBottom: '16px' }}>⚠️</div>
                    <p style={{ color: 'var(--text-secondary)' }}>{error}</p>
                </div>
            </div>
        );
    }

    // Determine view mode
    const isSingleGpu = gpus.length === 1;

    return (
        <div className="app-container">
            <header className="app-header">
                <div className="app-icon">G</div>
                <div className="header-content">
                    <div className="app-title">GPU Monitor</div>
                    <div className="app-subtitle">Real-time Performance</div>
                </div>
                
                <div className="header-controls">
                    {!isSingleGpu && gpus.length > 1 && (
                        <div className="gpu-search">
                            <svg className="search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                <circle cx="11" cy="11" r="8"></circle>
                                <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
                            </svg>
                            <input 
                                type="text" 
                                placeholder={`Filter ${gpus.length} GPUs...`}
                                value={searchTerm}
                                onChange={(e) => setSearchTerm(e.target.value)}
                            />
                        </div>
                    )}
                    <div className="status-badge">
                        <span className="status-dot" />
                        <span>Live</span>
                    </div>
                </div>
            </header>

            {isSingleGpu ? (
                // Single GPU View (Expanded)
                <div className="gpu-expanded-container">
                    <GpuCard gpu={gpus[0]} mode="expanded" />
                </div>
            ) : (
                // Multi GPU View (Grid)
                <div className="gpu-grid">
                    {filteredGpus.length > 0 ? (
                        filteredGpus.map((gpu) => (
                            <GpuCard key={gpu.device.uuid} gpu={gpu} mode="compact" />
                        ))
                    ) : (
                        <div style={{ 
                            gridColumn: '1 / -1', 
                            textAlign: 'center', 
                            color: 'var(--text-secondary)',
                            padding: '40px'
                        }}>
                            No GPUs found matching "{searchTerm}"
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}

export default App;
