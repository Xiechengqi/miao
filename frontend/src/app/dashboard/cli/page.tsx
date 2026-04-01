"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { Cli } from "@/types/api";
import { TerminalLogEntry } from "@/types/api";
import {
  AppWindow,
  Plus,
  Trash2,
  RefreshCw,
  Play,
  Square,
  Pencil,
  Download,
  FileText,
  ExternalLink,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { formatUptime } from "@/lib/utils";
import { ansiToHtml, stripLogPrefix } from "@/lib/ansi";

// ============================================================================
// Log Modal
// ============================================================================

function CliLogModal({
  isOpen,
  onClose,
  cliId,
  cliName,
}: {
  isOpen: boolean;
  onClose: () => void;
  cliId: string;
  cliName: string;
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
      const data = await api.getCliLogs(cliId, 200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load CLI logs:", error);
    } finally {
      setLoading(false);
    }
  }, [cliId]);

  const connectWs = useCallback(() => {
    const token = localStorage.getItem("miao_token");
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const wsUrl = `${protocol}://${window.location.host}/api/clis/${cliId}/ws/logs?token=${token}`;

    const ws = new WebSocket(wsUrl);
    ws.onopen = () => {
      if (!isUnmountedRef.current) setWsConnected(true);
    };
    ws.onmessage = (event) => {
      if (isUnmountedRef.current) return;
      try {
        const entry = JSON.parse(event.data) as TerminalLogEntry;
        setLogs((prev) => [entry, ...prev].slice(0, 200));
      } catch (e) {
        console.error("Failed to parse CLI log:", e);
      }
    };
    ws.onclose = () => {
      if (!isUnmountedRef.current) setWsConnected(false);
    };
    ws.onerror = () => {
      if (!isUnmountedRef.current) setWsConnected(false);
    };
    wsRef.current = ws;
  }, [cliId]);

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
    <Modal isOpen={isOpen} onClose={onClose} title={`日志 - ${cliName}`} size="lg">
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

// ============================================================================
// Main Page
// ============================================================================

const defaultForm = {
  name: "",
  binary_url: "",
  command: "",
  web_url: "",
  skill_md_url: "",
};

export default function CliPage() {
  const { setLoading, loading, addToast, clis, setClis, clisLoaded, setClisLoaded } = useStore();

  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formData, setFormData] = useState(defaultForm);
  const [showLogModal, setShowLogModal] = useState(false);
  const [selectedCliForLog, setSelectedCliForLog] = useState<{ id: string; name: string } | null>(null);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [actionLoadingId, setActionLoadingId] = useState<string | null>(null);

  useEffect(() => {
    loadClis();
  }, []);

  const loadClis = async () => {
    try {
      const data = await api.getClis();
      setClis(data);
      setClisLoaded(true);
    } catch (error) {
      console.error("Failed to load CLIs:", error);
    }
  };

  const handleSubmit = async () => {
    setLoading(true, "save");
    try {
      if (editingId) {
        await api.updateCli(editingId, {
          name: formData.name.trim() || undefined,
          binary_url: formData.binary_url.trim() || undefined,
          command: formData.command.trim() || undefined,
          web_url: formData.web_url.trim() || undefined,
          skill_md_url: formData.skill_md_url.trim() || undefined,
        });
        addToast({ type: "success", message: "CLI 已更新" });
      } else {
        await api.createCli({
          name: formData.name.trim(),
          binary_url: formData.binary_url.trim(),
          command: formData.command.trim() || undefined,
          web_url: formData.web_url.trim() || undefined,
          skill_md_url: formData.skill_md_url.trim() || undefined,
        });
        addToast({ type: "success", message: "CLI 已创建" });
      }
      setShowModal(false);
      setEditingId(null);
      setFormData(defaultForm);
      loadClis();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("确定要删除此 CLI 吗？删除后将同时移除已下载的二进制文件。")) return;
    setLoading(true, "delete");
    try {
      await api.deleteCli(id);
      addToast({ type: "success", message: "CLI 已删除" });
      loadClis();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleInstall = async (id: string) => {
    setActionLoadingId(id);
    try {
      await api.installCli(id);
      addToast({ type: "success", message: "二进制安装成功" });
      loadClis();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "安装失败" });
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleUpdateBinary = async (id: string) => {
    setActionLoadingId(id);
    try {
      await api.updateCliBinary(id);
      addToast({ type: "success", message: "二进制已更新" });
      loadClis();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "更新失败" });
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleToggle = async (id: string, running: boolean) => {
    setActionLoadingId(id);
    try {
      if (running) {
        await api.stopCli(id);
      } else {
        await api.startCli(id);
      }
      loadClis();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleRestart = async (id: string) => {
    setActionLoadingId(id);
    try {
      await api.restartCli(id);
      addToast({ type: "success", message: "CLI 已重启" });
      loadClis();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重启失败" });
    } finally {
      setActionLoadingId(null);
    }
  };

  const toggleExpand = (id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const openModal = (cli?: Cli) => {
    if (cli) {
      setEditingId(cli.id);
      setFormData({
        name: cli.name,
        binary_url: cli.binary_url,
        command: cli.command || "",
        web_url: cli.web_url || "",
        skill_md_url: cli.skill_md_url || "",
      });
    } else {
      setEditingId(null);
      setFormData(defaultForm);
    }
    setShowModal(true);
  };

  const openLogModal = (cli: Cli) => {
    setSelectedCliForLog({ id: cli.id, name: cli.name });
    setShowLogModal(true);
  };

  if (!clisLoaded) {
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
        <div>
          <h1 className="text-3xl font-black">CLI</h1>
          <p className="text-slate-500 mt-1">管理命令行应用</p>
        </div>
        <Button onClick={() => openModal()}>
          <Plus className="w-4 h-4" />
          添加 CLI
        </Button>
      </div>

      {/* Empty State */}
      {clis.length === 0 && (
        <Card className="p-6">
          <div className="text-center py-8">
            <AppWindow className="w-16 h-16 mx-auto text-slate-300 mb-4" />
            <h2 className="text-xl font-bold text-slate-700 mb-2">暂无 CLI 应用</h2>
            <p className="text-slate-500 mb-6">
              点击上方按钮添加你的第一个 CLI 应用
            </p>
          </div>
        </Card>
      )}

      {/* CLI List */}
      <div className="space-y-4">
        {clis.map((cli) => {
          const isExpanded = expandedIds.has(cli.id);
          const hasCommand = !!cli.command;
          const hasWebUrl = !!cli.web_url;
          const isLoading = actionLoadingId === cli.id;

          return (
            <Card key={cli.id} className="overflow-hidden">
              <div className="p-4">
                <div className="flex flex-col sm:flex-row sm:items-center gap-3">
                  {/* Name & Status */}
                  <div className="flex items-center gap-3 flex-1 min-w-0">
                    <div
                      className={`flex items-center gap-2 min-w-0 ${hasWebUrl ? "cursor-pointer" : ""}`}
                      onClick={hasWebUrl ? () => toggleExpand(cli.id) : undefined}
                    >
                      <h3 className="text-lg font-bold truncate">{cli.name}</h3>
                      {hasWebUrl && (
                        isExpanded
                          ? <ChevronUp className="w-4 h-4 text-slate-400 flex-shrink-0" />
                          : <ChevronDown className="w-4 h-4 text-slate-400 flex-shrink-0" />
                      )}
                    </div>

                    {/* Status Badge */}
                    {!cli.installed ? (
                      <Badge variant="warning">未安装</Badge>
                    ) : cli.status.running ? (
                      <Badge variant="success">运行中</Badge>
                    ) : hasCommand ? (
                      <Badge variant="default">已停止</Badge>
                    ) : (
                      <Badge variant="info">已安装</Badge>
                    )}

                    {/* Uptime */}
                    {cli.status.running && cli.status.uptime_secs && (
                      <span className="text-xs text-slate-400">
                        {formatUptime(cli.status.uptime_secs)}
                      </span>
                    )}

                    {/* SKILL.md link */}
                    {cli.skill_md_url && (
                      <a
                        href={cli.skill_md_url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-indigo-500 hover:text-indigo-700"
                        title="SKILL.md"
                      >
                        <ExternalLink className="w-4 h-4" />
                      </a>
                    )}
                  </div>

                  {/* Action Buttons */}
                  <div className="flex items-center gap-1.5 flex-wrap">
                    {/* Install button when not installed */}
                    {!cli.installed && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleInstall(cli.id)}
                        disabled={isLoading}
                      >
                        <Download className="w-4 h-4" />
                        安装
                      </Button>
                    )}

                    {/* Start/Stop button */}
                    {hasCommand && cli.installed && (
                      <Button
                        variant={cli.status.running ? "danger" : "secondary"}
                        size="sm"
                        onClick={() => handleToggle(cli.id, cli.status.running)}
                        disabled={isLoading}
                      >
                        {cli.status.running ? (
                          <>
                            <Square className="w-3.5 h-3.5" />
                            停止
                          </>
                        ) : (
                          <>
                            <Play className="w-3.5 h-3.5" />
                            启动
                          </>
                        )}
                      </Button>
                    )}

                    {/* Restart button */}
                    {hasCommand && cli.installed && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleRestart(cli.id)}
                        disabled={isLoading || !cli.status.running}
                      >
                        <RefreshCw className="w-3.5 h-3.5" />
                        重启
                      </Button>
                    )}

                    {/* Log button */}
                    {hasCommand && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => openLogModal(cli)}
                      >
                        <FileText className="w-3.5 h-3.5" />
                        日志
                      </Button>
                    )}

                    {/* Update binary */}
                    {cli.installed && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleUpdateBinary(cli.id)}
                        disabled={isLoading}
                      >
                        <Download className="w-3.5 h-3.5" />
                        更新
                      </Button>
                    )}

                    {/* Edit */}
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => openModal(cli)}
                    >
                      <Pencil className="w-3.5 h-3.5" />
                    </Button>

                    {/* Delete */}
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={() => handleDelete(cli.id)}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </Button>
                  </div>
                </div>

                {/* Command info line */}
                {cli.command && (
                  <div className="mt-2 text-xs text-slate-400 font-mono truncate">
                    {cli.command}
                  </div>
                )}
              </div>

              {/* Expandable iframe area */}
              {hasWebUrl && isExpanded && (
                <div className="border-t border-slate-200">
                  <iframe
                    src={cli.web_url}
                    className="w-full border-0"
                    style={{ height: "500px" }}
                    title={`${cli.name} Web UI`}
                  />
                </div>
              )}
            </Card>
          );
        })}
      </div>

      {/* Add/Edit Modal */}
      <Modal
        isOpen={showModal}
        onClose={() => {
          setShowModal(false);
          setEditingId(null);
          setFormData(defaultForm);
        }}
        title={editingId ? "编辑 CLI" : "添加 CLI"}
      >
        <div className="space-y-4">
          <Input
            label="名称 *"
            placeholder="例如：frp"
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
          />
          <Input
            label="二进制下载链接 *"
            placeholder="https://example.com/binary"
            value={formData.binary_url}
            onChange={(e) => setFormData({ ...formData, binary_url: e.target.value })}
          />
          <Input
            label="启动命令"
            placeholder="例如：./frpc -c config.toml"
            value={formData.command}
            onChange={(e) => setFormData({ ...formData, command: e.target.value })}
          />
          <Input
            label="Web URL"
            placeholder="例如：http://localhost:7500"
            value={formData.web_url}
            onChange={(e) => setFormData({ ...formData, web_url: e.target.value })}
          />
          <Input
            label="SKILL.md 链接"
            placeholder="https://example.com/SKILL.md"
            value={formData.skill_md_url}
            onChange={(e) => setFormData({ ...formData, skill_md_url: e.target.value })}
          />
          <div className="flex gap-3 pt-2">
            <Button
              className="flex-1"
              onClick={handleSubmit}
              disabled={!formData.name.trim() || !formData.binary_url.trim() || loading}
            >
              {loading ? "保存中..." : editingId ? "保存" : "创建"}
            </Button>
            <Button
              variant="secondary"
              onClick={() => {
                setShowModal(false);
                setEditingId(null);
                setFormData(defaultForm);
              }}
            >
              取消
            </Button>
          </div>
        </div>
      </Modal>

      {/* Log Modal */}
      {selectedCliForLog && (
        <CliLogModal
          isOpen={showLogModal}
          onClose={() => {
            setShowLogModal(false);
            setSelectedCliForLog(null);
          }}
          cliId={selectedCliForLog.id}
          cliName={selectedCliForLog.name}
        />
      )}
    </div>
  );
}
