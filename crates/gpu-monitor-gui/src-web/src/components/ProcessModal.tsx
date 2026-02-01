import { useState, useEffect } from 'react';
import { GpuInfo } from '../App';
import ProcessList from './ProcessList';

interface ProcessModalProps {
    gpu: GpuInfo;
    onClose: () => void;
}

function ProcessModal({ gpu, onClose }: ProcessModalProps) {
    const [searchTerm, setSearchTerm] = useState('');
    const [isVisible, setIsVisible] = useState(false);

    useEffect(() => {
        setIsVisible(true);
        
        // Close on Escape key
        const handleEsc = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onClose();
        };
        window.addEventListener('keydown', handleEsc);
        return () => window.removeEventListener('keydown', handleEsc);
    }, [onClose]);

    const filteredProcesses = gpu.processes.filter(p => 
        p.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
        p.pid.toString().includes(searchTerm)
    );

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div 
                className="modal-content" 
                onClick={e => e.stopPropagation()}
                style={{ opacity: isVisible ? 1 : 0 }}
            >
                <div className="modal-header">
                    <div>
                        <div className="modal-title">{gpu.device.name}</div>
                        <div style={{ fontSize: '12px', color: 'var(--text-secondary)', marginTop: '4px' }}>
                            GPU {gpu.device.index} • {gpu.device.pci_bus_id}
                        </div>
                    </div>
                    <button className="btn-close" onClick={onClose}>
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                            <line x1="18" y1="6" x2="6" y2="18"></line>
                            <line x1="6" y1="6" x2="18" y2="18"></line>
                        </svg>
                    </button>
                </div>

                <div className="modal-body">
                    <div className="modal-toolbar">
                        <div style={{ fontWeight: 600 }}>
                            Active Processes ({gpu.processes.length})
                        </div>
                        <input 
                            type="text" 
                            className="search-input"
                            placeholder="Search by name or PID..."
                            value={searchTerm}
                            onChange={(e) => setSearchTerm(e.target.value)}
                            autoFocus
                        />
                    </div>
                    
                    <ProcessList processes={filteredProcesses} />
                </div>
            </div>
        </div>
    );
}

export default ProcessModal;
