"use client";

import { Card } from "@/components/ui/Card/Card";
import { formatBytes } from "@/lib/format";
import { SystemInfo } from "@/types/api";

interface SystemInfoPanelProps {
  info?: SystemInfo | null;
}

export function SystemInfoPanel({ info }: SystemInfoPanelProps) {
  if (!info) {
    return null;
  }

  return (
    <Card className="p-5">
      <h3 className="text-lg font-semibold text-slate-900">系统信息</h3>
      <div className="mt-4 grid gap-3 text-sm text-slate-600 md:grid-cols-2">
        <div>
          <p className="text-xs text-slate-400">主机名</p>
          <p className="font-medium">{info.hostname}</p>
        </div>
        <div>
          <p className="text-xs text-slate-400">系统版本</p>
          <p className="font-medium">
            {info.osName} {info.osVersion}
          </p>
        </div>
        <div>
          <p className="text-xs text-slate-400">内核版本</p>
          <p className="font-medium">{info.kernelVersion}</p>
        </div>
        <div>
          <p className="text-xs text-slate-400">CPU 型号</p>
          <p className="font-medium">{info.processor?.brand}</p>
        </div>
        <div>
          <p className="text-xs text-slate-400">CPU 核心</p>
          <p className="font-medium">{info.totalProcessors}</p>
        </div>
        <div>
          <p className="text-xs text-slate-400">内存总量</p>
          <p className="font-medium">{formatBytes(info.memory)}</p>
        </div>
      </div>
    </Card>
  );
}
