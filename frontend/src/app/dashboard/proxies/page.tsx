"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { Card, CardContent, Button, Badge, TogglePower, Modal, ConfirmModal } from "@/components/ui";
import { api, getV2rayaLogsWsUrl } from "@/lib/api";
import { formatUptime, cn } from "@/lib/utils";
import { Activity, Clock, RefreshCw, FileText, KeyRound, AlertTriangle, Play, Square, LoaderCircle } from "lucide-react";
import { V2rayaCheck, V2rayaStatus, LogEntry } from "@/types/api";
import { ansiToHtml, stripLogPrefix } from "@/lib/ansi";
import { useStore } from "@/stores/useStore";

type ConnectivitySite = {
  name: string;
  url: string;
};

type ConnectivityResult = {
  success: boolean;
  latency_ms?: number;
};

const CONNECTIVITY_SITES: ConnectivitySite[] = [
  { name: "Google", url: "https://www.google.com" },
  { name: "GitHub", url: "https://github.com" },
  { name: "YouTube", url: "https://www.youtube.com" },
  { name: "Twitter", url: "https://twitter.com" },
  { name: "Telegram", url: "https://telegram.org" },
  { name: "Baidu", url: "https://www.baidu.com" },
];

const CONNECTIVITY_STORAGE_KEY = "proxy-connectivity-results";

function getConnectivityTone(result?: ConnectivityResult) {
  if (!result) {
    return "border-slate-200 bg-white text-slate-500";
  }
  if (!result.success) {
    return "border-red-200 bg-red-50 text-red-700";
  }
  const latency = result.latency_ms ?? 0;
  if (latency < 80) {
    return "border-emerald-200 bg-emerald-50 text-emerald-700";
  }
  if (latency < 180) {
    return "border-sky-200 bg-sky-50 text-sky-700";
  }
  return "border-amber-200 bg-amber-50 text-amber-700";
}

