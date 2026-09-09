import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { gpu, snapshot, TIME } from './test/fixtures';
import type { MonitorTools } from './monitor/useMonitorTools';
import type { MonitorSnapshot } from './monitor/models';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
let current: MonitorSnapshot;
let tools: MonitorTools;
const flush = () => act(async () => {});
const tick = (ms = 2000) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
beforeEach(() => {
    vi.useFakeTimers(); vi.setSystemTime(TIME); vi.mocked(invoke).mockReset();
    current = snapshot(TIME, [gpu(0), gpu(1)]);
    tools = { history: { interval_ms: 1000, window_ms: 60_000, frames: [] }, events: [],
        alert_config: { enabled: true, temperature_threshold: 85, temperature_recovery: 80, memory_threshold: 95, memory_recovery: 90, duration_ms: 10000, cooldown_ms: 60000 },
        recording: { active: false, finishing: false, path: null, samples_written: 0, dropped_samples: 0, bytes_written: 0, error: null } };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
        if (command === 'get_gpu_info') return current;
        if (command === 'get_monitor_tools') return tools;
        if (command === 'start_recording') { tools = { ...tools, recording: { ...tools.recording, active: true, path: String((args as Record<string, unknown> | undefined)?.path) } }; return tools.recording; }
        if (command === 'stop_recording') { tools = { ...tools, recording: { ...tools.recording, active: false, finishing: true } }; return tools.recording; }
        if (command === 'load_recording') return [snapshot(TIME - 20000), snapshot(TIME - 19000), snapshot(TIME - 18000)];
        return undefined;
    });
});
afterEach(() => vi.useRealTimers());
it('supports capacity filtering, table details and user-filtered cross-GPU processes', async () => {
    const process = { pid: 42, name: 'python', user: 'alice', uid: 1000, command: ['python', 'train.py'], elapsed_seconds: 5, started_at_ms: TIME - 5000, gpu_memory: 1024, process_type: 'Compute' as const };
    current.gpus.forEach(card => { card.processes = [{ ...process }]; });
    render(<App />); await flush();
    fireEvent.change(screen.getByLabelText('View'), { target: { value: 'table' } });
    fireEvent.change(screen.getByLabelText('Minimum free GiB'), { target: { value: '20' } });
    const table = screen.getByRole('table', { name: 'GPU comparison' });
    expect(within(table).queryByRole('button', { name: 'GPU 0' })).not.toBeInTheDocument();
    fireEvent.click(within(table).getByRole('button', { name: 'GPU 1' }));
    expect(screen.getByLabelText('GPU details')).toHaveFocus();
    expect(screen.getByText('Performance State')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '← All GPUs' }));
    fireEvent.change(screen.getByLabelText('Minimum free GiB'), { target: { value: '' } });
    fireEvent.change(screen.getByLabelText('View'), { target: { value: 'processes' } });
    expect(screen.getByText('42 · python')).toBeInTheDocument();
    expect(screen.getByText('GPU 0: 0 MiB')).toBeInTheDocument();
    expect(screen.getByText('GPU 1: 0 MiB')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Filter all processes by user'), { target: { value: 'bob' } });
    expect(screen.queryByText('42 · python')).not.toBeInTheDocument();
});
it('records with arguments excluded and replays recorded timestamps independently of live updates', async () => {
    render(<App />); await flush();
    fireEvent.click(screen.getByText('Recording, replay and export'));
    fireEvent.change(screen.getByLabelText('Recording file path'), { target: { value: '/tmp/session.jsonl' } });
    fireEvent.click(screen.getByRole('button', { name: 'Start recording' })); await flush();
    expect(invoke).toHaveBeenCalledWith('start_recording', { path: '/tmp/session.jsonl', includeCommands: false });
    fireEvent.click(screen.getByRole('button', { name: 'Stop recording' })); await flush();
    expect(screen.getByRole('button', { name: 'Open replay' })).toBeDisabled();
    tools = { ...tools, recording: { ...tools.recording, finishing: false } }; await tick();
    fireEvent.click(screen.getByRole('button', { name: 'Open replay' })); await flush();
    expect(screen.getByRole('status')).toHaveTextContent('Replay');
    fireEvent.click(screen.getByRole('button', { name: 'Play' })); await tick(1000);
    expect(screen.getByLabelText('Replay frame')).toHaveValue('1');
    fireEvent.change(screen.getByLabelText('Replay frame'), { target: { value: '0' } });
    expect(screen.getByRole('button', { name: 'Play' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Return to live' }));
    expect(screen.queryByLabelText('Replay controls')).not.toBeInTheDocument();
});
it('applies rules and notifies new events once after opt-in', async () => {
    tools.events = [{ id: 1, at_ms: TIME, gpu_uuid: 'GPU-0', kind: 'temperature', state: 'firing', message: 'Temperature high', value: 90 }];
    render(<App />); await flush();
    fireEvent.click(screen.getByText('Alerts and events (1)'));
    fireEvent.change(screen.getByLabelText('Temperature threshold (°C)'), { target: { value: '88' } });
    fireEvent.click(screen.getByRole('button', { name: 'Apply alert rules' })); await flush();
    expect(invoke).toHaveBeenCalledWith('configure_alerts', { config: { ...tools.alert_config, temperature_threshold: 88 } });
    fireEvent.click(screen.getByLabelText('Desktop notifications for new events while this window is open')); await flush();
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'notify_event')).toHaveLength(0);
    tools = { ...tools, events: [...tools.events, { ...tools.events[0], id: 2, state: 'recovered', message: 'Temperature recovered' }] };
    await tick(); await tick();
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'notify_event')).toEqual([['notify_event', { eventId: 2 }]]);
});
