import { HISTORY_WINDOW_MS, HistoryPoint } from '../monitor/state';

interface SparklineProps {
    history: HistoryPoint[]; metric: 'load' | 'memory'; now: number; color: string;
    height?: number; windowMs?: number; intervalMs?: number;
}

// Keep each time bucket's extrema in their original order. Explicit gaps survive
// reduction, so a narrow chart still shows short peaks and collection outages.
export function chartSamples(history: HistoryPoint[], metric: 'load' | 'memory', now: number, windowMs: number, gapMs: number): HistoryPoint[] {
    const result: HistoryPoint[] = [];
    let bucket: HistoryPoint[] = [], bucketIndex = -1, previous: number | null = null;
    const flush = () => {
        if (!bucket.length) return;
        let min = bucket[0], max = bucket[0];
        for (const point of bucket) {
            if (point[metric]! < min[metric]!) min = point;
            if (point[metric]! > max[metric]!) max = point;
        }
        result.push(...[...new Set([bucket[0], min, max, bucket[bucket.length - 1]])].sort((a, b) => a.at - b.at));
        bucket = [];
    };
    for (const point of history) {
        if (point.at < now - windowMs || point.at > now) continue;
        const index = Math.floor((point.at - now + windowMs) / windowMs * 120);
        if (point[metric] === null || (previous !== null && point.at - previous > gapMs)) {
            flush();
            result.push({ at: point.at, load: null, memory: null });
        }
        if (point[metric] !== null) {
            if (index !== bucketIndex) flush();
            bucket.push(point);
            bucketIndex = index;
        }
        previous = point.at;
    }
    flush();
    return result;
}
export function chartPath(history: HistoryPoint[], metric: 'load' | 'memory', now: number, height: number, windowMs = HISTORY_WINDOW_MS, intervalMs = 1000): string {
    let connected = false;
    const segments: string[] = [];
    for (const point of chartSamples(history, metric, now, windowMs, intervalMs * 2.5)) {
        const value = point[metric];
        if (value === null) { connected = false; continue; }
        const x = (point.at - now + windowMs) / windowMs * 100;
        const y = height * (1 - Math.max(0, Math.min(100, value)) / 100);
        segments.push(`${connected ? 'L' : 'M'} ${x.toFixed(3)},${y.toFixed(3)}`);
        connected = true;
    }
    return segments.join(' ');
}
export default function Sparkline({ history, metric, now, color, height = 80, windowMs = HISTORY_WINDOW_MS, intervalMs = 1000 }: SparklineProps) {
    const visible = history.filter(point => point.at >= now - windowMs && point.at <= now && point[metric] !== null);
    const latest = visible[visible.length - 1];
    const seconds = windowMs / 1000;
    return <div className="sparkline-container">
        <svg role="img" aria-label={`${metric === 'load' ? 'GPU load' : 'Memory capacity usage'} over the last ${seconds} seconds`}
            width="100%" height={height} viewBox={`0 0 100 ${height}`} preserveAspectRatio="none">
            {[0, 25, 50, 75, 100].map(percent => <line key={percent} x1="0" x2="100"
                y1={height * percent / 100} y2={height * percent / 100}
                stroke="var(--border-light)" vectorEffect="non-scaling-stroke" />)}
            <path d={chartPath(history, metric, now, height, windowMs, intervalMs)} fill="none" stroke={color}
                strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
            {latest && <circle cx={(latest.at - now + windowMs) / windowMs * 100}
                cy={height * (1 - Math.max(0, Math.min(100, latest[metric]!)) / 100)} r="1" fill={color} />}
        </svg>
        <div className="chart-axis"><span>−{seconds >= 3600 ? `${seconds / 3600}h` : seconds >= 300 ? `${seconds / 60}m` : `${seconds}s`}</span><span>now</span></div>
    </div>;
}