function formatConnectivityResult(result?: ConnectivityResult) {
  if (!result) return "--";
  if (!result.success) return "超时";
  return `${result.latency_ms ?? 0} ms`;
}

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
  const [v2rayaWebUrl, setV2rayaWebUrl] = useState("");
  const [iframeKey, setIframeKey] = useState(0);
  const [connectivityResults, setConnectivityResults] = useState<Record<string, ConnectivityResult>>({});
  const [testingConnectivity, setTestingConnectivity] = useState(false);
  const [currentTestingSite, setCurrentTestingSite] = useState<string | null>(null);
  const statusIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const stopConnectivityRef = useRef(false);

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

  useEffect(() => {
    setV2rayaWebUrl(`http://${window.location.hostname}:2017`);
  }, []);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem(CONNECTIVITY_STORAGE_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw) as Record<string, ConnectivityResult>;
      if (!parsed || typeof parsed !== "object") return;
      setConnectivityResults(parsed);
    } catch (error) {
      console.warn("Failed to restore connectivity cache:", error);
    }
  }, []);

  useEffect(() => {
    try {
      window.localStorage.setItem(
        CONNECTIVITY_STORAGE_KEY,
        JSON.stringify(connectivityResults)
      );
    } catch (error) {
      console.warn("Failed to persist connectivity cache:", error);
    }
  }, [connectivityResults]);

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

  const testSingleSite = useCallback(async (site: ConnectivitySite) => {
    setCurrentTestingSite(site.name);
    try {
      const result = await api.testConnectivity(site.url);
      setConnectivityResults((prev) => ({
        ...prev,
        [site.name]: {
          success: result.success,
          latency_ms: result.latency_ms,
        },
      }));
    } catch {
      setConnectivityResults((prev) => ({
        ...prev,
        [site.name]: { success: false },
      }));
    } finally {
      setCurrentTestingSite((prev) => (prev === site.name ? null : prev));
    }
  }, []);

  const handleTestAllConnectivity = useCallback(async () => {
    setTestingConnectivity(true);
    stopConnectivityRef.current = false;
    setConnectivityResults({});

    for (const site of CONNECTIVITY_SITES) {
      await testSingleSite(site);
      if (stopConnectivityRef.current) {
        break;
      }
    }

    stopConnectivityRef.current = false;
    setTestingConnectivity(false);
  }, [testSingleSite]);

  const handleStopConnectivity = useCallback(() => {
    stopConnectivityRef.current = true;
    setTestingConnectivity(false);
  }, []);

  const handleTestSingleSite = useCallback(async (site: ConnectivitySite) => {
    if (testingConnectivity || currentTestingSite) return;
    await testSingleSite(site);
  }, [currentTestingSite, testSingleSite, testingConnectivity]);

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
    if (checkResult && !checkResult.bin_exists) missing.push("v2raya（miao 根目录下）");
    if (checkResult && !checkResult.v2ray_exists) missing.push("v2ray（miao 根目录下）");

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
  const connectivityDisabled = !isRunning || testingConnectivity || Boolean(currentTestingSite);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-3">
            <Activity className="w-6 h-6 text-indigo-500" />
            <h1 className="text-2xl font-bold text-slate-800">代理</h1>
            <Badge variant={isRunning ? "success" : "default"}>
              {isRunning ? "运行中" : "已停止"}
            </Badge>
          </div>
          {isRunning && v2rayaStatus && (
            <div className="flex flex-wrap items-center gap-6 text-sm text-slate-500">
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
        </div>
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

      {/* Control Panel */}
      <Card>
        <CardContent>
          {/* V2rayA Web UI iframe */}
          {isRunning ? (
            <div className="rounded-lg overflow-hidden border border-slate-200 bg-white">
              <div className="border-b border-slate-100 bg-slate-50/80 px-4 py-4 sm:px-5">
                <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
                  <div className="flex min-w-0 items-center gap-3 overflow-x-auto pb-1 xl:pb-0">
                    {CONNECTIVITY_SITES.map((site) => {
                      const result = connectivityResults[site.name];
                      const isTesting = currentTestingSite === site.name;
                      return (
                        <button
                          key={site.name}
                          type="button"
                          onClick={() => void handleTestSingleSite(site)}
                          disabled={connectivityDisabled}
                          className={cn(
                            "flex h-11 shrink-0 items-center gap-2 rounded-xl border px-3 text-left transition-all duration-200",
                            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-offset-2",
                            "disabled:cursor-not-allowed disabled:opacity-70",
                            !connectivityDisabled && "hover:-translate-y-0.5 hover:shadow-sm",
                            getConnectivityTone(result),
                            isTesting && "border-indigo-200 bg-indigo-50 text-indigo-700"
                          )}
                        >
                          <span className="text-sm font-semibold">{site.name}</span>
                          {isTesting && <LoaderCircle className="h-3.5 w-3.5 animate-spin" />}
                          <Badge
                            variant={
                              !result ? "default" :
                              !result.success ? "error" :
                              (result.latency_ms ?? 0) < 80 ? "success" :
                              (result.latency_ms ?? 0) < 180 ? "info" :
                              "warning"
                            }
                            className="min-w-[68px] justify-center"
                          >
                            {formatConnectivityResult(result)}
                          </Badge>
                        </button>
                      );
                    })}
                  </div>
                  <div className="flex shrink-0 justify-end">
                    <Button
                      variant={testingConnectivity ? "danger" : "secondary"}
                      size="sm"
                      onClick={testingConnectivity ? handleStopConnectivity : handleTestAllConnectivity}
                      disabled={!isRunning}
                    >
                      {testingConnectivity ? (
                        <>
                          <Square className="mr-2 h-3.5 w-3.5" />
                          停止测试
                        </>
                      ) : (
                        <>
                          <Play className="mr-2 h-3.5 w-3.5" />
                          开始测试
                        </>
                      )}
                    </Button>
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-2 border-b border-slate-100 px-4 py-3 sm:px-5">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setIframeKey((prev) => prev + 1)}
                  title="重新加载 V2rayA"
                >
                  <RefreshCw className="w-4 h-4" />
                </Button>
                <a
                  href={v2rayaWebUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="min-w-0 truncate text-sm font-mono text-slate-600 hover:text-violet-600"
                >
                  {v2rayaWebUrl}
                </a>
              </div>

              <iframe
                key={iframeKey}
                src={v2rayaWebUrl}
                className="w-full border-0"
                style={{ minHeight: "calc(100vh - 240px)" }}
                title="V2rayA"
              />
            </div>
          ) : (
            <div className="space-y-4">
              <div className="rounded-lg border border-slate-200 bg-slate-50 px-4 py-4 sm:px-5">
                <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
                  <div className="flex min-w-0 items-center gap-3 overflow-x-auto pb-1 xl:pb-0">
                    {CONNECTIVITY_SITES.map((site) => (
                      <div
                        key={site.name}
                        className="flex h-11 shrink-0 items-center gap-2 rounded-xl border border-slate-200 bg-white px-3 text-slate-400"
                      >
                        <span className="text-sm font-semibold text-slate-600">{site.name}</span>
                        <Badge variant="default" className="min-w-[68px] justify-center">
                          --
                        </Badge>
                      </div>
                    ))}
                  </div>
                  <div className="flex shrink-0 justify-end">
                    <Button variant="secondary" size="sm" disabled>
                      <Play className="mr-2 h-3.5 w-3.5" />
                      开始测试
                    </Button>
                  </div>
                </div>
              </div>

              <div className="flex flex-col items-center justify-center py-16 text-slate-400">
                <Activity className="w-10 h-10 mb-3 opacity-30" />
                <p>V2rayA 未运行，点击启动按钮开始</p>
              </div>
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
