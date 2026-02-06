"use client";

import { useRef, useState } from "react";
import type { MouseEvent } from "react";
import { Card } from "@/components/ui/Card/Card";
import { SystemMetricsPoint } from "@/types/api";

interface PerformanceTrendChartProps {
  series: SystemMetricsPoint[];
  memoryTotalBytes?: number;
  range: string;
  rangeOptions: string[];
  onRangeChange: (value: string) => void;
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
  range,
  rangeOptions,
  onRangeChange,
}: PerformanceTrendChartProps) {
  const width = 600;
  const height = 160;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  const seriesLength = series.length;
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

  const hasData = seriesLength > 1;
  const sampling = seriesLength > 0 && !hasData;
  const hoverPoint = hoverIndex !== null ? series[hoverIndex] : undefined;
  const hoverX =
    hoverIndex !== null && seriesLength > 1
      ? (hoverIndex / (seriesLength - 1)) * width
      : 0;
  const hoverLeftPercent =
    hoverIndex !== null && seriesLength > 1
      ? Math.min(92, Math.max(8, (hoverIndex / (seriesLength - 1)) * 100))
      : 0;

  const handlePointerMove = (event: MouseEvent<HTMLDivElement>) => {
    if (!containerRef.current || seriesLength < 2) {
      return;
    }
    const rect = containerRef.current.getBoundingClientRect();
    const x = Math.min(Math.max(event.clientX - rect.left, 0), rect.width);
    const index = Math.round((x / rect.width) * (seriesLength - 1));
    setHoverIndex(index);
  };

  const handlePointerLeave = () => {
    setHoverIndex(null);
  };

  const formatPercentValue = (value?: number) => {
    if (value === undefined || Number.isNaN(value)) {
      return "-";
    }
    return `${Math.round(value)}%`;
  };

  const formatDate = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    const mm = String(date.getMonth() + 1).padStart(2, "0");
    const dd = String(date.getDate()).padStart(2, "0");
    return `${mm}/${dd}`;
  };

  const xAxisTicks = (() => {
    if (!hasData) return [];
    const ticks: Array<{ index: number; label: string }> = [];
    const seen = new Set<string>();
    series.forEach((point, index) => {
      const label = formatDate(point.timestamp);
      if (seen.has(label)) return;
      seen.add(label);
      ticks.push({ index, label });
    });
    return ticks;
  })();

  const hoverMemoryPercent =
    hoverPoint && memoryTotalBytes
      ? Math.min(
          100,
          Math.round((hoverPoint.memoryUsedKb * 1024 * 100) / memoryTotalBytes)
        )
      : undefined;
  const hoverDiskPercent =
    hoverPoint && hoverPoint.diskTotalBytes > 0
      ? Math.min(
          100,
          Math.round((hoverPoint.diskUsedBytes * 100) / hoverPoint.diskTotalBytes)
        )
      : undefined;
  const hoverTime = hoverPoint
    ? new Date(hoverPoint.timestamp * 1000).toLocaleTimeString()
    : "";
  const hoverCpuY =
    hoverPoint && seriesLength > 1
      ? height - (hoverPoint.cpuPercent / 100) * height
      : 0;
  const hoverMemoryY =
    hoverMemoryPercent !== undefined
      ? height - (hoverMemoryPercent / 100) * height
      : 0;
  const hoverGpuY =
    hoverPoint?.gpuPercent !== undefined
      ? height - (hoverPoint.gpuPercent / 100) * height
      : 0;
  const hoverDiskY =
    hoverDiskPercent !== undefined
      ? height - (hoverDiskPercent / 100) * height
      : 0;

  return (
    <Card className="p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="text-lg font-semibold text-slate-900">性能趋势</h3>
        <div className="flex items-center gap-1 overflow-x-auto rounded-full bg-slate-100 p-1 text-xs text-slate-500 scrollbar-none">
          {rangeOptions.map((option) => (
            <button
              key={option}
              onClick={() => onRangeChange(option)}
              className={`whitespace-nowrap rounded-full px-2 py-1 text-xs transition sm:px-3 ${
                range === option
                  ? "bg-white text-indigo-600 shadow-sm"
                  : "text-slate-500 hover:text-indigo-600 hover-select"
              }`}
            >
              {option}
            </button>
          ))}
        </div>
      </div>
      <div className="mt-2 flex flex-wrap gap-3 text-xs text-slate-500">
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
      <div
        ref={containerRef}
        className="relative mt-4"
        onMouseMove={handlePointerMove}
        onMouseLeave={handlePointerLeave}
      >
        {!hasData && (
          <div className="rounded-lg border border-dashed border-slate-200 py-8 text-center text-sm text-slate-500">
            {sampling ? "采样中..." : "暂无趋势数据"}
          </div>
        )}
        {hasData && (
          <>
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
              {hoverIndex !== null && (
                <>
                  <line
                    x1={hoverX}
                    x2={hoverX}
                    y1={0}
                    y2={height}
                    stroke="#CBD5F5"
                    strokeWidth="1"
                  />
                  <circle cx={hoverX} cy={hoverCpuY} r="3" fill="#6366F1" />
                  <circle cx={hoverX} cy={hoverMemoryY} r="3" fill="#10B981" />
                  {hoverPoint?.gpuPercent !== undefined && (
                    <circle cx={hoverX} cy={hoverGpuY} r="3" fill="#A855F7" />
                  )}
                  {hoverDiskPercent !== undefined && (
                    <circle cx={hoverX} cy={hoverDiskY} r="3" fill="#F59E0B" />
                  )}
                </>
              )}
            </svg>
            {xAxisTicks.length > 0 && (
              <div className="relative mt-2 h-4 text-xs text-slate-400">
                {xAxisTicks.map((tick) => (
                  <span
                    key={`${tick.label}-${tick.index}`}
                    className="absolute -translate-x-1/2"
                    style={{ left: `${(tick.index / (seriesLength - 1)) * 100}%` }}
                  >
                    {tick.label}
                  </span>
                ))}
              </div>
            )}
          </>
        )}
        {hoverPoint && (
          <div
            className="pointer-events-none absolute top-2 z-10 w-48 rounded-lg border border-slate-200 bg-white/95 p-3 text-xs text-slate-600 shadow-lg"
            style={{ left: `${hoverLeftPercent}%`, transform: "translateX(-50%)" }}
          >
            <p className="font-semibold text-slate-900">{hoverTime}</p>
            <div className="mt-2 space-y-1">
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-1">
                  <span className="h-1.5 w-1.5 rounded-full bg-indigo-500" />
                  CPU
                </span>
                <span>{formatPercentValue(hoverPoint.cpuPercent)}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-1">
                  <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                  内存
                </span>
                <span>{formatPercentValue(hoverMemoryPercent)}</span>
              </div>
              {hoverPoint.gpuPercent !== undefined && (
                <div className="flex items-center justify-between">
                  <span className="flex items-center gap-1">
                    <span className="h-1.5 w-1.5 rounded-full bg-purple-500" />
                    GPU
                  </span>
                  <span>{formatPercentValue(hoverPoint.gpuPercent)}</span>
                </div>
              )}
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-1">
                  <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
                  磁盘
                </span>
                <span>{formatPercentValue(hoverDiskPercent)}</span>
              </div>
            </div>
          </div>
        )}
      </div>
    </Card>
  );
}
