export type * from './wire';
export { SCHEMA_VERSION } from './wire';
import type { MemoryInfo, MetricIssue, GpuInfo } from './wire';

export function memoryPercent(memory: MemoryInfo | null): number | null {
    return memory && memory.total > 0 ? memory.used / memory.total * 100 : null;
}
export function processIssues(gpu: GpuInfo): MetricIssue[] {
    return gpu.issues.filter(issue => issue.metric.startsWith('processes'));
}
