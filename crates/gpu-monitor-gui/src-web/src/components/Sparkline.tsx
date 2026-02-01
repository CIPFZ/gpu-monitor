import { useMemo } from 'react';

interface SparklineProps {
    data: number[];
    color: string;
    height?: number;
    max?: number;
    label?: string;
    value?: string | number;
    unit?: string;
}

function Sparkline({ data, color, height = 80, max = 100, label, value, unit }: SparklineProps) {
    // Generate path for the line
    const pathData = useMemo(() => {
        if (data.length < 2) return '';

        const points = data.map((val, i) => {
            const x = (i / (data.length - 1)) * 100;
            // Invert Y axis because SVG origin is top-left
            // Clamp value between 0 and max
            const clampedVal = Math.max(0, Math.min(val, max));
            const y = height - (clampedVal / max) * height;
            return `${x},${y}`;
        });

        return `M ${points.join(' L ')}`;
    }, [data, height, max]);

    // Grid lines generation
    const horizontalGridLines = [0, 25, 50, 75, 100].map(percent => {
        const y = height - (percent / 100) * height;
        return (
            <line 
                key={`h-${percent}`} 
                x1="0" 
                y1={y} 
                x2="100" 
                y2={y} 
                stroke="var(--border-light)" 
                strokeWidth="1" 
                vectorEffect="non-scaling-stroke"
                strokeDasharray="2 2"
            />
        );
    });

    const verticalGridLines = [0, 20, 40, 60, 80, 100].map(percent => {
        return (
            <line 
                key={`v-${percent}`} 
                x1={percent} 
                y1="0" 
                x2={percent} 
                y2={height} 
                stroke="var(--border-light)" 
                strokeWidth="1" 
                vectorEffect="non-scaling-stroke"
                strokeDasharray="2 2"
            />
        );
    });

    return (
        <div className="sparkline-container" style={{ height: height + 30 }}>
            {(label || value) && (
                <div className="sparkline-header">
                    {label && <span className="sparkline-label">{label}</span>}
                    {value && (
                        <span className="sparkline-value">
                            {value}
                            {unit && <span className="sparkline-unit">{unit}</span>}
                        </span>
                    )}
                </div>
            )}
            <div className="sparkline-graph" style={{ height, position: 'relative' }}>
                <svg 
                    width="100%" 
                    height="100%" 
                    viewBox={`0 0 100 ${height}`} 
                    preserveAspectRatio="none"
                    style={{ overflow: 'visible' }}
                >
                    {/* Grid */}
                    <g className="grid-lines">
                        {horizontalGridLines}
                        {verticalGridLines}
                    </g>
                    
                    {/* Line */}
                    <path 
                        d={pathData} 
                        fill="none" 
                        stroke={color} 
                        strokeWidth="2" 
                        vectorEffect="non-scaling-stroke"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                    />
                </svg>
                
                {/* Axis Labels (Optional, for better readability) */}
                <div style={{ 
                    position: 'absolute', 
                    right: 0, 
                    top: 0, 
                    fontSize: '9px', 
                    color: 'var(--text-tertiary)',
                    lineHeight: 1 
                }}>
                    {max}
                </div>
                <div style={{ 
                    position: 'absolute', 
                    right: 0, 
                    bottom: 0, 
                    fontSize: '9px', 
                    color: 'var(--text-tertiary)',
                    lineHeight: 1 
                }}>
                    0
                </div>
            </div>
        </div>
    );
}

export default Sparkline;
