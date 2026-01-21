"use client";

import { Card } from "@/components/ui/Card/Card";
import { formatBytes, formatPercent } from "@/lib/format";
import { SystemInfo, SystemStatus } from "@/types/api";

interface GpuStatusListProps {
  info?: SystemInfo | null;
  status?: SystemStatus | null;
}

export function GpuStatusList({ info, status }: GpuStatusListProps) {
  const gpuInfoById = new Map(info?.graphics.map((gpu) => [gpu.id, gpu]) ?? []);
  const items = status?.graphics ?? [];

  return (
    <Card className="p-5">
      <h3 className="text-lg font-semibold text-slate-900">GPU</h3>
      <div className="mt-4 space-y-4">
        {items.length === 0 && (
          <p className="text-sm text-slate-500">未检测到 GPU</p>
        )}
        {items.map((gpu) => {
          const gpuInfo = gpuInfoById.get(gpu.id);
          return (
            <div key={gpu.id} className="rounded-lg border border-slate-100 p-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-semibold text-slate-900">
                    {gpuInfo?.name || gpu.id}
                  </p>
                  <p className="text-xs text-slate-400">{gpuInfo?.brand}</p>
                </div>
                <span className="text-sm text-slate-500">
                  {gpu.temperature}°C
                </span>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-3 text-sm text-slate-600">
                <div>
                  <p className="text-xs text-slate-400">GPU 使用率</p>
                  <p className="font-medium">{formatPercent(gpu.gpu)}</p>
                </div>
                <div>
                  <p className="text-xs text-slate-400">显存占用</p>
                  <p className="font-medium">{formatBytes(gpu.memoryUsed)}</p>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
