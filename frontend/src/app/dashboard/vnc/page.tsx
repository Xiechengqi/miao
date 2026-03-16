"use client";

import { useEffect, useState } from "react";
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

  const handleStart = async () => {
    setLoading(true, "start");
    try {
      await api.startIVnc();
      addToast({ type: "success", message: "iVnc 已启动" });
      await loadStatus();
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
    </div>
  );
}
