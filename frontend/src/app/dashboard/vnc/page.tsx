"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api, getVncLogsWsUrl } from "@/lib/api";
import { VncSession, LogEntry } from "@/types/api";
import { formatUptime } from "@/lib/utils";
import { ansiToHtml, stripLogPrefix } from "@/lib/ansi";
import { Monitor, Plus, ExternalLink, Trash2, RefreshCw, Play, Square, Pencil, AlertTriangle, FileText } from "lucide-react";

const defaultForm = {
  name: "",
  enabled: true,
  addr: "0.0.0.0",
  port: "7900",
  display: ":10",
  resolution: "1920x1080",
  depth: "24",
  frame_rate: "24",
  password: "",
  view_only: false,
  restart_on_save: true,
  clear_password: false,
};

function VncLogModal({
  isOpen,
  onClose,
  sessionId,
  sessionName,
}: {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  sessionName: string;
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
      const data = await api.getVncLogs(sessionId, 200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load VNC logs:", error);
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  const connectWs = useCallback(() => {
    let wsUrl: string;
    try {
      wsUrl = getVncLogsWsUrl(sessionId);
    } catch (error) {
      console.warn("Cannot connect to VNC logs WebSocket:", error);
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
        console.error("Failed to parse VNC log:", e);
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
  }, [sessionId]);

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
    <Modal isOpen={isOpen} onClose={onClose} title={`日志 - ${sessionName}`} size="lg">
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

export default function VncPage() {
  const { setLoading, loading, addToast } = useStore();

  const [sessions, setSessions] = useState<VncSession[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formData, setFormData] = useState(defaultForm);
  const [vncAvailable, setVncAvailable] = useState<boolean | null>(null);
  const [openboxAvailable, setOpenboxAvailable] = useState<boolean | null>(null);
  const [showLogModal, setShowLogModal] = useState(false);
  const [selectedSessionForLog, setSelectedSessionForLog] = useState<{ id: string; name: string } | null>(null);

  useEffect(() => {
    loadSessions();
    checkToolsStatus();
  }, []);

  const checkToolsStatus = async () => {
    try {
      const tools = await api.getToolsStatus();
      setVncAvailable(tools.vnc);
      setOpenboxAvailable(tools.openbox);
    } catch (error) {
      console.error("Failed to check tools status:", error);
    }
  };

  const loadSessions = async () => {
    try {
      const data = await api.getVncSessions();
      setSessions(data);
      setLoaded(true);
    } catch (error) {
      console.error("Failed to load VNC sessions:", error);
      setLoaded(true);
    }
  };

  const vncUrl = (session: VncSession) => {
    const addr = (session.addr || "").trim();
    const host = !addr || addr === "0.0.0.0" ? window.location.hostname : addr;
    const port = Number(session.port) || 0;
    if (!port) return "";
    return `http://${host}:${port}`;
  };

  const openModal = (session?: VncSession) => {
    if (session) {
      setEditingId(session.id);
      setFormData({
        name: session.name || "",
        enabled: session.enabled ?? true,
        addr: session.addr || "0.0.0.0",
        port: session.port?.toString() || "7900",
        display: session.display || ":10",
        resolution: session.resolution || "1920x1080",
        depth: session.depth?.toString() || "24",
        frame_rate: session.frame_rate?.toString() || "24",
        password: "",
        view_only: !!session.view_only,
        restart_on_save: true,
        clear_password: false,
      });
    } else {
      setEditingId(null);
      setFormData(defaultForm);
    }
    setShowModal(true);
  };

  const openLogModal = (session: VncSession) => {
    setSelectedSessionForLog({ id: session.id, name: session.name || session.id });
    setShowLogModal(true);
  };

  const handleSubmit = async () => {
    setLoading(true, "save");
    try {
      const payload: Record<string, unknown> = {
        name: formData.name.trim() || null,
        enabled: formData.enabled,
        addr: formData.addr.trim(),
        port: Number(formData.port) || 0,
        display: formData.display.trim(),
        resolution: formData.resolution.trim(),
        depth: Number(formData.depth) || 0,
        frame_rate: Number(formData.frame_rate) || 0,
        view_only: formData.view_only,
        restart: !!editingId && formData.restart_on_save && formData.enabled,
      };
      const password = formData.password.trim();

      if (editingId) {
        if (formData.clear_password) {
          payload.password = "";
        } else if (password) {
          payload.password = password;
        }
        await api.updateVncSession(editingId, payload as Partial<VncSession>);
        addToast({ type: "success", message: "VNC 会话已更新" });
      } else {
        if (password) payload.password = password;
        await api.createVncSession(payload);
        addToast({ type: "success", message: "VNC 会话已添加" });
      }
      setShowModal(false);
      setEditingId(null);
      setFormData(defaultForm);
      loadSessions();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (session: VncSession) => {
    const isEnabled = session.enabled ?? session.status.running;
    setLoading(true, isEnabled ? "stop" : "start");
    try {
      if (isEnabled) {
        await api.stopVncSession(session.id);
        addToast({ type: "success", message: "VNC 会话已停止" });
      } else {
        await api.startVncSession(session.id);
        addToast({ type: "success", message: "VNC 会话已启动" });
      }
      loadSessions();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleRestart = async (session: VncSession) => {
    setLoading(true, "restart");
    try {
      await api.restartVncSession(session.id);
      addToast({ type: "success", message: "VNC 会话已重启" });
      loadSessions();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重启失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("确定要删除此桌面吗？")) return;

    setLoading(true, "delete");
    try {
      await api.deleteVncSession(id);
      addToast({ type: "success", message: "桌面已删除" });
      loadSessions();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  if (!loaded) {
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
      {/* VNC/Openbox Not Available Warning */}
      {(vncAvailable === false || openboxAvailable === false) && (
        <Card className="p-4 bg-amber-50 border-amber-200">
          <div className="flex items-start gap-3">
            <AlertTriangle className="w-5 h-5 text-amber-600 shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold text-amber-800">VNC 服务不可用</p>
              <p className="text-sm text-amber-700 mt-1">
                {vncAvailable === false &&
                  "当前环境未安装 vncserver 或 vncpasswd，请先安装 KasmVNC 或 TigerVNC 后再使用此功能。"}
                {vncAvailable === false && openboxAvailable === false && " "}
                {openboxAvailable === false &&
                  "当前环境未安装 openbox，请先安装后再创建 VNC 桌面。"}
              </p>
            </div>
          </div>
        </Card>
      )}

      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-3xl font-black">远程桌面</h1>
          {vncAvailable && openboxAvailable && (
            <span className="px-2 py-0.5 text-xs font-medium bg-emerald-100 text-emerald-700 rounded">
              VNC 已安装
            </span>
          )}
        </div>
        <Button
          onClick={() => openModal()}
          disabled={vncAvailable === false || openboxAvailable === false}
        >
          <Plus className="w-4 h-4" />
          创建桌面
        </Button>
      </div>

      {/* Session List */}
      <div className="grid gap-4">
        {sessions.map((session) => {
          const isEnabled = session.enabled ?? session.status.running;

          return (
            <Card key={session.id} className="p-4" hoverEffect>
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div className="flex items-start gap-3">
                  <div className="w-10 h-10 rounded-lg bg-violet-600/10 flex items-center justify-center">
                    <Monitor className="w-5 h-5 text-violet-600" />
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-bold">{session.name || session.id}</span>
                      <Badge variant={session.status.running ? "success" : "default"}>
                        {session.status.running ? "running" : "stopped"}
                      </Badge>
                    </div>
                    <p className="text-sm text-slate-500">地址: {session.addr}:{session.port}</p>
                    <p className="text-sm text-slate-500">
                      DISPLAY: {session.display} · 分辨率: {session.resolution} · 深度: {session.depth} · FPS: {session.frame_rate}
                    </p>
                    <p className="text-sm text-slate-500">
                      只读: {session.view_only ? "是" : "否"} · 密码: {session.password || "-"}
                    </p>
                    {session.status.running && (
                      <p className="text-sm text-slate-500">
                        PID: {session.status.pid || "-"} · 运行: {formatUptime(session.status.uptime_secs)}
                      </p>
                    )}
                  </div>
                </div>

                <div className="flex gap-2">
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={() => {
                      const url = vncUrl(session);
                      if (url) window.open(url, "_blank");
                    }}
                    disabled={!vncUrl(session)}
                  >
                    <ExternalLink className="w-4 h-4" />
                    打开桌面
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => handleRestart(session)}>
                    <RefreshCw className="w-4 h-4" />
                    重启
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => handleToggle(session)}
                  >
                    {isEnabled ? (
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
                  <Button variant="secondary" size="sm" onClick={() => openLogModal(session)}>
                    <FileText className="w-4 h-4" />
                    日志
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => openModal(session)}>
                    <Pencil className="w-4 h-4" />
                    编辑
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => handleDelete(session.id)}>
                    <Trash2 className="w-4 h-4 text-red-500" />
                  </Button>
                </div>
              </div>
            </Card>
          );
        })}

        {sessions.length === 0 && (
          <Card className="p-12 text-center">
            <p className="text-slate-500">暂无桌面会话</p>
          </Card>
        )}
      </div>

      {/* Create/Edit Modal */}
      <Modal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
        title={editingId ? "编辑 VNC 会话" : "创建桌面"}
        size="lg"
      >
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="名称"
              placeholder="例如: chromium-vnc"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
            />
            <div className="flex items-center gap-2 text-sm text-slate-600 pt-7">
              <input
                type="checkbox"
                checked={formData.enabled}
                onChange={(e) => setFormData({ ...formData, enabled: e.target.checked })}
              />
              {formData.enabled ? "已启用" : "未启用"}
            </div>
          </div>
          <div className="grid grid-cols-3 gap-4">
            <Input
              label="绑定地址"
              placeholder="0.0.0.0"
              value={formData.addr}
              onChange={(e) => setFormData({ ...formData, addr: e.target.value })}
            />
            <Input
              label="端口"
              type="number"
              value={formData.port}
              onChange={(e) => setFormData({ ...formData, port: e.target.value })}
            />
            <Input
              label="DISPLAY"
              placeholder=":10"
              value={formData.display}
              onChange={(e) => setFormData({ ...formData, display: e.target.value })}
            />
          </div>
          <div className="grid grid-cols-3 gap-4">
            <Input
              label="分辨率"
              placeholder="1920x1080"
              value={formData.resolution}
              onChange={(e) => setFormData({ ...formData, resolution: e.target.value })}
            />
            <Input
              label="深度"
              type="number"
              value={formData.depth}
              onChange={(e) => setFormData({ ...formData, depth: e.target.value })}
            />
            <Input
              label="FPS"
              type="number"
              value={formData.frame_rate}
              onChange={(e) => setFormData({ ...formData, frame_rate: e.target.value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="密码"
              placeholder={editingId ? "留空保持不变" : "留空使用默认密码"}
              value={formData.password}
              onChange={(e) => setFormData({ ...formData, password: e.target.value })}
            />
            <div className="flex items-center gap-2 text-sm text-slate-600 pt-7">
              <input
                type="checkbox"
                checked={formData.view_only}
                onChange={(e) => setFormData({ ...formData, view_only: e.target.checked })}
              />
              {formData.view_only ? "只读" : "可操作"}
            </div>
          </div>
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
                  checked={formData.clear_password}
                  onChange={(e) => setFormData({ ...formData, clear_password: e.target.checked })}
                />
                清除密码
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

      {selectedSessionForLog && (
        <VncLogModal
          isOpen={showLogModal}
          onClose={() => {
            setShowLogModal(false);
            setSelectedSessionForLog(null);
          }}
          sessionId={selectedSessionForLog.id}
          sessionName={selectedSessionForLog.name}
        />
      )}
    </div>
  );
}
