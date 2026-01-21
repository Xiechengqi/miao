"use client";

import { Card } from "@/components/ui/Card/Card";
import { SystemMetricsPoint } from "@/types/api";

interface PerformanceTrendChartProps {
  series: SystemMetricsPoint[];
  memoryTotalBytes?: number;
}

function buildPolyline(values: number[], width: number, height: number): string {
  if (values.length === 0) {
    return "";
  }
  return values
    .map((value, index) => {
      const x = (index / Math.max(values.length - 1, 1)) * width;
      const y = height - (value / 100) * height;
      return `${x},${y}`;
    })
    .join(" ");
}

export function PerformanceTrendChart({
  series,
  memoryTotalBytes,
}: PerformanceTrendChartProps) {
  const width = 600;
  const height = 160;
  const cpuValues = series.map((point) => point.cpuPercent);
  const hasGpu = series.some((point) => point.gpuPercent !== undefined);
  const gpuValues = series.map((point) => point.gpuPercent ?? 0);
  const memoryValues = series.map((point) => {
    if (!memoryTotalBytes) {
      return 0;
    }
    return Math.min(
      100,
      Math.round((point.memoryUsedKb * 1024 * 100) / memoryTotalBytes)
    );
  });
  const diskValues = series.map((point) => {
    if (point.diskTotalBytes === 0) {
      return 0;
    }
    return Math.min(
      100,
      Math.round((point.diskUsedBytes * 100) / point.diskTotalBytes)
    );
  });

  const hasData = series.length > 1;

  return (
    <Card className="p-5">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-slate-900">性能趋势</h3>
        <div className="flex gap-3 text-xs text-slate-500">
          <span className="flex items-center gap-1">
            <span className="h-2 w-2 rounded-full bg-indigo-500" />
            CPU
          </span>
          <span className="flex items-center gap-1">
            <span className="h-2 w-2 rounded-full bg-emerald-500" />
            内存
          </span>
          {hasGpu && (
            <span className="flex items-center gap-1">
              <span className="h-2 w-2 rounded-full bg-purple-500" />
              GPU
            </span>
          )}
          <span className="flex items-center gap-1">
            <span className="h-2 w-2 rounded-full bg-amber-500" />
            磁盘
          </span>
        </div>
      </div>
      <div className="mt-4">
        {!hasData && (
          <div className="rounded-lg border border-dashed border-slate-200 py-8 text-center text-sm text-slate-500">
            暂无趋势数据
          </div>
        )}
        {hasData && (
          <svg
            viewBox={`0 0 ${width} ${height}`}
            className="h-40 w-full"
            preserveAspectRatio="none"
          >
            <polyline
              fill="none"
              stroke="#6366F1"
              strokeWidth="2"
              points={buildPolyline(cpuValues, width, height)}
            />
            <polyline
              fill="none"
              stroke="#10B981"
              strokeWidth="2"
              points={buildPolyline(memoryValues, width, height)}
            />
            <polyline
              fill="none"
              stroke="#A855F7"
              strokeWidth="2"
              points={buildPolyline(gpuValues, width, height)}
              style={{ display: hasGpu ? "block" : "none" }}
            />
            <polyline
              fill="none"
              stroke="#F59E0B"
              strokeWidth="2"
              points={buildPolyline(diskValues, width, height)}
            />
          </svg>
        )}
      </div>
    </Card>
  );
}
