"use client";

import { Card } from "@/components/ui/Card/Card";
import { formatBytes } from "@/lib/format";
import { SystemInfo, SystemStatus } from "@/types/api";

interface DiskUsageListProps {
  info?: SystemInfo | null;
  status?: SystemStatus | null;
}

export function DiskUsageList({ info, status }: DiskUsageListProps) {
  const disks = status?.disks ?? [];
  const diskInfoByName = new Map(info?.disks.map((disk) => [disk.name, disk]) ?? []);

  return (
    <Card className="p-5">
      <h3 className="text-lg font-semibold text-slate-900">磁盘</h3>
      <div className="mt-4 space-y-4">
        {disks.length === 0 && (
          <p className="text-sm text-slate-500">暂无磁盘信息</p>
        )}
        {disks.map((disk) => {
          const total = disk.total || 0;
          const used = disk.used || 0;
          const percent = total > 0 ? Math.round((used / total) * 100) : 0;
          const infoDisk = diskInfoByName.get(disk.name);
          return (
            <div key={disk.name} className="rounded-lg border border-slate-100 p-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-semibold text-slate-900">
                    {infoDisk?.mountPoint || disk.name}
                  </p>
                  <p className="text-xs text-slate-400">{infoDisk?.fs}</p>
                </div>
                <span className="text-sm text-slate-500">{percent}%</span>
              </div>
              <div className="mt-3 h-2 w-full rounded-full bg-slate-100">
                <div
                  className="h-2 rounded-full bg-indigo-500"
                  style={{ width: `${percent}%` }}
                />
              </div>
              <p className="mt-2 text-xs text-slate-400">
                {formatBytes(used)} / {formatBytes(total)}
              </p>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
