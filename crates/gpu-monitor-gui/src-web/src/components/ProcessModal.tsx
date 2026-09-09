import { useEffect, useRef } from 'react';
import { GpuInfo } from '../monitor/models';
import ProcessPanel from './ProcessPanel';

interface ProcessModalProps { gpu: GpuInfo; stale: boolean; onClose: () => void }
export default function ProcessModal({ gpu, stale, onClose }: ProcessModalProps) {
    const dialog = useRef<HTMLDialogElement>(null);
    useEffect(() => {
        const element = dialog.current!;
        const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        element.showModal();
        element.querySelector('button')?.focus();
        return () => {
            element.close();
            // React removes the dialog before effect cleanup; native focus restoration
            // alone cannot reliably return to its still-mounted trigger.
            if (trigger?.isConnected) trigger.focus();
        };
    }, []);
    return <dialog ref={dialog} className="process-dialog" aria-labelledby="process-dialog-title"
        onCancel={event => { event.preventDefault(); onClose(); }}>
        <div className="modal-header">
            <h2 id="process-dialog-title" className="modal-title">GPU {gpu.device.index} · {gpu.device.name}</h2>
            <button className="btn-close" aria-label="Close processes" onClick={onClose}>×</button>
        </div>
        {stale && <p className="data-notice">Stale · Last successful sample {new Date(gpu.sampled_at_ms).toLocaleTimeString()}</p>}
        <div className="modal-body"><ProcessPanel gpu={gpu} /></div>
    </dialog>;
}
