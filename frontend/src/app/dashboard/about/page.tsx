"use client";

import { useEffect, useState, useRef } from "react";
import { ClayBlobs, Modal, Button } from "@/components/ui";
import { Box, GitCommit, Clock, ExternalLink, RefreshCw } from "lucide-react";
import { cn } from "@/lib/utils";

interface BuildInfo {
  version: string;
  commit: string;
  commitDate: string;
  commitMessage: string;
  buildTime: string;
}

interface UpgradeLogEntry {
  step: number;
  total_steps: number;
  message: string;
  level: string;
  progress?: number;
}

async function getBuildInfo(): Promise<BuildInfo> {
  try {
    const res = await fetch("/build-info.json");
    if (res.ok) {
      return await res.json();
    }
  } catch {
    // ignore
  }
  return {
    version: "",
    commit: "",
    commitDate: "",
    commitMessage: "",
    buildTime: "",
  };
}

export default function AboutPage() {
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [upgrading, setUpgrading] = useState(false);
  const [showUpgradeModal, setShowUpgradeModal] = useState(false);
  const [upgradeLogs, setUpgradeLogs] = useState<UpgradeLogEntry[]>([]);
  const [upgradeProgress, setUpgradeProgress] = useState(0);
  const [upgradeStatus, setUpgradeStatus] = useState<"running" | "success" | "error">("running");
  const upgradeLogsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        const build = await getBuildInfo();
        setBuildInfo(build);
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, []);

  const handleIvncUpgrade = async () => {
    if (upgrading) return;
    if (!confirm("确定要更新 iVNC 吗？")) return;

    setUpgrading(true);
    setShowUpgradeModal(true);
    setUpgradeLogs([]);
    setUpgradeProgress(0);
    setUpgradeStatus("running");

    const token = localStorage.getItem("miao_token");
    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${wsProtocol}//${window.location.host}/api/binaries/upgrade/ivnc/ws?token=${token}`;

    const ws = new WebSocket(wsUrl);

    ws.onmessage = (event) => {
      try {
        const entry: UpgradeLogEntry = JSON.parse(event.data);
        setUpgradeLogs((prev) => [...prev, entry]);
        setUpgradeProgress(Math.round((entry.step / entry.total_steps) * 100));
        if (entry.level === "error") setUpgradeStatus("error");
        setTimeout(() => {
          if (upgradeLogsRef.current) {
            upgradeLogsRef.current.scrollTop = upgradeLogsRef.current.scrollHeight;
          }
        }, 50);
      } catch {}
    };

    ws.onclose = () => {
      setUpgradeLogs((prev) => {
        const hasError = prev.some((log) => log.level === "error");
        if (!hasError && prev.length > 0) {
          setUpgradeStatus("success");
        }
        setUpgrading(false);
        return prev;
      });
    };

    ws.onerror = () => {
      setUpgradeStatus("error");
      setUpgrading(false);
    };
  };

  if (loading) {
    return (
      <div className="min-h-[60vh] flex items-center justify-center">
        <ClayBlobs />
        <div className="text-center">
          <div className="w-10 h-10 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      </div>
    );
  }

  const displayCommit = buildInfo?.commit || "Unknown";

  return (
    <div className="max-w-2xl mx-auto">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-slate-800">版本信息</h1>
        <p className="text-slate-500 mt-1">查看当前运行的版本和构建信息</p>
      </div>

      <div className="bg-white/70 backdrop-blur-xl rounded-2xl shadow-[0_4px_20px_-2px_rgba(79,70,229,0.1)] overflow-hidden">
        {/* Header */}
        <div className="bg-gradient-to-br from-[#A78BFA]/20 to-[#7C3AED]/10 px-8 py-10 text-center">
          <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-[#A78BFA] to-[#7C3AED] flex items-center justify-center mx-auto shadow-[0_4px_14px_0_rgba(79,70,229,0.3)]">
            <Box className="w-8 h-8 text-white" />
          </div>
          <h2 className="text-3xl font-black text-slate-800 mt-4">Miao</h2>
          <p className="text-slate-500 mt-1">代理服务管理面板</p>
        </div>

        {/* Info List */}
        <div className="divide-y divide-slate-100">
          {/* Commit */}
          <div className="px-8 py-5 flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-emerald-50 flex items-center justify-center">
                <GitCommit className="w-5 h-5 text-emerald-600" />
              </div>
              <div>
                <p className="text-sm text-slate-500">Commit</p>
                <p className="font-mono font-semibold text-slate-800">
                  {displayCommit}
                </p>
                {buildInfo?.commitMessage && (
                  <p className="text-sm text-slate-500 mt-1">{buildInfo.commitMessage}</p>
                )}
              </div>
            </div>
            {displayCommit && displayCommit !== "Unknown" && (
              <a
                href={`https://github.com/Xiechengqi/miao/commit/${displayCommit}`}
                target="_blank"
                rel="noopener noreferrer"
                className={cn(
                  "px-4 py-2 rounded-lg text-sm font-semibold text-emerald-600",
                  "bg-emerald-50 hover:bg-emerald-100 transition-colors cursor-pointer",
                  "flex items-center gap-2"
                )}
              >
                查看详情
                <ExternalLink className="w-4 h-4" />
              </a>
            )}
          </div>

          {/* Build Time */}
          {buildInfo?.buildTime && (
            <div className="px-8 py-5 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-amber-50 flex items-center justify-center">
                <Clock className="w-5 h-5 text-amber-600" />
              </div>
              <div>
                <p className="text-sm text-slate-500">构建时间 (UTC+8)</p>
                <p className="font-semibold text-slate-800">{buildInfo.buildTime}</p>
              </div>
            </div>
          )}

          {/* Commit Date */}
          {buildInfo?.commitDate && (
            <div className="px-8 py-5 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-rose-50 flex items-center justify-center">
                <GitCommit className="w-5 h-5 text-rose-600" />
              </div>
              <div>
                <p className="text-sm text-slate-500">提交时间 (UTC+8)</p>
                <p className="font-semibold text-slate-800">{buildInfo.commitDate}</p>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* iVNC Section */}
      <div className="mt-8 bg-white/70 backdrop-blur-xl rounded-2xl shadow-[0_4px_20px_-2px_rgba(79,70,229,0.1)] overflow-hidden">
        <div className="bg-gradient-to-br from-blue-500/20 to-blue-600/10 px-8 py-10 text-center">
          <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-blue-500 to-blue-600 flex items-center justify-center mx-auto shadow-[0_4px_14px_0_rgba(59,130,246,0.3)]">
            <Box className="w-8 h-8 text-white" />
          </div>
          <h2 className="text-3xl font-black text-slate-800 mt-4">iVNC</h2>
          <p className="text-slate-500 mt-1">远程桌面服务</p>
        </div>
        <div className="px-8 py-6 flex justify-center">
          <button
            onClick={handleIvncUpgrade}
            disabled={upgrading}
            className={cn(
              "px-6 py-3 rounded-lg font-semibold transition-all flex items-center gap-2",
              upgrading
                ? "bg-slate-200 text-slate-400 cursor-not-allowed"
                : "bg-blue-500 text-white hover:bg-blue-600 shadow-md hover:shadow-lg"
            )}
          >
            <RefreshCw className={cn("w-5 h-5", upgrading && "animate-spin")} />
            {upgrading ? "更新中..." : "更新 iVNC"}
          </button>
        </div>
      </div>

      {/* Footer */}
      <div className="mt-8 text-center text-slate-400 text-sm">
        <p>Miao 控制面板 - 代理服务管理面板</p>
        <p className="mt-1">
          <a
            href="https://github.com/Xiechengqi/miao"
            target="_blank"
            rel="noopener noreferrer"
            className="text-indigo-600 hover:text-indigo-700"
          >
            GitHub
          </a>
        </p>
      </div>

      {/* Upgrade Modal */}
      <Modal
        isOpen={showUpgradeModal}
        onClose={() => {
          if (upgradeStatus !== "running") {
            setShowUpgradeModal(false);
          }
        }}
        title="iVNC 更新"
        size="lg"
      >
        <div className="space-y-4">
          <div className="space-y-2">
            <div className="flex justify-between text-sm text-slate-600">
              <span>更新进度</span>
              <span>{upgradeProgress}%</span>
            </div>
            <div className="h-2 bg-slate-200 rounded-full overflow-hidden">
              <div
                className={cn(
                  "h-full transition-all duration-300 rounded-full",
                  upgradeStatus === "error" ? "bg-red-500" :
                  upgradeStatus === "success" ? "bg-emerald-500" : "bg-blue-500"
                )}
                style={{ width: `${upgradeProgress}%` }}
              />
            </div>
          </div>
          <div
            ref={upgradeLogsRef}
            className="h-64 overflow-y-auto bg-slate-900 rounded-lg p-4 font-mono text-sm"
          >
            {upgradeLogs.map((log, index) => (
              <div
                key={index}
                className={cn(
                  "py-0.5",
                  log.level === "error" && "text-red-400",
                  log.level === "success" && "text-emerald-400",
                  log.level === "info" && "text-slate-300",
                  log.level === "progress" && "text-sky-400"
                )}
              >
                <span className="text-slate-500">[{log.step}/{log.total_steps}]</span>{" "}
                {log.message}
                {log.level === "progress" && log.progress != null && (
                  <span className="text-slate-500"> ({log.progress}%)</span>
                )}
              </div>
            ))}
            {upgradeStatus === "running" && upgradeLogs.length > 0 && (
              <div className="text-slate-500 animate-pulse">▌</div>
            )}
          </div>
          <div className="flex items-center justify-between">
            <span className={cn(
              "text-sm font-medium",
              upgradeStatus === "error" && "text-red-600",
              upgradeStatus === "success" && "text-emerald-600",
              upgradeStatus === "running" && "text-slate-600"
            )}>
              {upgradeStatus === "running" && "更新中，请勿关闭页面..."}
              {upgradeStatus === "success" && "更新成功"}
              {upgradeStatus === "error" && "更新失败"}
            </span>
            {upgradeStatus !== "running" && (
              <Button
                variant={upgradeStatus === "error" ? "secondary" : "primary"}
                size="sm"
                onClick={() => setShowUpgradeModal(false)}
              >
                关闭
              </Button>
            )}
          </div>
        </div>
      </Modal>
    </div>
  );
}
