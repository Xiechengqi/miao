"use client";

import { useEffect, useMemo, useState, useRef, useCallback } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { SyncConfig, Host, SyncLogEntry } from "@/types/api";
import { Plus, Trash2, Pencil, Zap, Play, FileText, Clock, RefreshCw } from "lucide-react";

function LogModal({
  isOpen,
  onClose,
  syncId,
  syncName,
}: {
  isOpen: boolean;
  onClose: () => void;
  syncId: string;
  syncName: string;
}) {
  const [logs, setLogs] = useState<SyncLogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [wsConnected, setWsConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const isUnmountedRef = useRef(false);

  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString("zh-CN", { hour12: false });
  };

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getSyncLogs(syncId, 100);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load logs:", error);
    } finally {
      setLoading(false);
    }
  }, [syncId]);

  const connectWs = useCallback(() => {
    const token = localStorage.getItem("miao_token");
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const wsUrl = `${protocol}://${window.location.host}/api/syncs/${syncId}/ws/logs?token=${token}`;

    const ws = new WebSocket(wsUrl);
    ws.onopen = () => {
      if (!isUnmountedRef.current) {
        setWsConnected(true);
      }
    };
    ws.onmessage = (event) => {
      if (isUnmountedRef.current) return;
      try {
        const entry = JSON.parse(event.data) as SyncLogEntry;
        setLogs((prev) => [...prev.slice(-99), entry]);
      } catch (e) {
        console.error("Failed to parse log:", e);
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
  }, [syncId]);

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

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  return (
    <Modal isOpen={isOpen} onClose={onClose} title={`日志 - ${syncName}`} size="lg">
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

        <div className="max-h-96 overflow-y-auto bg-slate-900 rounded-lg p-4 font-mono text-sm">
          {logs.length === 0 ? (
            <div className="text-slate-500 text-center py-8">暂无日志</div>
          ) : (
            <div className="space-y-1">
              {logs.map((log, index) => (
                <div key={index} className="flex gap-2">
                  <span className="text-slate-400 shrink-0">{formatTime(log.timestamp)}</span>
                  <span
                    className={`shrink-0 ${
                      log.level === "error"
                        ? "text-red-400"
                        : log.level === "warning"
                          ? "text-yellow-400"
                          : "text-green-400"
                    }`}
                  >
                    [{log.level.toUpperCase()}]
                  </span>
                  <span className="text-slate-200">{log.message}</span>
                </div>
              ))}
              <div ref={logsEndRef} />
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
}

const defaultSyncForm = {
  name: "",
  enabled: true,
  host_id: "",
  local_paths_text: "",
  remote_path: "",
  delete: false,
  exclude_text: "",
  include_text: "",
  compression_level: "3",
  compression_threads: "0",
  incremental: false,
  preserve_permissions: false,
  follow_symlinks: false,
  schedule_enabled: false,
  schedule_cron: "",
  schedule_timezone: "Asia/Shanghai",
};

export default function SyncPage() {
  const { setLoading, loading, addToast } = useStore();

  const [syncs, setSyncs] = useState<SyncConfig[]>([]);
  const [syncsLoaded, setSyncsLoaded] = useState(false);
  const [hosts, setHosts] = useState<Host[]>([]);
  // 所有主机都可以用于备份，认证方式由后端处理
  const availableHosts = useMemo(() => hosts, [hosts]);

  const [showSyncModal, setShowSyncModal] = useState(false);
  const [editingSyncId, setEditingSyncId] = useState<string | null>(null);
  const [syncForm, setSyncForm] = useState(defaultSyncForm);
  const [showLogModal, setShowLogModal] = useState(false);
  const [selectedSyncForLog, setSelectedSyncForLog] = useState<{ id: string; name: string } | null>(null);

  const localPaths = useMemo(() => {
    return syncForm.local_paths_text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
  }, [syncForm.local_paths_text]);

  const canSubmitSync = useMemo(() => {
    if (!syncForm.host_id) return false;
    if (localPaths.length === 0) return false;
    if (syncForm.schedule_enabled && !syncForm.schedule_cron.trim()) return false;
    return true;
  }, [syncForm, localPaths]);

  useEffect(() => {
    loadSyncs();
    loadHosts();
  }, []);

  const loadHosts = async () => {
    try {
      const data = await api.getHosts();
      setHosts(data);
    } catch (error) {
      console.error("Failed to load hosts:", error);
    }
  };

  const loadSyncs = async () => {
    try {
      const data = await api.getSyncs();
      setSyncs(data);
    } catch (error) {
      addToast({ type: "error", message: "获取备份配置失败" });
    } finally {
      setSyncsLoaded(true);
    }
  };

  const resetSyncForm = () => {
    setSyncForm(defaultSyncForm);
  };

  const openSyncModal = (sync?: SyncConfig) => {
    void loadHosts();
    if (sync) {
      const matchedHost = availableHosts.find(
        (host) =>
          host.host === sync.ssh.host &&
          host.port === sync.ssh.port &&
          host.username === sync.ssh.username
      );
      setEditingSyncId(sync.id);
      setSyncForm({
        name: sync.name || "",
        enabled: sync.enabled,
        host_id: matchedHost?.id || "",
        local_paths_text: (sync.local_paths || []).join("\n"),
        remote_path: sync.remote_path || "",
        delete: !!sync.options?.delete,
        exclude_text: (sync.options?.exclude || []).join("\n"),
        include_text: (sync.options?.include || []).join("\n"),
        compression_level: (sync.options?.compression_level ?? 3).toString(),
        compression_threads: (sync.options?.compression_threads ?? 0).toString(),
        incremental: !!sync.options?.incremental,
        preserve_permissions: !!sync.options?.preserve_permissions,
        follow_symlinks: !!sync.options?.follow_symlinks,
        schedule_enabled: !!sync.schedule?.enabled,
        schedule_cron: sync.schedule?.cron || "",
        schedule_timezone: sync.schedule?.timezone || "Asia/Shanghai",
      });
    } else {
      setEditingSyncId(null);
      resetSyncForm();
    }
    setShowSyncModal(true);
  };

  const closeSyncModal = () => {
    setShowSyncModal(false);
    setEditingSyncId(null);
    resetSyncForm();
  };

  const handleSubmitSync = async () => {
    if (!canSubmitSync) return;
    setLoading(true, "sync-save");
    try {
      const remotePath = localPaths.length > 1
        ? null
        : (syncForm.remote_path || "").trim() || null;
      const cron = syncForm.schedule_cron.trim();
      const timezone = syncForm.schedule_timezone.trim() || "Asia/Shanghai";
      const schedule = syncForm.schedule_enabled
        ? { enabled: true, cron, timezone }
        : null;

      const selectedHost = availableHosts.find((host) => host.id === syncForm.host_id);
      if (!selectedHost) {
        addToast({ type: "error", message: "所选主机不可用" });
        return;
      }

      // 根据主机认证类型构建 auth 对象
      let auth: Record<string, unknown>;
      if (selectedHost.auth_type === "private_key_path") {
        auth = {
          type: "private_key_path",
          path: selectedHost.private_key_path || "",
          passphrase: selectedHost.private_key_passphrase || null,
        };
      } else {
        // password 类型
        auth = {
          type: "password",
          password: "",
        };
      }

      const payload: Record<string, unknown> = {
        name: syncForm.name.trim() || null,
        enabled: !!syncForm.enabled,
        host_id: selectedHost.id,
        local_paths: localPaths,
        remote_path: remotePath,
        ssh_host: selectedHost.host,
        ssh_port: selectedHost.port,
        username: selectedHost.username,
        auth,
        options: {
          delete: !!syncForm.delete,
          exclude: syncForm.exclude_text
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter(Boolean),
          include: syncForm.include_text
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter(Boolean),
          compression_level: Number(syncForm.compression_level) || 3,
          compression_threads: Number(syncForm.compression_threads) || 0,
          incremental: !!syncForm.incremental,
          preserve_permissions: !!syncForm.preserve_permissions,
          follow_symlinks: !!syncForm.follow_symlinks,
        },
        schedule,
      };

      if (editingSyncId) {
        await api.updateSync(editingSyncId, payload);
        addToast({ type: "success", message: "备份配置已更新" });
      } else {
        await api.createSync(payload);
        addToast({ type: "success", message: "备份配置已添加" });
      }
      closeSyncModal();
      loadSyncs();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "保存失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleToggleSync = async (sync: SyncConfig) => {
    setLoading(true, "sync-toggle");
    try {
      const isScheduled = !!sync.schedule?.enabled;
      const isRunning = sync.status?.state === "running";
      const shouldStop = isScheduled ? sync.enabled : isRunning;
      if (shouldStop) {
        await api.stopSync(sync.id);
      } else {
        await api.startSync(sync.id);
      }
      addToast({ type: "success", message: shouldStop ? "备份已停止" : "备份已启动" });
      loadSyncs();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleTestSync = async (sync: SyncConfig) => {
    setLoading(true, "sync-test");
    try {
      await api.testSync(sync.id);
      addToast({ type: "success", message: "连接测试成功" });
      loadSyncs();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "测试失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleDeleteSync = async (sync: SyncConfig) => {
    if (!confirm(`确定要删除 ${sync.name || sync.id} 吗？`)) return;
    setLoading(true, "sync-delete");
    try {
      await api.deleteSync(sync.id);
      addToast({ type: "success", message: "备份配置已删除" });
      loadSyncs();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleRunSync = async (sync: SyncConfig) => {
    if (!confirm(`确定要立即运行备份 "${sync.name || sync.id}" 吗？`)) return;
    setLoading(true, "sync-run");
    try {
      await api.runSync(sync.id);
      addToast({ type: "success", message: "备份已启动" });

      // Use ref to track the poll interval
      const pollIntervalRef = { current: null as NodeJS.Timeout | null };

      const poll = async () => {
        await loadSyncs();
        // Check if still running
        const updatedSync = syncs.find((s) => s.id === sync.id);
        if (updatedSync?.status?.state !== "running") {
          if (pollIntervalRef.current) {
            clearInterval(pollIntervalRef.current);
            pollIntervalRef.current = null;
          }
        }
      };

      // Start polling
      pollIntervalRef.current = setInterval(poll, 2000);

      // Stop polling after 30 seconds
      setTimeout(() => {
        if (pollIntervalRef.current) {
          clearInterval(pollIntervalRef.current);
          pollIntervalRef.current = null;
        }
      }, 30000);
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "运行失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleToggleSchedule = async (sync: SyncConfig) => {
    setLoading(true, "sync-toggle-schedule");
    try {
      const result = await api.toggleScheduleSync(sync.id);
      addToast({
        type: "success",
        message: result.enabled ? "定时已启用" : "定时已禁用",
      });
      loadSyncs();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const openLogModal = (sync: SyncConfig) => {
    setSelectedSyncForLog({ id: sync.id, name: sync.name || sync.id });
    setShowLogModal(true);
  };

  const formatSyncPaths = (paths: string[]) => {
    if (!paths.length) return "-";
    return paths.join(", ");
  };

  return (
    <div className="space-y-8">
      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-2xl font-bold text-slate-900">备份同步</h2>
            <p className="text-slate-500 text-sm mt-1">高性能流式备份同步</p>
          </div>
          <Button onClick={() => openSyncModal()}>
            <Plus className="w-4 h-4" />
            添加备份
          </Button>
        </div>

        <div className="mt-6">
          {!syncsLoaded ? (
            <div className="text-center py-10">
              <div className="w-10 h-10 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
              <p className="mt-4 text-slate-500">加载中...</p>
            </div>
          ) : syncs.length === 0 ? (
            <div className="text-center py-8 text-slate-500">暂无备份配置</div>
          ) : (
            <div className="space-y-3">
              {syncs.map((sync) => (
                <div
                  key={sync.id}
                  className="flex flex-col gap-3 rounded-lg border border-slate-100 bg-white px-4 py-4 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div>
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-semibold text-slate-900">{sync.name || sync.id}</span>
                      <Badge
                        variant={
                          sync.status?.state === "running"
                            ? "success"
                            : sync.status?.state === "error"
                              ? "error"
                              : "default"
                        }
                      >
                        {sync.status?.state || "idle"}
                      </Badge>
                      {sync.schedule?.enabled && (
                        <Badge variant="warning">cron</Badge>
                      )}
                    </div>
                    <div className="text-sm text-slate-500 mt-1">
                      本地: {formatSyncPaths(sync.local_paths)}
                    </div>
                    <div className="text-sm text-slate-500 mt-1">
                      远程: {sync.ssh.username}@{sync.ssh.host}:{sync.ssh.port}
                    </div>
                    <div className="text-sm text-slate-500 mt-1">
                      远程路径: {sync.remote_path || "跟随本地"}
                    </div>
                    {sync.schedule?.cron && (
                      <div className="text-sm text-slate-500 mt-1">
                        定时: {sync.schedule.cron} ({sync.schedule.timezone || "Asia/Shanghai"})
                      </div>
                    )}
                    {sync.status?.last_error && (
                      <div className="text-sm text-red-500 mt-1">
                        {sync.status.last_error.message}
                      </div>
                    )}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleTestSync(sync)}
                      loading={loading}
                    >
                      <Zap className="w-4 h-4" />
                      测试
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleRunSync(sync)}
                      loading={loading}
                      disabled={sync.status?.state === "running"}
                    >
                      <Play className="w-4 h-4" />
                      运行
                    </Button>
                    {sync.schedule?.cron && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleToggleSchedule(sync)}
                        loading={loading}
                      >
                        <Clock className="w-4 h-4" />
                        {sync.enabled ? "定时停止" : "定时启动"}
                      </Button>
                    )}
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => openLogModal(sync)}
                    >
                      <FileText className="w-4 h-4" />
                      日志
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => openSyncModal(sync)}>
                      <Pencil className="w-4 h-4" />
                      编辑
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => handleDeleteSync(sync)}>
                      <Trash2 className="w-4 h-4 text-red-500" />
                      删除
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </Card>

      <Modal
        isOpen={showSyncModal}
        onClose={closeSyncModal}
        title={editingSyncId ? "编辑备份" : "添加备份"}
        size="lg"
      >
        <div className="space-y-5">
          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            <Input
              label="名称（可选）"
              placeholder="例如: 备份主目录"
              value={syncForm.name}
              onChange={(e) => setSyncForm({ ...syncForm, name: e.target.value })}
            />
            <div className="flex items-end">
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={syncForm.enabled}
                  onChange={(e) => setSyncForm({ ...syncForm, enabled: e.target.checked })}
                />
                {syncForm.enabled ? "已启用" : "未启用"}
              </label>
            </div>
          </div>

          <div>
            <label className="block text-sm font-semibold text-slate-700 mb-2">本地路径（多行）</label>
            <textarea
              className="w-full min-h-[120px] rounded-lg border border-slate-200 px-4 py-3 text-slate-900 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
              value={syncForm.local_paths_text}
              onChange={(e) => setSyncForm({ ...syncForm, local_paths_text: e.target.value })}
              placeholder="/data&#10;/var/log/app.log"
            />
            <p className="text-xs text-slate-500 mt-2">
              {localPaths.length} 项 · 多路径时远程路径固定为本地路径
            </p>
          </div>

          <Input
            label="远程路径（可选）"
            placeholder="留空则与本地相同"
            value={syncForm.remote_path}
            disabled={localPaths.length > 1}
            onChange={(e) => setSyncForm({ ...syncForm, remote_path: e.target.value })}
          />

          <div>
            <label className="block text-sm font-semibold text-slate-700 mb-2">
              选择主机
            </label>
            <select
              value={syncForm.host_id}
              onChange={(e) => setSyncForm({ ...syncForm, host_id: e.target.value })}
              className="w-full h-11 rounded-lg border border-slate-200 bg-white px-4 text-slate-900 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
            >
              <option value="" disabled>
                请选择主机
              </option>
              {hosts.map((host) => {
                const authLabels: Record<string, string> = {
                  password: "密码",
                  private_key_path: "私钥",
                };
                const suffix = `（${authLabels[host.auth_type] || host.auth_type}）`;
                return (
                  <option key={host.id} value={host.id}>
                    {host.name || host.host} ({host.username}@{host.host}:{host.port}){suffix}
                  </option>
                );
              })}
            </select>
            {hosts.length === 0 && (
              <p className="text-xs text-slate-500 mt-2">请先添加 SSH 主机</p>
            )}
          </div>

          <p className="text-xs text-slate-500">
            远端需要 tar 和 zstd（首次同步时自动安装）；支持密码或私钥文件认证。
          </p>

          <div className="space-y-3">
            <p className="text-sm font-semibold text-slate-700">同步选项</p>
            <div className="flex flex-wrap gap-4 text-sm text-slate-600">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={syncForm.delete}
                  onChange={(e) => setSyncForm({ ...syncForm, delete: e.target.checked })}
                />
                删除远端多余文件
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={syncForm.incremental}
                  onChange={(e) => setSyncForm({ ...syncForm, incremental: e.target.checked })}
                />
                增量备份
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={syncForm.preserve_permissions}
                  onChange={(e) => setSyncForm({ ...syncForm, preserve_permissions: e.target.checked })}
                />
                保留权限
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={syncForm.follow_symlinks}
                  onChange={(e) => setSyncForm({ ...syncForm, follow_symlinks: e.target.checked })}
                />
                跟随符号链接
              </label>
            </div>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <Input
              label="压缩级别 (1-22)"
              type="number"
              placeholder="3"
              value={syncForm.compression_level}
              onChange={(e) => setSyncForm({ ...syncForm, compression_level: e.target.value })}
            />
            <Input
              label="压缩线程数 (0=自动)"
              type="number"
              placeholder="0"
              value={syncForm.compression_threads}
              onChange={(e) => setSyncForm({ ...syncForm, compression_threads: e.target.value })}
            />
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">排除规则（多行）</label>
              <textarea
                className="w-full min-h-[120px] rounded-lg border border-slate-200 px-4 py-3 text-slate-900 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
                value={syncForm.exclude_text}
                onChange={(e) => setSyncForm({ ...syncForm, exclude_text: e.target.value })}
                placeholder="*.log&#10;node_modules/"
              />
            </div>
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">包含规则（多行）</label>
              <textarea
                className="w-full min-h-[120px] rounded-lg border border-slate-200 px-4 py-3 text-slate-900 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
                value={syncForm.include_text}
                onChange={(e) => setSyncForm({ ...syncForm, include_text: e.target.value })}
                placeholder="important.txt"
              />
            </div>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            <div className="flex items-end">
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={syncForm.schedule_enabled}
                  onChange={(e) => setSyncForm({ ...syncForm, schedule_enabled: e.target.checked })}
                />
                {syncForm.schedule_enabled ? "定时已启用" : "定时未启用"}
              </label>
            </div>
            <Input
              label="Cron 表达式"
              placeholder="0 2 * * *"
              value={syncForm.schedule_cron}
              onChange={(e) => setSyncForm({ ...syncForm, schedule_cron: e.target.value })}
            />
            <Input
              label="时区"
              placeholder="Asia/Shanghai"
              value={syncForm.schedule_timezone}
              onChange={(e) => setSyncForm({ ...syncForm, schedule_timezone: e.target.value })}
            />
          </div>

          <div className="flex flex-wrap justify-end gap-3 pt-2">
            <Button variant="secondary" onClick={closeSyncModal}>
              取消
            </Button>
            <Button onClick={handleSubmitSync} loading={loading} disabled={!canSubmitSync}>
              保存配置
            </Button>
          </div>
        </div>
      </Modal>

      {selectedSyncForLog && (
        <LogModal
          isOpen={showLogModal}
          onClose={() => {
            setShowLogModal(false);
            setSelectedSyncForLog(null);
          }}
          syncId={selectedSyncForLog.id}
          syncName={selectedSyncForLog.name}
        />
      )}
    </div>
  );
}
