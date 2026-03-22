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
  BookOpen,
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
  const [iframeKey, setIframeKey] = useState(0);
  const [viewMode, setViewMode] = useState<"console" | "desktop">("console");
  const [showDocsModal, setShowDocsModal] = useState(false);

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

    setUpgrading(true);
    setShowUpgradeModal(true);
    setUpgradeLogs([]);
    setUpgradeProgress(0);
    setUpgradeStatus("running");

    // 如果 iVnc 正在运行，先自动停止
    if (status?.running) {
      try {
        await api.stopIVnc();
        // 等待进程真正退出
        await new Promise(resolve => setTimeout(resolve, 1000));
      } catch {
        // 忽略停止错误，继续升级
      }
    }

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
          {status?.running ? (
            <>
              <Button variant="secondary" onClick={handleRestart}>
                <RefreshCw className="w-4 h-4" />
                重启
              </Button>
              <Button variant="secondary" onClick={handleStop}>
                <Square className="w-4 h-4" />
                停止
              </Button>
              <div className="w-px h-8 bg-slate-300" />
            </>
          ) : (
            <>
              <Button onClick={handleStart}>
                <Play className="w-4 h-4" />
                启动
              </Button>
              <div className="w-px h-8 bg-slate-300" />
            </>
          )}
          <Button
            variant="secondary"
            onClick={handleUpgrade}
            loading={upgrading}
          >
            <RefreshCw className="w-4 h-4" />
            更新 iVnc
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
          <Button
            variant="secondary"
            onClick={() => setShowDocsModal(true)}
          >
            <BookOpen className="w-4 h-4" />
            文档
          </Button>
        </div>
      </div>

      {status?.running && (
        <Card className="p-0 overflow-hidden">
          <div className="flex items-center gap-2 p-3 border-b border-slate-200">
            <div className="flex gap-1 bg-slate-100 rounded-lg p-1">
              <button
                onClick={() => setViewMode("console")}
                className={`px-3 py-1 rounded text-sm font-medium transition-colors ${
                  viewMode === "console"
                    ? "bg-white text-slate-900 shadow-sm"
                    : "text-slate-600 hover:text-slate-900"
                }`}
              >
                管理
              </button>
              <button
                onClick={() => setViewMode("desktop")}
                className={`px-3 py-1 rounded text-sm font-medium transition-colors ${
                  viewMode === "desktop"
                    ? "bg-white text-slate-900 shadow-sm"
                    : "text-slate-600 hover:text-slate-900"
                }`}
              >
                桌面
              </button>
            </div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setIframeKey(prev => prev + 1)}
            >
              <RefreshCw className="w-4 h-4" />
            </Button>
            <a
              href={`http://${window.location.hostname}:${status.port}${viewMode === "console" ? "/console" : "/"}`}
              target="_blank"
              rel="noopener noreferrer"
              className="text-sm text-slate-600 hover:text-violet-600 font-mono"
            >
              http://{window.location.hostname}:{status.port}{viewMode === "console" ? "/console" : "/"}
            </a>
          </div>
          <iframe
            key={iframeKey}
            src={`http://${window.location.hostname}:${status.port}${viewMode === "console" ? "/console" : "/"}`}
            className="w-full border-0"
            style={{ height: '70vh' }}
            title="iVNC Web"
          />
        </Card>
      )}

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

      {/* Docs Modal */}
      <Modal
        isOpen={showDocsModal}
        onClose={() => setShowDocsModal(false)}
        title="桌面环境文档"
        size="lg"
      >
        <div className="space-y-6">
          <section>
            <h3 className="text-lg font-bold text-slate-900 mb-3">iVNC 启动依赖</h3>
            <p className="text-sm text-slate-600 mb-3">
              iVNC 运行需要以下系统库，请先安装：
            </p>
            <pre className="bg-slate-900 text-slate-200 rounded-lg p-4 text-sm font-mono overflow-x-auto">
              <code>{`apt-get install \\
  libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 \\
  libpixman-1-0 libxkbcommon0 \\
  gstreamer1.0-tools gstreamer1.0-plugins-base \\
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \\
  gstreamer1.0-plugins-ugly gstreamer1.0-x \\
  libpulse0 libopus0 pulseaudio pulseaudio-utils \\
  libgtk-3-0 libwebkit2gtk-4.1-0 libsoup-3.0-0`}</code>
            </pre>
          </section>

          <section>
            <h3 className="text-lg font-bold text-slate-900 mb-3">安装 Google Chrome 浏览器</h3>
            <div className="rounded-lg bg-red-50 border border-red-200 p-4 mb-4">
              <div className="flex items-start gap-2">
                <AlertTriangle className="w-5 h-5 text-red-500 shrink-0 mt-0.5" />
                <div>
                  <p className="font-semibold text-red-800">注意：apt 和 Snap 安装的 Chrome/Chromium 均无法正常使用</p>
                  <p className="text-sm text-red-700 mt-1">
                    通过 apt 或 Snap 安装的浏览器在 systemd service 环境下存在 cgroup 限制、
                    缺少 DBus session bus 等问题，会导致启动失败。请从 Google 官方下载 deb 包安装。
                  </p>
                </div>
              </div>
            </div>
          </section>

          <section>
            <h3 className="text-base font-semibold text-slate-800 mb-2">Ubuntu 22.04 / 24.04</h3>
            <p className="text-sm text-slate-600 mb-3">
              从 Google 官方下载并安装 Chrome：
            </p>
            <pre className="bg-slate-900 text-slate-200 rounded-lg p-4 text-sm font-mono overflow-x-auto">
              <code>{`wget https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb
apt install ./google-chrome-stable_current_amd64.deb`}</code>
            </pre>
          </section>

          <section>
            <h3 className="text-base font-semibold text-slate-800 mb-2">验证安装</h3>
            <pre className="bg-slate-900 text-slate-200 rounded-lg p-4 text-sm font-mono overflow-x-auto">
              <code>{`google-chrome --version`}</code>
            </pre>
          </section>
        </div>
      </Modal>
    </div>
  );
}
