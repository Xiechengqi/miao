"use client";

import { useEffect, useState, useRef } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { IVncStatus, IVncConfig, LogEntry } from "@/types/api";
import { formatUptime } from "@/lib/utils";
import {
  Monitor,
  ExternalLink,
  RefreshCw,
  Play,
  Square,
  Settings,
  Download,
  AlertTriangle,
  FileText,
  Eye,
  EyeOff,
} from "lucide-react";

interface UpgradeLogEntry {
  step: number;
  total_steps: number;
  message: string;
  level: string;
  progress?: number;
}

export default function VncPage() {
  const { setLoading, loading, addToast } = useStore();

  const [status, setStatus] = useState<IVncStatus | null>(null);
  const [config, setConfig] = useState<IVncConfig | null>(null);
  const [installing, setInstalling] = useState(false);
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [showLogsModal, setShowLogsModal] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [configForm, setConfigForm] = useState<IVncConfig | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [upgrading, setUpgrading] = useState(false);
  const [showUpgradeModal, setShowUpgradeModal] = useState(false);
  const [upgradeLogs, setUpgradeLogs] = useState<UpgradeLogEntry[]>([]);
  const [upgradeProgress, setUpgradeProgress] = useState(0);
  const [upgradeStatus, setUpgradeStatus] = useState<"running" | "success" | "error">("running");
  const upgradeLogsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadStatus();
    loadConfig();
  }, []);

  const loadStatus = async () => {
    try {
      const data = await api.getIVncStatus();
      setStatus(data);
    } catch (error) {
      console.error("Failed to load iVnc status:", error);
    }
  };

  const loadConfig = async () => {
    try {
      const data = await api.getIVncConfig();
      setConfig(data);
      setConfigForm(data);
    } catch (error) {
      console.error("Failed to load iVnc config:", error);
    }
  };

  const loadLogs = async () => {
    try {
      const data = await api.getIVncLogs(200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load logs:", error);
    }
  };

  const handleInstall = async () => {
    setInstalling(true);
    try {
      await api.installIVnc();
      addToast({ type: "success", message: "iVnc 安装成功" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "安装失败",
      });
    } finally {
      setInstalling(false);
    }
  };

  const handleUpgrade = async () => {
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
          loadStatus();
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

  const handleStart = async () => {
    setLoading(true, "start");
    try {
      await api.startIVnc();
      addToast({ type: "success", message: "iVnc 已启动" });
      await loadStatus();
      await loadLogs(); // 启动后加载日志
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "启动失败",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    setLoading(true, "stop");
    try {
      await api.stopIVnc();
      addToast({ type: "success", message: "iVnc 已停止" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "停止失败",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleRestart = async () => {
    setLoading(true, "restart");
    try {
      await api.restartIVnc();
      addToast({ type: "success", message: "iVnc 已重启" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "重启失败",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenDesktop = () => {
    if (!status || !status.running) return;
    const url = `http://${window.location.hostname}:${status.port}`;
    window.open(url, "_blank");
  };

  const handleOpenConsole = () => {
    if (!status || !status.running) return;
    const url = `http://${window.location.hostname}:${status.port}/console`;
    window.open(url, "_blank");
  };

  const handleSaveConfig = async () => {
    if (!configForm) return;
    setLoading(true, "save");
    try {
      await api.updateIVncConfig(configForm);
      addToast({ type: "success", message: "配置已保存" });
      setShowConfigModal(false);
      await loadConfig();
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "保存失败",
      });
    } finally {
      setLoading(false);
    }
  };

  if (status?.installed === false) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-black">远程桌面</h1>
          <p className="text-slate-500 mt-1">基于 iVnc 的 WebRTC 桌面流媒体</p>
        </div>
        <Card className="p-6 bg-amber-50 border-amber-200">
          <div className="flex items-start gap-3">
            <AlertTriangle className="w-5 h-5 text-amber-600 shrink-0 mt-0.5" />
            <div className="space-y-3 w-full">
              <p className="font-semibold text-amber-800">iVnc 未安装</p>
              <p className="text-sm text-amber-700">
                iVnc 是基于 Wayland 的高性能桌面流媒体服务，使用 WebRTC 实现低延迟传输。
              </p>
              <Button onClick={handleInstall} loading={installing}>
                <Download className="w-4 h-4" />
                安装 iVnc
              </Button>
            </div>
          </div>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-black">远程桌面</h1>
          <p className="text-slate-500 mt-1">基于 iVnc 的 WebRTC 桌面流媒体</p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="secondary"
            onClick={handleUpgrade}
            loading={upgrading}
          >
            <RefreshCw className="w-4 h-4" />
            更新
          </Button>
          <Button
            variant="secondary"
            onClick={() => {
              loadLogs();
              setShowLogsModal(true);
            }}
          >
            <FileText className="w-4 h-4" />
            日志
          </Button>
          <Button
            variant="secondary"
            onClick={() => {
              setConfigForm(config);
              setShowConfigModal(true);
            }}
          >
            <Settings className="w-4 h-4" />
            配置
          </Button>
        </div>
      </div>

      <Card className="p-6">
        <div className="flex items-center justify-between">
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 rounded-lg bg-violet-600/10 flex items-center justify-center">
              <Monitor className="w-6 h-6 text-violet-600" />
            </div>
            <div>
              <div className="flex items-center gap-2 mb-1">
                <span className="text-xl font-bold">iVnc 桌面</span>
                <Badge variant={status?.running ? "success" : "default"}>
                  {status?.running ? "运行中" : "已停止"}
                </Badge>
              </div>
              <p className="text-sm text-slate-500">
                版本: {status?.version || "未知"} · 端口: {status?.port}
              </p>
              {status?.running && (
                <p className="text-sm text-slate-500">
                  PID: {status.pid} · 运行时间: {formatUptime(status.uptime_secs || 0)}
                </p>
              )}
            </div>
          </div>

          <div className="flex gap-2">
            {status?.running ? (
              <>
                <Button variant="primary" onClick={handleOpenDesktop}>
                  <ExternalLink className="w-4 h-4" />
                  打开桌面
                </Button>
                <Button variant="secondary" onClick={handleOpenConsole}>
                  <Settings className="w-4 h-4" />
                  管理页
                </Button>
                <Button variant="secondary" onClick={handleRestart}>
                  <RefreshCw className="w-4 h-4" />
                  重启
                </Button>
                <Button variant="secondary" onClick={handleStop}>
                  <Square className="w-4 h-4" />
                  停止
                </Button>
              </>
            ) : (
              <Button onClick={handleStart}>
                <Play className="w-4 h-4" />
                启动
              </Button>
            )}
          </div>
        </div>
      </Card>

      {/* Config Modal */}
      <Modal
        isOpen={showConfigModal}
        onClose={() => setShowConfigModal(false)}
        title="iVnc 配置"
        size="lg"
      >
        {configForm && (
          <div className="space-y-4">
            <Input
              label="端口"
              type="number"
              value={configForm.port}
              onChange={(e) =>
                setConfigForm({ ...configForm, port: parseInt(e.target.value) || 8008 })
              }
            />
            <Input
              label="用户名"
              value={configForm.basic_auth_user}
              onChange={(e) =>
                setConfigForm({ ...configForm, basic_auth_user: e.target.value })
              }
            />
            <div>
              <label className="block text-sm font-medium text-slate-700 mb-1">
                密码
              </label>
              <div className="relative">
                <input
                  type={showPassword ? "text" : "password"}
                  value={configForm.basic_auth_password}
                  onChange={(e) =>
                    setConfigForm({ ...configForm, basic_auth_password: e.target.value })
                  }
                  className="w-full px-3 py-2 border border-slate-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-violet-500 pr-10"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(!showPassword)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-700"
                >
                  {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                </button>
              </div>
            </div>
            <Input
              label="帧率 (FPS)"
              type="number"
              value={configForm.target_fps}
              onChange={(e) =>
                setConfigForm({ ...configForm, target_fps: parseInt(e.target.value) || 30 })
              }
            />
            <Input
              label="视频码率 (kbps)"
              type="number"
              value={configForm.video_bitrate}
              onChange={(e) =>
                setConfigForm({ ...configForm, video_bitrate: parseInt(e.target.value) || 4000 })
              }
            />
            <div className="flex justify-end gap-3 pt-4">
              <Button variant="secondary" onClick={() => setShowConfigModal(false)}>
                取消
              </Button>
              <Button onClick={handleSaveConfig} loading={loading}>
                保存
              </Button>
            </div>
          </div>
        )}
      </Modal>

      {/* Logs Modal */}
      <Modal
        isOpen={showLogsModal}
        onClose={() => setShowLogsModal(false)}
        title="iVnc 日志"
        size="lg"
      >
        <div className="space-y-4">
          <div className="flex justify-end">
            <Button variant="secondary" size="sm" onClick={loadLogs}>
              <RefreshCw className="w-4 h-4" />
              刷新
            </Button>
          </div>
          <div className="max-h-96 overflow-y-auto bg-slate-900 rounded-lg p-4 font-mono text-sm">
            {logs.length === 0 ? (
              <div className="text-slate-500 text-center py-8">暂无日志</div>
            ) : (
              <div className="space-y-1">
                {logs.map((log, index) => (
                  <div key={index} className="text-slate-200 whitespace-pre-wrap break-all">
                    {log.message}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </Modal>

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
                className={`h-full transition-all duration-300 rounded-full ${
                  upgradeStatus === "error" ? "bg-red-500" :
                  upgradeStatus === "success" ? "bg-emerald-500" : "bg-blue-500"
                }`}
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
                className={`py-0.5 ${
                  log.level === "error" ? "text-red-400" :
                  log.level === "success" ? "text-emerald-400" :
                  log.level === "info" ? "text-slate-300" : "text-sky-400"
                }`}
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
            <span className={`text-sm font-medium ${
              upgradeStatus === "error" ? "text-red-600" :
              upgradeStatus === "success" ? "text-emerald-600" : "text-slate-600"
            }`}>
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
