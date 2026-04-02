"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { Card, CardHeader, CardContent, Button, Badge, TogglePower, Modal, ConfirmModal } from "@/components/ui";
import { api, getV2rayaLogsWsUrl } from "@/lib/api";
import { formatUptime, cn } from "@/lib/utils";
import { Activity, Clock, RefreshCw, FileText, KeyRound, AlertTriangle } from "lucide-react";
import { V2rayaCheck, V2rayaStatus, LogEntry } from "@/types/api";
import { ansiToHtml, stripLogPrefix } from "@/lib/ansi";
import { useStore } from "@/stores/useStore";

function V2rayaLogModal({
  isOpen,
  onClose,
}: {
  isOpen: boolean;
  onClose: () => void;
}) {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [wsConnected, setWsConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const logsContainerRef = useRef<HTMLDivElement>(null);
  const isUnmountedRef = useRef(false);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getV2rayaLogs(200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load v2raya logs:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  const connectWs = useCallback(() => {
    let wsUrl: string;
    try {
      wsUrl = getV2rayaLogsWsUrl();
    } catch (error) {
      console.warn("Cannot connect to v2raya logs WebSocket:", error);
      setWsConnected(false);
      return;
    }

    const ws = new WebSocket(wsUrl);
    ws.onopen = () => {
      if (!isUnmountedRef.current) {
        setWsConnected(true);
      }
    };
    ws.onmessage = (event) => {
      if (isUnmountedRef.current) return;
      try {
        const entry = JSON.parse(event.data) as LogEntry;
        setLogs((prev) => [entry, ...prev].slice(0, 200));
      } catch (e) {
        console.warn("Failed to parse v2raya log entry:", e);
      }
    };
    ws.onclose = () => {
      if (!isUnmountedRef.current) {
        setWsConnected(false);
      }
    };
    ws.onerror = () => {
      if (!isUnmountedRef.current) {
        setWsConnected(false);
      }
    };
    wsRef.current = ws;
  }, []);

  useEffect(() => {
    if (isOpen) {
      isUnmountedRef.current = false;
      loadLogs();
      connectWs();
    }
    return () => {
      isUnmountedRef.current = true;
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
    };
  }, [isOpen, loadLogs, connectWs]);

  useEffect(() => {
    if (logsContainerRef.current) {
      logsContainerRef.current.scrollTop = 0;
    }
  }, [logs]);

  return (
    <Modal isOpen={isOpen} onClose={onClose} title="V2rayA 日志" size="lg">
      <div className="space-y-3">
        <div className="flex items-center gap-2 text-sm">
          <div
            className={cn(
              "w-2 h-2 rounded-full",
              wsConnected ? "bg-emerald-500" : "bg-slate-400"
            )}
          />
          <span className="text-slate-500">
            {wsConnected ? "实时连接" : "未连接"}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={loadLogs}
            disabled={loading}
          >
            <RefreshCw
              className={cn("w-3.5 h-3.5", loading && "animate-spin")}
            />
          </Button>
        </div>

        <div
          ref={logsContainerRef}
          className="h-96 overflow-y-auto bg-slate-900 rounded-lg p-4 font-mono text-xs leading-relaxed"
        >
          {logs.length === 0 && !loading && (
            <div className="text-slate-500 text-center py-8">暂无日志</div>
          )}
          {logs.map((log, index) => (
            <div
              key={index}
              className={cn(
                "py-0.5 break-all",
                log.level === "error" && "text-red-400",
                log.level === "warning" && "text-amber-400",
                log.level === "info" && "text-slate-300",
                log.level === "debug" && "text-slate-500"
              )}
            >
              <span className="text-slate-600 select-none">{log.time} </span>
              <span
                dangerouslySetInnerHTML={{
                  __html: ansiToHtml(stripLogPrefix(log.message)),
                }}
              />
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
}

export default function ProxiesPage() {
  const { addToast } = useStore();

  // V2rayA state
  const [checkResult, setCheckResult] = useState<V2rayaCheck | null>(null);
  const [v2rayaStatus, setV2rayaStatus] = useState<V2rayaStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [showLogModal, setShowLogModal] = useState(false);
  const [showResetConfirm, setShowResetConfirm] = useState(false);
  const [resetLoading, setResetLoading] = useState(false);
  const statusIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const checkV2raya = useCallback(async () => {
    try {
      const result = await api.checkV2raya();
      setCheckResult(result);
      return result.ready;
    } catch (error) {
      console.error("Failed to check v2raya:", error);
      setCheckResult(null);
      return false;
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await api.getV2rayaStatus();
      setV2rayaStatus(status);
    } catch (error) {
      console.error("Failed to get v2raya status:", error);
    }
  }, []);

  useEffect(() => {
    const init = async () => {
      setLoading(true);
      const ready = await checkV2raya();
      if (ready) {
        await refreshStatus();
      }
      setLoading(false);
    };
    init();
  }, [checkV2raya, refreshStatus]);

  // Poll status every 3 seconds
  useEffect(() => {
    if (checkResult?.ready) {
      statusIntervalRef.current = setInterval(refreshStatus, 3000);
      return () => {
        if (statusIntervalRef.current) clearInterval(statusIntervalRef.current);
      };
    }
  }, [checkResult?.ready, refreshStatus]);

  const handleToggle = useCallback(async () => {
    if (!v2rayaStatus) return;
    setActionLoading(true);
    try {
      if (v2rayaStatus.running) {
        await api.stopV2raya();
        addToast({ type: "success", message: "V2rayA 已停止" });
      } else {
        await api.startV2raya();
        addToast({ type: "success", message: "V2rayA 已启动" });
      }
      await refreshStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "操作失败",
      });
    } finally {
      setActionLoading(false);
    }
  }, [v2rayaStatus, refreshStatus, addToast]);

  const handleResetPassword = useCallback(async () => {
    setResetLoading(true);
    try {
      const result = await api.resetV2rayaPassword();
      addToast({ type: "success", message: "V2rayA 密码已重置" });
      console.log("Reset password output:", result);
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "重置密码失败",
      });
    } finally {
      setResetLoading(false);
      setShowResetConfirm(false);
    }
  }, [addToast]);

  if (loading) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-3">
          <Activity className="w-6 h-6 text-indigo-500" />
          <h1 className="text-2xl font-bold text-slate-800">代理</h1>
        </div>
        <div className="flex items-center justify-center py-20 text-slate-400">
          <RefreshCw className="w-5 h-5 animate-spin mr-2" />
          加载中...
        </div>
      </div>
    );
  }

  // V2rayA not installed
  if (!checkResult?.ready) {
    const missing: string[] = [];
    if (checkResult && !checkResult.bin_exists) missing.push("/usr/local/bin/v2raya");
    if (checkResult && !checkResult.config_exists) missing.push("/usr/local/etc/v2raya");
    if (checkResult && !checkResult.v2ray_exists) missing.push("/usr/local/bin/v2ray");

    return (
      <div className="space-y-6">
        <div className="flex items-center gap-3">
          <Activity className="w-6 h-6 text-indigo-500" />
          <h1 className="text-2xl font-bold text-slate-800">代理</h1>
        </div>
        <Card>
          <CardContent className="py-12">
            <div className="flex flex-col items-center gap-4 text-center">
              <AlertTriangle className="w-12 h-12 text-amber-500" />
              <h2 className="text-lg font-semibold text-slate-700">V2rayA 未就绪</h2>
              <p className="text-slate-500 max-w-md">
                以下路径不存在，请先安装 V2rayA 和 V2Ray：
              </p>
              <div className="bg-slate-50 rounded-lg p-4 font-mono text-sm text-slate-600 space-y-1">
                {missing.map((p) => (
                  <div key={p} className="flex items-center gap-2">
                    <span className="text-red-500">✗</span> {p}
                  </div>
                ))}
              </div>
              <Button variant="ghost" onClick={() => { setLoading(true); checkV2raya().then(() => setLoading(false)); }}>
                <RefreshCw className="w-4 h-4 mr-1" /> 重新检查
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  const isRunning = v2rayaStatus?.running ?? false;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Activity className="w-6 h-6 text-indigo-500" />
          <h1 className="text-2xl font-bold text-slate-800">代理</h1>
          <Badge variant={isRunning ? "success" : "default"}>
            {isRunning ? "运行中" : "已停止"}
          </Badge>
        </div>
      </div>

      {/* Control Panel */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between w-full">
            <h2 className="text-lg font-semibold">V2rayA</h2>
            <div className="flex items-center gap-3">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowLogModal(true)}
                title="查看日志"
              >
                <FileText className="w-4 h-4" />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowResetConfirm(true)}
                disabled={resetLoading}
                title="重置密码"
              >
                <KeyRound className="w-4 h-4" />
                <span className="ml-1">Reset Password</span>
              </Button>
              <TogglePower
                running={isRunning}
                onToggle={handleToggle}
                loading={actionLoading}
              />
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {isRunning && v2rayaStatus && (
            <div className="flex items-center gap-6 text-sm text-slate-500 mb-4">
              {v2rayaStatus.pid && (
                <span className="flex items-center gap-1">
                  PID: {v2rayaStatus.pid}
                </span>
              )}
              {v2rayaStatus.uptime_secs != null && (
                <span className="flex items-center gap-1">
                  <Clock className="w-3.5 h-3.5" />
                  {formatUptime(v2rayaStatus.uptime_secs)}
                </span>
              )}
            </div>
          )}

          {/* V2rayA Web UI iframe */}
          {isRunning ? (
            <div className="rounded-lg overflow-hidden border border-slate-200">
              <iframe
                src="/v2raya/"
                className="w-full border-0"
                style={{ minHeight: "calc(100vh - 300px)" }}
                title="V2rayA"
              />
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center py-16 text-slate-400">
              <Activity className="w-10 h-10 mb-3 opacity-30" />
              <p>V2rayA 未运行，点击启动按钮开始</p>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Log Modal */}
      <V2rayaLogModal
        isOpen={showLogModal}
        onClose={() => setShowLogModal(false)}
      />

      {/* Reset Password Confirm */}
      <ConfirmModal
        isOpen={showResetConfirm}
        onClose={() => setShowResetConfirm(false)}
        onConfirm={handleResetPassword}
        title="重置 V2rayA 密码"
        message="确定要重置 V2rayA 的管理员密码吗？重置后需要重新设置密码。"
        confirmText="确认重置"
        loading={resetLoading}
      />
    </div>
  );
}
