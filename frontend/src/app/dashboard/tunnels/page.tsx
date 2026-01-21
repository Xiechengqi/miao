"use client";

import { useEffect, useMemo, useState } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { Plus, Play, Square, Trash2, RefreshCw, Activity, Copy, Pencil } from "lucide-react";
import { TcpTunnel } from "@/types/api";

const defaultForm = {
  name: "",
  enabled: true,
  mode: "single" as "single" | "full",
  local_addr: "127.0.0.1",
  local_port: "22",
  remote_bind_addr: "127.0.0.1",
  remote_port: "",
  allow_public_bind: false,
  ssh_host: "",
  ssh_port: "22",
  username: "",
  auth_type: "password" as "password" | "private_key_path",
  password: "",
  private_key_path: "",
  private_key_passphrase: "",
  clear_private_key_passphrase: false,
  strict_host_key_checking: false,
  host_key_fingerprint: "",
  connect_timeout_ms: "",
  keepalive_interval_ms: "",
  backoff_base_ms: "",
  backoff_max_ms: "",
  scan_interval_ms: "",
  debounce_ms: "",
  exclude_ports_text: "",
};

export default function TunnelsPage() {
  const {
    setLoading,
    loading,
    addToast,
    tcpTunnels,
    setTcpTunnels,
    tcpTunnelsLoaded,
    setTcpTunnelsLoaded,
    tcpTunnelsSupported,
    setTcpTunnelsSupported,
  } = useStore();

  const [showModal, setShowModal] = useState(false);
  const [editingTunnel, setEditingTunnel] = useState<TcpTunnel | null>(null);
  const [formData, setFormData] = useState(defaultForm);
  const [viewMode, setViewMode] = useState<"single" | "full">("single");

  useEffect(() => {
    loadTunnels();
  }, []);

  const loadTunnels = async () => {
    try {
      const data = await api.getTcpTunnels();
      setTcpTunnels(data);
      setTcpTunnelsLoaded(true);
    } catch (error) {
      console.error("Failed to load tunnels:", error);
      setTcpTunnelsSupported(false);
    }
  };

  const displayedTunnels = useMemo(
    () => tcpTunnels.filter((tunnel) => tunnel.mode === viewMode),
    [tcpTunnels, viewMode]
  );

  const parseNumber = (value: string) => {
    if (!value.trim()) return undefined;
    const parsed = Number(value);
    return Number.isNaN(parsed) ? undefined : parsed;
  };

  const parsePortsText = (value: string) => {
    if (!value.trim()) return [];
    return value
      .split(/[\s,]+/)
      .map((item) => item.trim())
      .filter((item) => item.length > 0)
      .map((item) => Number(item))
      .filter((item) => !Number.isNaN(item));
  };

  const handleSubmit = async () => {
    setLoading(true, "save");
    try {
      const payload: Record<string, unknown> = {
        name: formData.name.trim() || null,
        enabled: formData.enabled,
        mode: formData.mode,
        remote_bind_addr: formData.remote_bind_addr,
        ssh_host: formData.ssh_host.trim(),
        ssh_port: parseInt(formData.ssh_port) || 22,
        username: formData.username.trim(),
        auth_type: formData.auth_type,
        strict_host_key_checking: formData.strict_host_key_checking,
        host_key_fingerprint: formData.host_key_fingerprint.trim() || undefined,
      };

      if (formData.mode === "single") {
        payload.local_addr = formData.local_addr.trim();
        payload.local_port = parseInt(formData.local_port) || 0;
        payload.remote_port = parseNumber(formData.remote_port);
        payload.allow_public_bind = formData.allow_public_bind;
      } else {
        payload.scan_interval_ms = parseNumber(formData.scan_interval_ms);
        payload.debounce_ms = parseNumber(formData.debounce_ms);
        payload.exclude_ports = parsePortsText(formData.exclude_ports_text);
      }

      if (formData.auth_type === "password") {
        if (formData.password.trim()) {
          payload.password = formData.password.trim();
        }
      } else {
        payload.private_key_path = formData.private_key_path.trim();
        if (formData.clear_private_key_passphrase) {
          payload.clear_private_key_passphrase = true;
          payload.private_key_passphrase = "";
        } else if (formData.private_key_passphrase.trim()) {
          payload.private_key_passphrase = formData.private_key_passphrase.trim();
        }
      }

      const connectTimeout = parseNumber(formData.connect_timeout_ms);
      if (connectTimeout !== undefined) payload.connect_timeout_ms = connectTimeout;
      const keepalive = parseNumber(formData.keepalive_interval_ms);
      if (keepalive !== undefined) payload.keepalive_interval_ms = keepalive;
      const backoffBase = parseNumber(formData.backoff_base_ms);
      if (backoffBase !== undefined) payload.backoff_base_ms = backoffBase;
      const backoffMax = parseNumber(formData.backoff_max_ms);
      if (backoffMax !== undefined) payload.backoff_max_ms = backoffMax;

      if (editingTunnel) {
        await api.updateTcpTunnel(editingTunnel.id, payload as Partial<TcpTunnel>);
        addToast({ type: "success", message: "隧道已更新" });
      } else {
        await api.createTcpTunnel(payload as any);
        addToast({ type: "success", message: "隧道已创建" });
      }
      setShowModal(false);
      setEditingTunnel(null);
      setFormData(defaultForm);
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (tunnel: TcpTunnel) => {
    const isEnabled = tunnel.enabled ?? tunnel.status.state === "forwarding";
    setLoading(true, isEnabled ? "stop" : "start");
    try {
      if (isEnabled) {
        await api.stopTcpTunnel(tunnel.id);
      } else {
        await api.startTcpTunnel(tunnel.id);
      }
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "操作失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleRestart = async (tunnel: TcpTunnel) => {
    setLoading(true, "restart");
    try {
      await api.restartTcpTunnel(tunnel.id);
      addToast({ type: "success", message: "隧道已重启" });
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重启失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleTest = async (tunnel: TcpTunnel) => {
    setLoading(true, "test");
    try {
      await api.testTcpTunnel(tunnel.id);
      addToast({ type: "success", message: "测试已触发" });
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "测试失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = async (tunnel: TcpTunnel) => {
    setLoading(true, "copy");
    try {
      await api.copyTcpTunnel(tunnel.id);
      addToast({ type: "success", message: "隧道已复制" });
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "复制失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("确定要删除此隧道吗？")) return;

    setLoading(true, "delete");
    try {
      await api.deleteTcpTunnel(id);
      addToast({ type: "success", message: "隧道已删除" });
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  const openModal = (tunnel?: TcpTunnel) => {
    if (tunnel) {
      setEditingTunnel(tunnel);
      setFormData({
        name: tunnel.name || "",
        enabled: tunnel.enabled ?? true,
        mode: tunnel.mode,
        local_addr: tunnel.local_addr || "127.0.0.1",
        local_port: tunnel.local_port?.toString() || "22",
        remote_bind_addr: tunnel.remote_bind_addr || "127.0.0.1",
        remote_port: tunnel.remote_port?.toString() || "",
        allow_public_bind: !!tunnel.allow_public_bind,
        ssh_host: tunnel.ssh_host || "",
        ssh_port: tunnel.ssh_port?.toString() || "22",
        username: tunnel.username || "",
        auth_type: tunnel.auth_type || "password",
        password: "",
        private_key_path: tunnel.private_key_path || "",
        private_key_passphrase: "",
        clear_private_key_passphrase: false,
        strict_host_key_checking: !!tunnel.strict_host_key_checking,
        host_key_fingerprint: tunnel.host_key_fingerprint || "",
        connect_timeout_ms: tunnel.connect_timeout_ms?.toString() || "",
        keepalive_interval_ms: tunnel.keepalive_interval_ms?.toString() || "",
        backoff_base_ms: tunnel.backoff_base_ms?.toString() || "",
        backoff_max_ms: tunnel.backoff_max_ms?.toString() || "",
        scan_interval_ms: tunnel.scan_interval_ms?.toString() || "",
        debounce_ms: tunnel.debounce_ms?.toString() || "",
        exclude_ports_text: tunnel.exclude_ports?.join(", ") || "",
      });
    } else {
      setEditingTunnel(null);
      setFormData({ ...defaultForm, mode: viewMode });
    }
    setShowModal(true);
  };

  if (!tcpTunnelsLoaded) {
    return (
      <div className="space-y-6">
        <div className="text-center py-12">
          <div className="w-12 h-12 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      </div>
    );
  }

  if (!tcpTunnelsSupported) {
    return (
      <div className="space-y-6">
        <div className="text-center py-12">
          <p className="text-slate-500">当前版本未启用 tcp_tunnel 特性</p>
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
            TCP 穿透
          </h1>
          <p className="text-slate-500 mt-1">管理 SSH 反向隧道</p>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex items-center bg-slate-100 rounded-lg p-1">
            <Button
              variant={viewMode === "single" ? "primary" : "ghost"}
              size="sm"
              onClick={() => setViewMode("single")}
            >
              单穿透
            </Button>
            <Button
              variant={viewMode === "full" ? "primary" : "ghost"}
              size="sm"
              onClick={() => setViewMode("full")}
            >
              全穿透
            </Button>
          </div>
          <Button onClick={() => openModal()}>
            <Plus className="w-4 h-4" />
            添加隧道
          </Button>
        </div>
      </div>

      {/* Tunnel List */}
      <div className="grid gap-4">
        {displayedTunnels.map((tunnel) => {
          const isEnabled = tunnel.enabled ?? tunnel.status.state === "forwarding";

          return (
            <Card key={tunnel.id} className="p-4" hoverEffect>
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div className="flex-1">
                  <div className="flex items-center gap-2 mb-2 flex-wrap">
                    <span className="font-bold">{tunnel.name || tunnel.id}</span>
                    <Badge variant={tunnel.mode === "full" ? "primary" : "info"}>
                      {tunnel.mode === "full" ? "全穿透" : "单穿透"}
                    </Badge>
                    <Badge
                      variant={
                        tunnel.status.state === "forwarding" ? "success" :
                        tunnel.status.state === "connecting" ? "warning" :
                        tunnel.status.state === "error" ? "error" : "default"
                      }
                    >
                      {tunnel.status.state}
                    </Badge>
                    {tunnel.remote_bind_addr === "0.0.0.0" && (
                      <Badge variant="error">公网</Badge>
                    )}
                  </div>
                  <div className="text-sm text-slate-500">
                    {tunnel.mode === "full" ? (
                      <p>远程监听: {tunnel.remote_bind_addr}（同端口映射）</p>
                    ) : (
                      <p>
                        远程监听: {tunnel.remote_bind_addr}:{tunnel.remote_port} → 本地: {tunnel.local_addr}:{tunnel.local_port}
                      </p>
                    )}
                    <p>
                      SSH: {tunnel.username}@{tunnel.ssh_host}:{tunnel.ssh_port}
                      <span className="ml-2">连接: {tunnel.status.active_conns}</span>
                    </p>
                  </div>
                  {tunnel.status.last_error && (
                    <p className="text-sm text-red-500 mt-1">
                      {tunnel.status.last_error.code}: {tunnel.status.last_error.message}
                    </p>
                  )}
                </div>

                <div className="flex gap-2">
                  <Button variant="secondary" size="sm" onClick={() => handleRestart(tunnel)}>
                    <RefreshCw className="w-4 h-4" />
                    重启
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => handleToggle(tunnel)}
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
                  <Button variant="secondary" size="sm" onClick={() => handleTest(tunnel)}>
                    <Activity className="w-4 h-4" />
                    测试
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => openModal(tunnel)}>
                    <Pencil className="w-4 h-4" />
                    编辑
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => handleCopy(tunnel)}>
                    <Copy className="w-4 h-4" />
                    复制
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => handleDelete(tunnel.id)}>
                    <Trash2 className="w-4 h-4 text-red-500" />
                  </Button>
                </div>
              </div>
            </Card>
          );
        })}

        {displayedTunnels.length === 0 && (
          <Card className="p-12 text-center">
            <p className="text-slate-500">暂无 TCP 穿透配置</p>
          </Card>
        )}
      </div>

      {/* Add/Edit Modal */}
      <Modal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
        title={editingTunnel ? "编辑隧道" : "添加隧道"}
        size="lg"
      >
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="名称"
              placeholder="例如: 远程暴露本地 8080"
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
                模式
              </label>
              <select
                value={formData.mode}
                onChange={(e) => setFormData({ ...formData, mode: e.target.value as "single" | "full" })}
                className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
              >
                <option value="single">单穿透</option>
                <option value="full">全穿透</option>
              </select>
            </div>
            <Input
              label="SSH 端口"
              type="number"
              value={formData.ssh_port}
              onChange={(e) => setFormData({ ...formData, ssh_port: e.target.value })}
            />
          </div>

          {formData.mode === "single" ? (
            <div className="grid grid-cols-4 gap-4">
              <Input
                label="本地目标地址"
                placeholder="127.0.0.1"
                value={formData.local_addr}
                onChange={(e) => setFormData({ ...formData, local_addr: e.target.value })}
              />
              <Input
                label="本地端口"
                type="number"
                value={formData.local_port}
                onChange={(e) => setFormData({ ...formData, local_port: e.target.value })}
              />
              <div>
                <label className="block text-sm font-semibold text-slate-700 mb-2">
                  远程监听地址
                </label>
                <select
                  value={formData.remote_bind_addr}
                  onChange={(e) => setFormData({ ...formData, remote_bind_addr: e.target.value })}
                  className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
                >
                  <option value="127.0.0.1">127.0.0.1（仅远程本机）</option>
                  <option value="0.0.0.0">0.0.0.0（公网暴露）</option>
                </select>
              </div>
              <Input
                label="远程端口"
                type="number"
                value={formData.remote_port}
                onChange={(e) => setFormData({ ...formData, remote_port: e.target.value })}
              />
            </div>
          ) : (
            <div className="grid grid-cols-3 gap-4">
              <div>
                <label className="block text-sm font-semibold text-slate-700 mb-2">
                  远程监听地址
                </label>
                <select
                  value={formData.remote_bind_addr}
                  onChange={(e) => setFormData({ ...formData, remote_bind_addr: e.target.value })}
                  className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
                >
                  <option value="127.0.0.1">127.0.0.1（仅远程本机）</option>
                  <option value="0.0.0.0">0.0.0.0（公网暴露）</option>
                </select>
              </div>
              <Input
                label="扫描间隔 (ms)"
                type="number"
                value={formData.scan_interval_ms}
                onChange={(e) => setFormData({ ...formData, scan_interval_ms: e.target.value })}
              />
              <Input
                label="删除防抖 (ms)"
                type="number"
                value={formData.debounce_ms}
                onChange={(e) => setFormData({ ...formData, debounce_ms: e.target.value })}
              />
            </div>
          )}

          {formData.mode === "single" && formData.remote_bind_addr === "0.0.0.0" && (
            <Card className="border border-red-200 bg-red-50 p-4">
              <p className="text-sm text-red-700 font-semibold mb-2">风险提示：公网暴露</p>
              <p className="text-sm text-red-600">
                该配置会把远程服务器端口暴露到公网，请确认远程 sshd 启用 GatewayPorts，并配置防火墙/安全组。
              </p>
              <label className="flex items-center gap-2 text-sm text-red-700 mt-3">
                <input
                  type="checkbox"
                  checked={formData.allow_public_bind}
                  onChange={(e) => setFormData({ ...formData, allow_public_bind: e.target.checked })}
                />
                我理解风险并允许公网监听
              </label>
            </Card>
          )}

          <div className="grid grid-cols-3 gap-4">
            <Input
              label="SSH 主机"
              placeholder="example.com"
              value={formData.ssh_host}
              onChange={(e) => setFormData({ ...formData, ssh_host: e.target.value })}
            />
            <Input
              label="SSH 端口"
              type="number"
              value={formData.ssh_port}
              onChange={(e) => setFormData({ ...formData, ssh_port: e.target.value })}
            />
            <Input
              label="用户名"
              placeholder="root"
              value={formData.username}
              onChange={(e) => setFormData({ ...formData, username: e.target.value })}
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">
                认证方式
              </label>
              <select
                value={formData.auth_type}
                onChange={(e) => setFormData({ ...formData, auth_type: e.target.value as "password" | "private_key_path" })}
                className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
              >
                <option value="password">Password</option>
                <option value="private_key_path">Private Key Path</option>
              </select>
            </div>
            {formData.auth_type === "password" ? (
              <Input
                label="密码"
                placeholder="留空使用 ~/.ssh 密钥"
                value={formData.password}
                onChange={(e) => setFormData({ ...formData, password: e.target.value })}
              />
            ) : (
              <Input
                label="私钥路径"
                placeholder="/root/.ssh/id_ed25519"
                value={formData.private_key_path}
                onChange={(e) => setFormData({ ...formData, private_key_path: e.target.value })}
              />
            )}
          </div>

          {formData.auth_type === "private_key_path" && (
            <div className="space-y-2">
              <Input
                label="私钥口令（可选）"
                type="password"
                placeholder={editingTunnel ? "留空保持不变" : "无口令留空"}
                value={formData.private_key_passphrase}
                onChange={(e) => setFormData({ ...formData, private_key_passphrase: e.target.value })}
              />
              {editingTunnel && (
                <label className="flex items-center gap-2 text-sm text-slate-600">
                  <input
                    type="checkbox"
                    checked={formData.clear_private_key_passphrase}
                    onChange={(e) => setFormData({ ...formData, clear_private_key_passphrase: e.target.checked })}
                  />
                  清空口令
                </label>
              )}
            </div>
          )}

          <div className="grid grid-cols-2 gap-4">
            <div className="flex items-center gap-2 text-sm text-slate-600 pt-7">
              <input
                type="checkbox"
                checked={formData.strict_host_key_checking}
                onChange={(e) => setFormData({ ...formData, strict_host_key_checking: e.target.checked })}
              />
              严格校验主机指纹
            </div>
            <Input
              label="HostKey 指纹（SHA256:...）"
              placeholder="SHA256:xxxx"
              value={formData.host_key_fingerprint}
              onChange={(e) => setFormData({ ...formData, host_key_fingerprint: e.target.value })}
            />
          </div>

          <div className="grid grid-cols-4 gap-4">
            <Input
              label="连接超时 (ms)"
              type="number"
              value={formData.connect_timeout_ms}
              onChange={(e) => setFormData({ ...formData, connect_timeout_ms: e.target.value })}
            />
            <Input
              label="Keepalive (ms)"
              type="number"
              value={formData.keepalive_interval_ms}
              onChange={(e) => setFormData({ ...formData, keepalive_interval_ms: e.target.value })}
            />
            <Input
              label="重连 base (ms)"
              type="number"
              value={formData.backoff_base_ms}
              onChange={(e) => setFormData({ ...formData, backoff_base_ms: e.target.value })}
            />
            <Input
              label="重连 max (ms)"
              type="number"
              value={formData.backoff_max_ms}
              onChange={(e) => setFormData({ ...formData, backoff_max_ms: e.target.value })}
            />
          </div>

          {formData.mode === "full" && (
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">
                不穿透端口黑名单（逗号/空格/换行分隔）
              </label>
              <textarea
                value={formData.exclude_ports_text}
                onChange={(e) => setFormData({ ...formData, exclude_ports_text: e.target.value })}
                rows={3}
                className="w-full px-4 py-3 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
                placeholder="例如: 80, 443, 3000"
              />
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
