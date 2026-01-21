"use client";

import { useEffect, useMemo, useState } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { App, AppTemplate, VncSession } from "@/types/api";
import { formatUptime } from "@/lib/utils";
import { AppWindow, Plus, RefreshCw, Play, Square, Trash2, Pencil } from "lucide-react";

const defaultForm = {
  name: "",
  enabled: true,
  vnc_session_id: "",
  display: "",
  command: "",
};

export default function AppsPage() {
  const { setLoading, loading, addToast } = useStore();
  const [apps, setApps] = useState<App[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formData, setFormData] = useState(defaultForm);
  const [appArgsText, setAppArgsText] = useState("");
  const [appEnvText, setAppEnvText] = useState("");
  const [appRestartOnSave, setAppRestartOnSave] = useState(true);
  const [selectedTemplateId, setSelectedTemplateId] = useState("");
  const [templates, setTemplates] = useState<AppTemplate[]>([]);
  const [vncSessions, setVncSessions] = useState<VncSession[]>([]);

  useEffect(() => {
    loadApps();
    loadTemplates();
    loadVncSessions();
  }, []);

  const loadApps = async () => {
    try {
      const data = await api.getApps();
      setApps(data);
      setLoaded(true);
    } catch (error) {
      console.error("Failed to load apps:", error);
      setLoaded(true);
    }
  };

  const loadTemplates = async () => {
    try {
      const data = await api.getAppTemplates();
      setTemplates(data.templates || []);
    } catch (error) {
      console.error("Failed to load app templates:", error);
    }
  };

  const loadVncSessions = async () => {
    try {
      const data = await api.getVncSessions();
      setVncSessions(data);
    } catch (error) {
      console.error("Failed to load VNC sessions:", error);
    }
  };

  const vncSessionsMap = useMemo(() => {
    const map = new Map<string, VncSession>();
    vncSessions.forEach((session) => {
      map.set(session.id, session);
    });
    return map;
  }, [vncSessions]);

  const resolveAppDisplay = (app: App) => {
    if (app.vnc_session_id) {
      return vncSessionsMap.get(app.vnc_session_id)?.display || "";
    }
    return app.display || "";
  };

  const resolveAppVncName = (app: App) => {
    if (!app.vnc_session_id) return "";
    const session = vncSessionsMap.get(app.vnc_session_id);
    return session ? session.name || session.id : app.vnc_session_id;
  };

  const splitArgsText = (text: string) => {
    if (!text) return [];
    return text
      .split(/\s+/)
      .map((value) => value.trim())
      .filter((value) => value.length > 0);
  };

  const parseEnvText = (text: string) => {
    const env: Record<string, string> = {};
    if (!text) return env;
    const lines = text.split("\n");
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      const idx = trimmed.indexOf("=");
      if (idx < 0) {
        env[trimmed] = "";
      } else {
        const key = trimmed.slice(0, idx).trim();
        const value = trimmed.slice(idx + 1);
        if (key) env[key] = value;
      }
    }
    return env;
  };

  const formatEnvText = (env?: Record<string, string>) => {
    if (!env) return "";
    return Object.entries(env)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n");
  };

  const applyTemplate = (templateId: string) => {
    if (!templateId) return;
    const tpl = templates.find((item) => item.id === templateId);
    if (!tpl) return;
    if (!formData.name) {
      setFormData({
        ...formData,
        name: tpl.name || "",
        command: tpl.command || "",
      });
    } else {
      setFormData({
        ...formData,
        command: tpl.command || "",
      });
    }
    setAppArgsText((tpl.args || []).join(" "));
    setAppEnvText(formatEnvText(tpl.env));
  };

  const openModal = (app?: App) => {
    if (app) {
      setEditingId(app.id);
      setFormData({
        name: app.name || "",
        enabled: app.enabled ?? true,
        vnc_session_id: app.vnc_session_id || "",
        display: app.display || "",
        command: app.command || "",
      });
      setAppArgsText((app.args || []).join(" "));
      setAppEnvText(formatEnvText(app.env));
      setAppRestartOnSave(true);
      setSelectedTemplateId("");
    } else {
      setEditingId(null);
      setFormData(defaultForm);
      setAppArgsText("");
      setAppEnvText("");
      setAppRestartOnSave(true);
      setSelectedTemplateId("");
    }
    setShowModal(true);
  };

  const handleSubmit = async () => {
    setLoading(true, "save");
    try {
      const vncSessionId = (formData.vnc_session_id || "").trim();
      const payload = {
        name: formData.name.trim() || null,
        enabled: !!formData.enabled,
        vnc_session_id: vncSessionId,
        display: vncSessionId ? "" : formData.display.trim(),
        command: formData.command.trim(),
        args: splitArgsText(appArgsText),
        env: parseEnvText(appEnvText),
        restart: !!editingId && appRestartOnSave && !!formData.enabled,
      } as Partial<App> & { restart?: boolean };

      if (editingId) {
        await api.updateApp(editingId, payload);
        addToast({ type: "success", message: "应用已更新" });
      } else {
        await api.createApp(payload);
        addToast({ type: "success", message: "应用已添加" });
      }
      setShowModal(false);
      setEditingId(null);
      setFormData(defaultForm);
      setAppArgsText("");
      setAppEnvText("");
      setAppRestartOnSave(true);
      setSelectedTemplateId("");
      loadApps();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (app: App) => {
    const isEnabled = app.enabled ?? app.status.running;
    setLoading(true, isEnabled ? "stop" : "start");
    try {
      if (isEnabled) {
        await api.stopApp(app.id);
        addToast({ type: "success", message: "应用已停止" });
      } else {
        await api.startApp(app.id);
        addToast({ type: "success", message: "应用已启动" });
      }
      loadApps();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleRestart = async (app: App) => {
    setLoading(true, "restart");
    try {
      await api.restartApp(app.id);
      addToast({ type: "success", message: "应用已重启" });
      loadApps();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重启失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (app: App) => {
    if (!confirm(`确定删除 ${app.name || app.id}？`)) return;

    setLoading(true, "delete");
    try {
      await api.deleteApp(app.id);
      addToast({ type: "success", message: "应用已删除" });
      loadApps();
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
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-3xl font-black">
            应用
          </h1>
          <p className="text-slate-500 mt-1">管理浏览器和应用</p>
        </div>
        <Button onClick={() => openModal()}>
          <Plus className="w-4 h-4" />
          添加应用
        </Button>
      </div>

      {/* App List */}
      <div className="grid gap-4">
        {apps.map((app) => {
          const isEnabled = app.enabled ?? app.status.running;

          return (
            <Card key={app.id} className="p-4" hoverEffect>
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div className="flex items-start gap-3">
                  <div className="w-10 h-10 rounded-lg bg-sky-600/10 flex items-center justify-center">
                    <AppWindow className="w-5 h-5 text-sky-600" />
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-bold">{app.name || app.id}</span>
                      <Badge variant={app.status.running ? "success" : "default"}>
                        {app.status.running ? "running" : "stopped"}
                      </Badge>
                    </div>
                    <p className="text-sm text-slate-500">
                      DISPLAY: {resolveAppDisplay(app) || "-"}
                      {app.vnc_session_id && (
                        <span> · 绑定: {resolveAppVncName(app)}</span>
                      )}
                    </p>
                    <p className="text-sm text-slate-500">
                      命令: {app.command || "-"} {app.args?.join(" ")}
                    </p>
                    {app.status.running && (
                      <p className="text-sm text-slate-500">
                        PID: {app.status.pid || "-"} · 运行: {formatUptime(app.status.uptime_secs)}
                      </p>
                    )}
                  </div>
                </div>

                <div className="flex gap-2">
                  <Button variant="secondary" size="sm" onClick={() => handleRestart(app)}>
                    <RefreshCw className="w-4 h-4" />
                    重启
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => handleToggle(app)}
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
                  <Button variant="ghost" size="sm" onClick={() => openModal(app)}>
                    <Pencil className="w-4 h-4" />
                    编辑
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => handleDelete(app)}>
                    <Trash2 className="w-4 h-4 text-red-500" />
                  </Button>
                </div>
              </div>
            </Card>
          );
        })}

        {apps.length === 0 && (
          <Card className="p-12 text-center">
            <p className="text-slate-500">暂无应用配置</p>
          </Card>
        )}
      </div>

      {/* Create/Edit Modal */}
      <Modal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
        title={editingId ? "编辑应用" : "添加应用"}
        size="lg"
      >
        <div className="space-y-4">
          <div className="grid grid-cols-3 gap-4">
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">
                模板预设
              </label>
              <select
                value={selectedTemplateId}
                onChange={(e) => {
                  const nextId = e.target.value;
                  setSelectedTemplateId(nextId);
                  applyTemplate(nextId);
                }}
                className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
              >
                <option value="">手动配置</option>
                {templates.map((tpl) => (
                  <option key={tpl.id} value={tpl.id}>
                    {tpl.name}
                  </option>
                ))}
              </select>
            </div>
            <Input
              label="名称"
              placeholder="例如: Chromium"
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

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">
                绑定 VNC 会话
              </label>
              <select
                value={formData.vnc_session_id}
                onChange={(e) => setFormData({ ...formData, vnc_session_id: e.target.value })}
                className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
              >
                <option value="">不绑定（手动 DISPLAY）</option>
                {vncSessions.map((session) => (
                  <option key={session.id} value={session.id}>
                    {session.name || session.id} ({session.display})
                  </option>
                ))}
              </select>
            </div>
            <Input
              label="DISPLAY"
              placeholder=":10"
              value={formData.display}
              onChange={(e) => setFormData({ ...formData, display: e.target.value })}
              disabled={!!formData.vnc_session_id}
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <Input
              label="启动命令"
              placeholder="chromium"
              value={formData.command}
              onChange={(e) => setFormData({ ...formData, command: e.target.value })}
            />
            <Input
              label="命令参数"
              placeholder="--no-sandbox"
              value={appArgsText}
              onChange={(e) => setAppArgsText(e.target.value)}
            />
          </div>

          <div>
            <label className="block text-sm font-semibold text-slate-700 mb-2">
              环境变量 (KEY=VALUE，每行一个)
            </label>
            <textarea
              value={appEnvText}
              onChange={(e) => setAppEnvText(e.target.value)}
              rows={4}
              className="w-full px-4 py-3 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
              placeholder="KEY=VALUE"
            />
          </div>

          {editingId && (
            <div className="flex flex-col gap-2 text-sm text-slate-600">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={appRestartOnSave}
                  onChange={(e) => setAppRestartOnSave(e.target.checked)}
                />
                保存后重启
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
    </div>
  );
}
