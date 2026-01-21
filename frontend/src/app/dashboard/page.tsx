"use client";

import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import { useStore } from "@/stores/useStore";
import {
  SystemInfo,
  SystemMetricsResponse,
  SystemStatus,
} from "@/types/api";
import { SystemOverviewCards } from "@/components/dashboard/SystemOverviewCards";
import { GpuStatusList } from "@/components/dashboard/GpuStatusList";
import { DiskUsageList } from "@/components/dashboard/DiskUsageList";
import { SystemInfoPanel } from "@/components/dashboard/SystemInfoPanel";
import { PerformanceTrendChart } from "@/components/dashboard/PerformanceTrendChart";

const RANGE_OPTIONS = ["15m", "1h", "6h", "24h", "7d"];

export default function DashboardPage() {
  const { addToast } = useStore();
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [metrics, setMetrics] = useState<SystemMetricsResponse | null>(null);
  const [range, setRange] = useState("1h");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;

    const loadInitial = async () => {
      try {
        const [infoData, statusData] = await Promise.all([
          api.getSystemInfo(),
          api.getSystemStatus(),
        ]);
        if (!active) return;
        setInfo(infoData);
        setStatus(statusData);
      } catch (error) {
        if (active) {
          addToast({ type: "error", message: "获取系统信息失败" });
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    };

    loadInitial();

    return () => {
      active = false;
    };
  }, [addToast]);

  useEffect(() => {
    let active = true;
    const timer = setInterval(async () => {
      try {
        const data = await api.getSystemStatus();
        if (active) {
          setStatus(data);
        }
      } catch (error) {
        if (active) {
          addToast({ type: "error", message: "刷新系统状态失败" });
        }
      }
    }, 5000);

    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [addToast]);

  useEffect(() => {
    let active = true;

    const loadMetrics = async () => {
      try {
        const data = await api.getSystemMetrics(range);
        if (active) {
          setMetrics(data);
        }
      } catch (error) {
        if (active) {
          addToast({ type: "error", message: "获取趋势数据失败" });
        }
      }
    };

    loadMetrics();

    return () => {
      active = false;
    };
  }, [addToast, range]);

  const lastUpdated = useMemo(() => {
    if (!status) return "-";
    return new Date(status.timestamp * 1000).toLocaleTimeString();
  }, [status]);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">性能总览</h1>
          <p className="text-sm text-slate-500">最后更新：{lastUpdated}</p>
        </div>
        {loading && (
          <span className="rounded-full bg-indigo-50 px-3 py-1 text-xs text-indigo-600">
            加载中...
          </span>
        )}
      </div>

      <SystemOverviewCards info={info} status={status} />

      <div className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-sm font-semibold text-slate-700">趋势区间</p>
          <div className="flex flex-wrap gap-2">
            {RANGE_OPTIONS.map((option) => (
              <button
                key={option}
                onClick={() => setRange(option)}
                className={`rounded-full border px-3 py-1 text-xs transition ${
                  range === option
                    ? "border-indigo-500 bg-indigo-50 text-indigo-600"
                    : "border-slate-200 text-slate-500 hover:border-indigo-200 hover:text-indigo-600"
                }`}
              >
                {option}
              </button>
            ))}
          </div>
        </div>
        <PerformanceTrendChart
          series={metrics?.series ?? []}
          memoryTotalBytes={info?.memory}
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <GpuStatusList info={info} status={status} />
        <DiskUsageList info={info} status={status} />
      </div>

      <SystemInfoPanel info={info} />
    </div>
  );
}
