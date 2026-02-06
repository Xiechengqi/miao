"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { Terminal as TerminalIcon, Plus, ExternalLink, Trash2, RefreshCw, Play, Square, Pencil, Download, FileText } from "lucide-react";
import { formatUptime, cn } from "@/lib/utils";
import { TerminalLogEntry } from "@/types/api";
import { ansiToHtml, stripLogPrefix } from "@/lib/ansi";

// 升级日志类型
type UpgradeLogEntry = {
  step: number;
  total_steps: number;
  message: string;
  level: string;
  progress?: number;
};

function TerminalLogModal({
  isOpen,
  onClose,
  terminalId,
  terminalName,
}: {
  isOpen: boolean;
  onClose: () => void;
  terminalId: string;
  terminalName: string;
}) {
  const [logs, setLogs] = useState<TerminalLogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [wsConnected, setWsConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const logsContainerRef = useRef<HTMLDivElement>(null);
  const isUnmountedRef = useRef(false);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getTerminalLogs(terminalId, 200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load terminal logs:", error);
    } finally {
      setLoading(false);
    }
  }, [terminalId]);

  const connectWs = useCallback(() => {
    const token = localStorage.getItem("miao_token");
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const wsUrl = `${protocol}://${window.location.host}/api/terminals/${terminalId}/ws/logs?token=${token}`;

    const ws = new WebSocket(wsUrl);
    ws.onopen = () => {
      if (!isUnmountedRef.current) {
        setWsConnected(true);
      }
    };
    ws.onmessage = (event) => {
      if (isUnmountedRef.current) return;
      try {
        const entry = JSON.parse(event.data) as TerminalLogEntry;
        setLogs((prev) => [entry, ...prev].slice(0, 200));
      } catch (e) {
        console.error("Failed to parse terminal log:", e);
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
  }, [terminalId]);

  const disconnectWs = useCallback(() => {
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    setWsConnected(false);
  }, []);

  useEffect(() => {
    isUnmountedRef.current = false;
    if (isOpen) {
      loadLogs();
      connectWs();
    } else {
      disconnectWs();
    }
    return () => {
      isUnmountedRef.current = true;
      disconnectWs();
    };
  }, [isOpen, loadLogs, connectWs, disconnectWs]);

  useEffect(() => {
    if (logsContainerRef.current) {
      logsContainerRef.current.scrollTop = 0;
    }
  }, [logs]);

  return (
    <Modal isOpen={isOpen} onClose={onClose} title={`日志 - ${terminalName}`} size="lg">
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className={`w-2 h-2 rounded-full ${wsConnected ? "bg-green-500" : "bg-red-500"}`} />
            <span className="text-sm text-slate-500">
              {wsConnected ? "实时连接中" : "连接已断开"}
            </span>
          </div>
          <Button variant="secondary" size="sm" onClick={loadLogs} loading={loading}>
            <RefreshCw className="w-4 h-4" />
            刷新
          </Button>
        </div>

        <div
          ref={logsContainerRef}
          className="max-h-96 overflow-y-auto bg-slate-900 rounded-lg p-4 font-mono text-sm"
        >
          {logs.length === 0 ? (
            <div className="text-slate-500 text-center py-8">暂无日志</div>
          ) : (
            <div className="space-y-1">
              {logs.map((log, index) => (
                <div
                  key={index}
                  className="whitespace-pre-wrap break-all text-slate-200"
                  dangerouslySetInnerHTML={{ __html: ansiToHtml(stripLogPrefix(log.message)) }}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
}

const defaultForm = {
  name: "",
  enabled: true,
  addr: "0.0.0.0",
  port: "7681",
  command: "/bin/bash",
  command_args_text: "-l",
  auth_username: "",
  auth_password: "",
  extra_args_text: "",
  restart_on_save: false,
  clear_auth: false,
};

export default function TerminalsPage() {
  const {
    setLoading,
    loading,
    addToast,
    terminals,
    setTerminals,
    terminalsLoaded,
    setTerminalsLoaded,
  } = useStore();

  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formData, setFormData] = useState(defaultForm);
  const [showLogModal, setShowLogModal] = useState(false);
  const [selectedTerminalForLog, setSelectedTerminalForLog] = useState<{ id: string; name: string } | null>(null);

  // gotty 安装状态
  const [gottyInstalled, setGottyInstalled] = useState<boolean | null>(null);
  const [installingGotty, setInstallingGotty] = useState(false);

  // gotty 升级状态
  const [showUpgradeModal, setShowUpgradeModal] = useState(false);
  const [upgrading, setUpgrading] = useState(false);
  const [upgradeLogs, setUpgradeLogs] = useState<UpgradeLogEntry[]>([]);
  const [upgradeProgress, setUpgradeProgress] = useState(0);
  const [upgradeStatus, setUpgradeStatus] = useState<"running" | "success" | "error">("running");
  const upgradeLogsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    checkGottyAndLoad();
  }, []);

  const checkGottyAndLoad = async () => {
    try {
      const binStatus = await api.getBinariesStatus();
      setGottyInstalled(binStatus.gotty.installed);
      if (binStatus.gotty.installed) {
        await loadTerminals();
      }
    } catch (error) {
      console.error("Failed to check gotty status:", error);
    }
  };

  const loadTerminals = async () => {
    try {
      const data = await api.getTerminals();
      setTerminals(data);
      setTerminalsLoaded(true);
    } catch (error) {
      console.error("Failed to load terminals:", error);
    }
  };

  const handleSubmit = async () => {
    setLoading(true, "save");
    try {
      const payload = {
        name: formData.name.trim() || null,
        enabled: formData.enabled,
        addr: formData.addr.trim(),
        port: parseInt(formData.port),
        command: formData.command.trim(),
        command_args: formData.command_args_text.trim()
          ? formData.command_args_text.trim().split(/\s+/)
          : [],
        auth_username: formData.auth_username.trim() || undefined,
        auth_password: formData.auth_password.trim() || undefined,
        extra_args: formData.extra_args_text.trim()
          ? formData.extra_args_text.trim().split(/\s+/)
          : [],
      };

      if (editingId) {
        const updatePayload = { ...payload } as Record<string, unknown>;
        if (!formData.auth_password && !formData.clear_auth) {
          delete updatePayload.auth_password;
        }
        if (!formData.auth_username && !formData.clear_auth) {
          delete updatePayload.auth_username;
        }
        if (formData.clear_auth) {
          updatePayload.auth_username = "";
          updatePayload.auth_password = "";
        }
        await api.updateTerminal(editingId, updatePayload);
        if (formData.restart_on_save) {
          await api.restartTerminal(editingId);
        }
        addToast({ type: "success", message: "终端已更新" });
      } else {
        await api.createTerminal(payload as any);
        addToast({ type: "success", message: "终端已创建" });
      }
      setShowModal(false);
      setEditingId(null);
      setFormData(defaultForm);
      loadTerminals();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "创建失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("确定要删除此终端配置吗？")) return;

    setLoading(true, "delete");
    try {
      await api.deleteTerminal(id);
      addToast({ type: "success", message: "终端已删除" });
      loadTerminals();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (id: string, running: boolean) => {
    setLoading(true, running ? "stop" : "start");
    try {
      if (running) {
        await api.stopTerminal(id);
      } else {
        await api.startTerminal(id);
      }
      loadTerminals();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleRestart = async (id: string) => {
    setLoading(true, "restart");
    try {
      await api.restartTerminal(id);
      addToast({ type: "success", message: "终端已重启" });
      loadTerminals();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重启失败" });
    } finally {
      setLoading(false);
    }
  };

  const terminalUrl = (terminal: typeof terminals[number]) => {
    const addr = (terminal.addr || "").trim();
    const host = !addr || addr === "0.0.0.0" ? window.location.hostname : addr;
    const port = Number(terminal.port) || 0;
    if (!port) return "";
    const extraArgs = terminal.extra_args || [];
    const hasTls = extraArgs.some((arg) => arg.startsWith("--tls") || arg.startsWith("--tls-cert"));
    const scheme = hasTls ? "https" : "http";
    return `${scheme}://${host}:${port}`;
  };

  const openModal = (terminal?: typeof terminals[number]) => {
    if (terminal) {
      setEditingId(terminal.id);
      setFormData({
        name: terminal.name || "",
        enabled: terminal.enabled ?? true,
        addr: terminal.addr || "0.0.0.0",
        port: terminal.port?.toString() || "7681",
        command: terminal.command || "/bin/bash",
        command_args_text: (terminal.command_args || []).join(" "),
        auth_username: terminal.auth_username || "",
        auth_password: "",
        extra_args_text: (terminal.extra_args || []).join(" "),
        restart_on_save: false,
        clear_auth: false,
      });
    } else {
      setEditingId(null);
      setFormData(defaultForm);
    }
    setShowModal(true);
  };

  const openLogModal = (terminal: typeof terminals[number]) => {
    setSelectedTerminalForLog({ id: terminal.id, name: terminal.name || terminal.id });
    setShowLogModal(true);
  };

  const handleUpgradeGotty = () => {
    if (upgrading) return;

    setUpgrading(true);
    setShowUpgradeModal(true);
    setUpgradeLogs([]);
    setUpgradeProgress(0);
    setUpgradeStatus("running");

    const token = localStorage.getItem("miao_token");
    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${wsProtocol}//${window.location.host}/api/binaries/upgrade/gotty/ws?token=${token}`;

    const ws = new WebSocket(wsUrl);

    ws.onmessage = (event) => {
      try {
        const entry: UpgradeLogEntry = JSON.parse(event.data);
        setUpgradeLogs((prev) => [...prev, entry]);
        setUpgradeProgress(Math.round((entry.step / entry.total_steps) * 100));

        if (entry.level === "error") {
          setUpgradeStatus("error");
        }

        setTimeout(() => {
          if (upgradeLogsRef.current) {
            upgradeLogsRef.current.scrollTop = upgradeLogsRef.current.scrollHeight;
          }
        }, 50);
      } catch {
        // Ignore parse errors
      }
    };

    ws.onclose = () => {
      setUpgradeLogs((prev) => {
        const hasError = prev.some((log) => log.level === "error");
        if (!hasError && prev.length > 0) {
          setUpgradeStatus("success");
        }
        return prev;
      });
      setUpgrading(false);
    };

    ws.onerror = () => {
      setUpgradeStatus("error");
      setUpgradeLogs((prev) => [
        ...prev,
        { step: 0, total_steps: 5, message: "WebSocket 连接失败", level: "error" },
      ]);
      setUpgrading(false);
    };
  };

  const handleInstallGotty = async () => {
    setInstallingGotty(true);
    try {
      await api.installGotty();
      setGottyInstalled(true);
      addToast({ type: "success", message: "gotty 安装成功" });
      await loadTerminals();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "安装失败",
      });
    } finally {
      setInstallingGotty(false);
    }
  };

  // gotty 未安装提示
  if (gottyInstalled === false) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-black">终端</h1>
          <p className="text-slate-500 mt-1">管理 SSH 终端会话</p>
        </div>
        <Card className="p-6">
          <div className="text-center py-8">
            <TerminalIcon className="w-16 h-16 mx-auto text-slate-300 mb-4" />
            <h2 className="text-xl font-bold text-slate-700 mb-2">gotty 未安装</h2>
            <p className="text-slate-500 mb-6">
              当前环境没有 gotty 程序，请点击下方按钮安装
            </p>
            <Button
              onClick={handleInstallGotty}
              disabled={installingGotty}
              className="px-6"
            >
              {installingGotty ? (
                <>
                  <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
                  安装中...
                </>
              ) : (
                <>
                  <Plus className="w-4 h-4 mr-2" />
                  安装 gotty
                </>
              )}
            </Button>
          </div>
        </Card>
      </div>
    );
  }

  if (!terminalsLoaded) {
    return (
      <div className="space-y-6">
        <div className="text-center py-12">
          <div className="w-12 h-12 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-3xl font-black">终端</h1>
          {gottyInstalled && (
            <span className="px-2 py-0.5 text-xs font-medium bg-emerald-100 text-emerald-700 rounded">
              gotty 已安装
            </span>
          )}
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" onClick={handleUpgradeGotty} disabled={upgrading}>
            <Download className="w-4 h-4" />
            {upgrading ? "更新中..." : "更新gotty"}
          </Button>
          <Button onClick={() => openModal()}>
            <Plus className="w-4 h-4" />
            添加终端
          </Button>
        </div>
      </div>

      {/* Terminal List */}
      <div className="grid gap-4">
        {terminals.map((terminal) => (
          <Card key={terminal.id} className="p-4" hoverEffect>
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-indigo-600/10 flex items-center justify-center">
                  <TerminalIcon className="w-5 h-5 text-indigo-600" />
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-bold">{terminal.name || terminal.id}</span>
                    <Badge variant={terminal.status.running ? "success" : "default"}>
                      {terminal.status.running ? "running" : "stopped"}
                    </Badge>
                  </div>
                  <p className="text-sm text-slate-500">地址: {terminal.addr}:{terminal.port}</p>
                  <p className="text-sm text-slate-500">
                    命令: {terminal.command || "-"} {terminal.command_args?.join(" ")}
                  </p>
                  <p className="text-sm text-slate-500">
                    认证: {terminal.auth_username || "-"} / {terminal.auth_password || "-"}
                  </p>
                  {terminal.status.running && (
                    <p className="text-sm text-slate-500">
                      PID: {terminal.status.pid || "-"} · 运行: {formatUptime(terminal.status.uptime_secs)}
                    </p>
                  )}
                </div>
              </div>

              <div className="flex gap-2">
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => {
                    const url = terminalUrl(terminal);
                    if (url) window.open(url, "_blank");
                  }}
                  disabled={!terminalUrl(terminal)}
                >
                  <ExternalLink className="w-4 h-4" />
                  打开终端
                </Button>
                <Button variant="secondary" size="sm" onClick={() => handleRestart(terminal.id)}>
                  <RefreshCw className="w-4 h-4" />
                  重启
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => handleToggle(terminal.id, terminal.status.running)}
                >
                  {terminal.status.running ? (
                    <>
                      <Square className="w-4 h-4" />
                      停止
                    </>
                  ) : (
                    <>
                      <Play className="w-4 h-4" />
                      启动
                    </>
                  )}
                </Button>
                <span
                  className="inline-flex"
                  title={terminal.status.running ? "查看运行日志" : "终端未运行，无法查看日志"}
                >
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => openLogModal(terminal)}
                    disabled={!terminal.status.running}
                  >
                    <FileText className="w-4 h-4" />
                    日志
                  </Button>
                </span>
                <Button variant="ghost" size="sm" onClick={() => openModal(terminal)}>
                  <Pencil className="w-4 h-4" />
                  编辑
                </Button>
                <Button variant="ghost" size="sm" onClick={() => handleDelete(terminal.id)}>
                  <Trash2 className="w-4 h-4 text-red-500" />
                </Button>
              </div>
            </div>
          </Card>
        ))}

        {terminals.length === 0 && (
          <Card className="p-12 text-center">
            <p className="text-slate-500">暂无终端配置</p>
          </Card>
        )}
      </div>

      {/* Add Modal */}
      <Modal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
        title={editingId ? "编辑终端" : "添加终端"}
        size="lg"
      >
        <div className="space-y-4">
          <Input
            label="名称"
            placeholder="My Server"
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
          />
          <div className="flex items-center gap-2 text-sm text-slate-600">
            <input
              type="checkbox"
              checked={formData.enabled}
              onChange={(e) => setFormData({ ...formData, enabled: e.target.checked })}
            />
            {formData.enabled ? "已启用" : "未启用"}
          </div>
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="绑定地址"
              placeholder="127.0.0.1"
              value={formData.addr}
              onChange={(e) => setFormData({ ...formData, addr: e.target.value })}
            />
            <Input
              label="端口"
              type="number"
              value={formData.port}
              onChange={(e) => setFormData({ ...formData, port: e.target.value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="启动命令"
              value={formData.command}
              onChange={(e) => setFormData({ ...formData, command: e.target.value })}
            />
            <Input
              label="命令参数"
              value={formData.command_args_text}
              onChange={(e) => setFormData({ ...formData, command_args_text: e.target.value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="认证用户名"
              placeholder={editingId ? "留空保持不变" : "可选"}
              value={formData.auth_username}
              onChange={(e) => setFormData({ ...formData, auth_username: e.target.value })}
            />
            <Input
              label="认证密码"
              placeholder={editingId ? "留空保持不变" : "可选"}
              value={formData.auth_password}
              onChange={(e) => setFormData({ ...formData, auth_password: e.target.value })}
            />
          </div>
          <Input
            label="额外参数"
            value={formData.extra_args_text}
            onChange={(e) => setFormData({ ...formData, extra_args_text: e.target.value })}
          />
          {editingId && (
            <div className="flex flex-col gap-2 text-sm text-slate-600">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={formData.restart_on_save}
                  onChange={(e) => setFormData({ ...formData, restart_on_save: e.target.checked })}
                />
                保存后重启
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={formData.clear_auth}
                  onChange={(e) => setFormData({ ...formData, clear_auth: e.target.checked })}
                />
                清除认证
              </label>
            </div>
          )}
          <div className="flex justify-end gap-3 pt-4">
            <Button variant="secondary" onClick={() => setShowModal(false)}>
              取消
            </Button>
            <Button onClick={handleSubmit} loading={loading}>
              保存
            </Button>
          </div>
        </div>
      </Modal>

      {/* gotty 升级弹框 */}
      <Modal
        isOpen={showUpgradeModal}
        onClose={() => {
          if (upgradeStatus !== "running") {
            setShowUpgradeModal(false);
          }
        }}
        title="更新gotty"
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
                  upgradeStatus === "success" ? "bg-emerald-500" : "bg-indigo-500"
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
          </div>

          {upgradeStatus !== "running" && (
            <div className="flex justify-end">
              <Button onClick={() => setShowUpgradeModal(false)}>
                关闭
              </Button>
            </div>
          )}
        </div>
      </Modal>

      {selectedTerminalForLog && (
        <TerminalLogModal
          isOpen={showLogModal}
          onClose={() => {
            setShowLogModal(false);
            setSelectedTerminalForLog(null);
          }}
          terminalId={selectedTerminalForLog.id}
          terminalName={selectedTerminalForLog.name}
        />
      )}
    </div>
  );
}
