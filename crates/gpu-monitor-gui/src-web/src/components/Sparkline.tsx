import { HISTORY_WINDOW_MS, HistoryPoint } from '../monitor/state';

interface SparklineProps {
    history: HistoryPoint[];
    metric: 'load' | 'memory';
    now: number;
    color: string;
    height?: number;
}

export function chartPath(history: HistoryPoint[], metric: 'load' | 'memory', now: number, height: number): string {
    let previous: number | null = null;
    const segments: string[] = [];
    for (const point of history) {
        if (point.at < now - HISTORY_WINDOW_MS || point.at > now) continue;
        const value = point[metric];
        if (value === null) { previous = null; continue; }
        const x = (point.at - (now - HISTORY_WINDOW_MS)) / HISTORY_WINDOW_MS * 100;
        const y = height * (1 - Math.max(0, Math.min(100, value)) / 100);
        const command = previous === null || point.at - previous > 2500 ? 'M' : 'L';
        segments.push(`${command} ${x.toFixed(3)},${y.toFixed(3)}`);
        previous = point.at;
    }
    return segments.join(' ');
}

export default function Sparkline({ history, metric, now, color, height = 80 }: SparklineProps) {
    const visible = history.filter(point => point.at >= now - HISTORY_WINDOW_MS && point.at <= now && point[metric] !== null);
    const latest = visible[visible.length - 1];
    return <div className="sparkline-container">
        <svg role="img" aria-label={`${metric === 'load' ? 'GPU load' : 'Memory capacity usage'} over the last 60 seconds`}
            width="100%" height={height} viewBox={`0 0 100 ${height}`} preserveAspectRatio="none">
            {[0, 25, 50, 75, 100].map(percent => <line key={percent} x1="0" x2="100"
                y1={height * percent / 100} y2={height * percent / 100}
                stroke="var(--border-light)" vectorEffect="non-scaling-stroke" />)}
            <path d={chartPath(history, metric, now, height)} fill="none" stroke={color}
                strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
            {latest && <circle cx={(latest.at - now + HISTORY_WINDOW_MS) / HISTORY_WINDOW_MS * 100}
                cy={height * (1 - Math.max(0, Math.min(100, latest[metric]!)) / 100)} r="1" fill={color} />}
        </svg>
        <div className="chart-axis"><span>−60s</span><span>now</span></div>
    </div>;
}
