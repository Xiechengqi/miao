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
    <div className="space-y-8" data-onboarding="dashboard-overview">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-bold text-slate-900">性能总览</h1>
          <p className="text-sm text-slate-500">系统资源与运行状态概览</p>
        </div>
        <div className="flex items-center gap-2">
          <span className="rounded-full bg-slate-100 px-3 py-1 text-xs text-slate-600">
            更新于 {lastUpdated}
          </span>
          {loading && (
            <span className="rounded-full bg-indigo-50 px-3 py-1 text-xs text-indigo-600">
              加载中...
            </span>
          )}
        </div>
      </div>

      <SystemOverviewCards info={info} status={status} />

      <div className="grid gap-6 xl:grid-cols-[2fr_1fr]">
        <PerformanceTrendChart
          series={metrics?.series ?? []}
          memoryTotalBytes={info?.memory}
          range={range}
          rangeOptions={RANGE_OPTIONS}
          onRangeChange={setRange}
        />
        <SystemInfoPanel info={info} />
      </div>

      <DiskUsageList info={info} status={status} />
    </div>
  );
}
