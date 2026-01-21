"use client";

import { Cpu, HardDrive, MemoryStick, Monitor } from "lucide-react";
import { Card } from "@/components/ui/Card/Card";
import { formatBytes, formatKb, formatPercent } from "@/lib/format";
import { SystemInfo, SystemStatus } from "@/types/api";

interface SystemOverviewCardsProps {
  info?: SystemInfo | null;
  status?: SystemStatus | null;
}

function averageGpuPercent(status?: SystemStatus | null): number | undefined {
  if (!status || status.graphics.length === 0) {
    return undefined;
  }
  const total = status.graphics.reduce((sum, gpu) => sum + gpu.gpu, 0);
  return total / status.graphics.length;
}

function primaryDiskUsage(status?: SystemStatus | null): { used: number; total: number } | null {
  if (!status || status.disks.length === 0) {
    return null;
  }
  return { used: status.disks[0].used, total: status.disks[0].total };
}

export function SystemOverviewCards({ info, status }: SystemOverviewCardsProps) {
  const gpuPercent = averageGpuPercent(status);
  const diskUsage = primaryDiskUsage(status);
  const memoryUsed = status ? formatKb(status.memoryUsedKb) : "-";
  const memoryTotal = info ? formatBytes(info.memory) : "-";
  const diskUsed = diskUsage ? formatBytes(diskUsage.used) : "-";
  const diskTotal = diskUsage ? formatBytes(diskUsage.total) : "-";

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
      <Card className="p-5">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-slate-500">CPU 使用率</p>
            <p className="mt-2 text-2xl font-semibold text-slate-900">
              {formatPercent(status?.cpuPercent)}
            </p>
          </div>
          <div className="rounded-lg bg-indigo-50 p-3 text-indigo-600">
            <Cpu className="h-5 w-5" />
          </div>
        </div>
      </Card>
      <Card className="p-5">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-slate-500">内存使用</p>
            <p className="mt-2 text-2xl font-semibold text-slate-900">
              {memoryUsed}
            </p>
            <p className="text-xs text-slate-400">总计 {memoryTotal}</p>
          </div>
          <div className="rounded-lg bg-emerald-50 p-3 text-emerald-600">
            <MemoryStick className="h-5 w-5" />
          </div>
        </div>
      </Card>
      <Card className="p-5">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-slate-500">GPU 使用率</p>
            <p className="mt-2 text-2xl font-semibold text-slate-900">
              {formatPercent(gpuPercent)}
            </p>
            {gpuPercent === undefined && (
              <p className="text-xs text-slate-400">未检测到 GPU</p>
            )}
          </div>
          <div className="rounded-lg bg-purple-50 p-3 text-purple-600">
            <Monitor className="h-5 w-5" />
          </div>
        </div>
      </Card>
      <Card className="p-5">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-slate-500">磁盘使用</p>
            <p className="mt-2 text-2xl font-semibold text-slate-900">
              {diskUsed}
            </p>
            <p className="text-xs text-slate-400">总计 {diskTotal}</p>
          </div>
          <div className="rounded-lg bg-amber-50 p-3 text-amber-600">
            <HardDrive className="h-5 w-5" />
          </div>
        </div>
      </Card>
    </div>
  );
}
