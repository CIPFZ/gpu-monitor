interface UsageRingProps {
    value: number;
    label: string;
    type: 'gpu' | 'memory' | 'temp';
    unit?: string;
    subtitle?: string;
}

function UsageRing({ value, label, type, unit = '%', subtitle }: UsageRingProps) {
    // SVG circle parameters
    const size = 80;
    const strokeWidth = 8;
    const radius = (size - strokeWidth) / 2;
    const circumference = 2 * Math.PI * radius;
    const offset = circumference - (value / 100) * circumference;

    return (
        <div className="usage-ring">
            <div className="ring-container">
                <svg className="ring-svg" width={size} height={size}>
                    <circle
                        className="ring-background"
                        cx={size / 2}
                        cy={size / 2}
                        r={radius}
                    />
                    <circle
                        className={`ring-progress ${type}`}
                        cx={size / 2}
                        cy={size / 2}
                        r={radius}
                        strokeDasharray={circumference}
                        strokeDashoffset={offset}
                    />
                </svg>
                <span className="ring-value">
                    {value}
                    <span style={{ fontSize: '12px', opacity: 0.7 }}>{unit}</span>
                </span>
            </div>
            <span className="ring-label">{label}</span>
            {subtitle && (
                <span style={{ fontSize: '10px', color: 'var(--text-tertiary)' }}>
                    {subtitle}
                </span>
            )}
        </div>
    );
}

export default UsageRing;
