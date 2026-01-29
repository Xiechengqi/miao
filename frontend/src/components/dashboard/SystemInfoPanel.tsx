"use client";

import { Card, CardContent, CardHeader } from "@/components/ui/Card/Card";
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
      <CardHeader className="mb-0">
        <h3 className="text-lg font-semibold text-slate-900">系统信息</h3>
      </CardHeader>
      <CardContent className="mt-4 grid gap-3 text-sm text-slate-600">
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="rounded-lg bg-slate-50 p-3">
            <p className="text-xs text-slate-400">主机名</p>
            <p className="mt-1 font-medium text-slate-900">{info.hostname}</p>
          </div>
          <div className="rounded-lg bg-slate-50 p-3">
            <p className="text-xs text-slate-400">系统版本</p>
            <p className="mt-1 font-medium text-slate-900">
              {info.osName} {info.osVersion}
            </p>
          </div>
          <div className="rounded-lg bg-slate-50 p-3">
            <p className="text-xs text-slate-400">内核版本</p>
            <p className="mt-1 font-medium text-slate-900">{info.kernelVersion}</p>
          </div>
          <div className="rounded-lg bg-slate-50 p-3">
            <p className="text-xs text-slate-400">CPU 型号</p>
            <p className="mt-1 font-medium text-slate-900">{info.processor?.brand}</p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
