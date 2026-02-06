"use client";

import { useEffect, useMemo, useState } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { UI_TEXT } from "@/lib/ui-text";
import { Plus, Play, Square, Trash2, RefreshCw, Activity, Copy, Pencil, Eye } from "lucide-react";
import { TcpTunnel, Host } from "@/types/api";

const TUNNEL_TEST_STORAGE_KEY = "miao_tunnel_test_results";

type TunnelTestResult = {
  ok: boolean;
  latency_ms?: number;
};

type TunnelTestCache = {
  results: Record<string, TunnelTestResult>;
};

const defaultForm = {
  name: "",
  enabled: true,
  mode: "single" as "single" | "full",
  host_id: "",
  local_addr: "127.0.0.1",
  local_port: "22",
  remote_bind_addr: "127.0.0.1",
  remote_port: "",
  allow_public_bind: false,
  strict_host_key_checking: false,
  host_key_fingerprint: "",
  connect_timeout_ms: "10000",
  keepalive_interval_ms: "30000",
  backoff_base_ms: "1000",
  backoff_max_ms: "30000",
  scan_interval_ms: "3000",
  debounce_ms: "8000",
  ports_filter_mode: "exclude" as "exclude" | "include",
  include_ports_text: "",
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
  const [hosts, setHosts] = useState<Host[]>([]);
  const [testingTunnelId, setTestingTunnelId] = useState<string | null>(null);
  const [loadErrors, setLoadErrors] = useState<{ single?: string; full?: string }>({});
  const [tunnelTestResults, setTunnelTestResults] = useState<Record<string, TunnelTestResult>>({});
  const [showDetailsModal, setShowDetailsModal] = useState(false);
  const [detailsTunnel, setDetailsTunnel] = useState<TcpTunnel | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [detailsTunnels, setDetailsTunnels] = useState<TcpTunnel[]>([]);
  const availableHosts = useMemo(
    () => hosts,
    [hosts]
  );

  // 初始化加载测试结果
  useEffect(() => {
    try {
      const saved = localStorage.getItem(TUNNEL_TEST_STORAGE_KEY);
      if (saved) {
        const cache: TunnelTestCache = JSON.parse(saved);
        if (cache.results && Object.keys(cache.results).length > 0) {
          setTunnelTestResults(cache.results);
        }
      }
    } catch {
      console.warn("Failed to load tunnel test results from localStorage");
    }
  }, []);

  // 同步测试结果到 localStorage
  useEffect(() => {
    try {
      if (Object.keys(tunnelTestResults).length === 0) {
        localStorage.removeItem(TUNNEL_TEST_STORAGE_KEY);
        return;
      }
      const payload: TunnelTestCache = { results: tunnelTestResults };
      localStorage.setItem(TUNNEL_TEST_STORAGE_KEY, JSON.stringify(payload));
    } catch (error) {
      console.warn("Failed to save tunnel test results:", error);
    }
  }, [tunnelTestResults]);

  useEffect(() => {
    loadTunnels();
    loadHosts();
  }, []);

  const loadHosts = async () => {
    try {
      const data = await api.getHosts();
      setHosts(data);
      return data;
    } catch (error) {
      console.error("Failed to load hosts:", error);
      return [];
    }
  };

  const loadTunnels = async () => {
    try {
      setLoadErrors({});
      const [singleRes, fullRes] = await Promise.allSettled([
        api.getTcpTunnels(),
        api.getTcpTunnelSets(),
      ]);
      const items: TcpTunnel[] = [];
      let supported = true;
      let hasSuccess = false;

      if (singleRes.status === "fulfilled") {
        items.push(...singleRes.value.items);
        supported = supported && singleRes.value.supported;
        hasSuccess = true;
      } else {
        console.error("Failed to load single tunnels:", singleRes.reason);
        setLoadErrors((prev) => ({ ...prev, single: "单穿透加载失败" }));
      }

      if (fullRes.status === "fulfilled") {
        items.push(...fullRes.value.items);
        supported = supported && fullRes.value.supported;
        hasSuccess = true;
      } else {
        console.error("Failed to load full tunnels:", fullRes.reason);
        setLoadErrors((prev) => ({ ...prev, full: "全穿透加载失败" }));
      }

      setTcpTunnels(items);
      setTcpTunnelsSupported(hasSuccess ? supported : false);
    } catch (error) {
      console.error("Failed to load tunnels:", error);
      setTcpTunnelsSupported(false);
    } finally {
      setTcpTunnelsLoaded(true);
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
    if (!formData.host_id) {
      addToast({ type: "error", message: "请先选择主机" });
      return;
    }
    setLoading(true, "save");
    try {
      const selectedHost = availableHosts.find((host) => host.id === formData.host_id);
      if (!selectedHost) {
        addToast({ type: "error", message: "所选主机不可用" });
        return;
      }
      if (selectedHost.auth_type === "private_key_path" && !selectedHost.private_key_path) {
        addToast({ type: "error", message: "所选主机缺少私钥路径" });
        return;
      }

      const payload: Record<string, unknown> = {
        name: formData.name.trim() || null,
        enabled: formData.enabled,
        mode: formData.mode,
        remote_bind_addr: formData.remote_bind_addr,
        strict_host_key_checking: formData.strict_host_key_checking,
        host_key_fingerprint: formData.host_key_fingerprint.trim() || undefined,
        host_id: selectedHost.id,
        ssh_host: selectedHost.host,
        ssh_port: selectedHost.port,
        username: selectedHost.username,
      };

      payload.auth = selectedHost.auth_type === "private_key_path"
        ? {
          type: "private_key_path",
          path: selectedHost.private_key_path || "",
          passphrase: selectedHost.private_key_passphrase || null,
        }
        : { type: "password", password: "" };

      if (formData.mode === "single") {
        payload.local_addr = formData.local_addr.trim();
        payload.local_port = parseInt(formData.local_port) || 0;
        payload.remote_port = parseNumber(formData.remote_port);
        payload.allow_public_bind = formData.allow_public_bind;
      } else {
        payload.scan_interval_ms = parseNumber(formData.scan_interval_ms);
        payload.debounce_ms = parseNumber(formData.debounce_ms);
        const isWhitelist = formData.ports_filter_mode === "include";
        payload.include_ports_enabled = isWhitelist;
        payload.include_ports = isWhitelist ? parsePortsText(formData.include_ports_text) : [];
        payload.exclude_ports = isWhitelist ? [] : parsePortsText(formData.exclude_ports_text);
      }

      const connectTimeout = parseNumber(formData.connect_timeout_ms);
      if (connectTimeout !== undefined) payload.connect_timeout_ms = connectTimeout;
      const keepalive = parseNumber(formData.keepalive_interval_ms);
      if (keepalive !== undefined) payload.keepalive_interval_ms = keepalive;
      const backoffBase = parseNumber(formData.backoff_base_ms);
      const backoffMax = parseNumber(formData.backoff_max_ms);
      if (formData.mode === "single" && (backoffBase !== undefined || backoffMax !== undefined)) {
        payload.reconnect_backoff_ms = {
          base_ms: backoffBase ?? 1000,
          max_ms: backoffMax ?? 30000,
        };
      }

      if (formData.mode === "single") {
        if (editingTunnel) {
          await api.updateTcpTunnel(editingTunnel.id, payload);
          addToast({ type: "success", message: "隧道已更新" });
        } else {
          await api.createTcpTunnel(payload);
          addToast({ type: "success", message: "隧道已创建" });
        }
      } else {
        const setPayload = {
          name: payload.name,
          enabled: payload.enabled,
          remote_bind_addr: payload.remote_bind_addr,
          ssh_host: payload.ssh_host,
          ssh_port: payload.ssh_port,
          username: payload.username,
          auth: payload.auth,
          strict_host_key_checking: payload.strict_host_key_checking,
          host_key_fingerprint: payload.host_key_fingerprint,
          scan_interval_ms: payload.scan_interval_ms,
          debounce_ms: payload.debounce_ms,
          include_ports_enabled: payload.include_ports_enabled,
          include_ports: payload.include_ports,
          exclude_ports: payload.exclude_ports,
          connect_timeout_ms: payload.connect_timeout_ms,
        };
        if (editingTunnel) {
          await api.updateTcpTunnelSet(editingTunnel.id, setPayload);
          addToast({ type: "success", message: "隧道已更新" });
        } else {
          await api.createTcpTunnelSet(setPayload);
          addToast({ type: "success", message: "隧道已创建" });
        }
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
      if (tunnel.mode === "full") {
        if (isEnabled) {
          await api.stopTcpTunnelSet(tunnel.id);
        } else {
          await api.startTcpTunnelSet(tunnel.id);
        }
      } else {
        if (isEnabled) {
          await api.stopTcpTunnel(tunnel.id);
        } else {
          await api.startTcpTunnel(tunnel.id);
        }
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
      if (tunnel.mode === "full") {
        await api.restartTcpTunnelSet(tunnel.id);
      } else {
        await api.restartTcpTunnel(tunnel.id);
      }
      addToast({ type: "success", message: "隧道已重启" });
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重启失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleTest = async (tunnel: TcpTunnel) => {
    setTestingTunnelId(tunnel.id);
    try {
      const result = tunnel.mode === "full"
        ? await api.testTcpTunnelSet(tunnel.id)
        : await api.testTcpTunnel(tunnel.id);
      setTunnelTestResults(prev => ({
        ...prev,
        [tunnel.id]: { ok: result.ok, latency_ms: result.latency_ms }
      }));
      const latencyText = result.latency_ms != null ? ` (${result.latency_ms}ms)` : "";
      addToast({ type: "success", message: `测试成功${latencyText}` });
    } catch (error) {
      setTunnelTestResults(prev => ({
        ...prev,
        [tunnel.id]: { ok: false }
      }));
      addToast({ type: "error", message: error instanceof Error ? error.message : "测试失败" });
    } finally {
      setTestingTunnelId(null);
    }
  };

  const handleCopy = async (tunnel: TcpTunnel) => {
    setLoading(true, "copy");
    try {
      if (tunnel.mode === "full") {
        await api.copyTcpTunnelSet(tunnel.id);
      } else {
        await api.copyTcpTunnel(tunnel.id);
      }
      addToast({ type: "success", message: "隧道已复制" });
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "复制失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleViewDetails = async (tunnel: TcpTunnel) => {
    if (tunnel.mode !== "full") return;
    setDetailsTunnel(tunnel);
    setDetailsLoading(true);
    setShowDetailsModal(true);
    try {
      const res = await api.getTcpTunnelSetTunnels(tunnel.id);
      setDetailsTunnels(res.items);
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "加载详情失败" });
      setDetailsTunnels([]);
    } finally {
      setDetailsLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("确定要删除此隧道吗？")) return;

    setLoading(true, "delete");
    try {
      const tunnel = tcpTunnels.find((t) => t.id === id);
      if (tunnel?.mode === "full") {
        await api.deleteTcpTunnelSet(id);
      } else {
        await api.deleteTcpTunnel(id);
      }
      addToast({ type: "success", message: "隧道已删除" });
      loadTunnels();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  const openModal = async (tunnel?: TcpTunnel) => {
    const hostList = await loadHosts();
    if (tunnel) {
      if (tunnel.mode === "full") {
        try {
          const detail = await api.getTcpTunnelSet(tunnel.id);
          const matchedHost = hostList.find(
            (host) =>
              host.host === detail.ssh_host &&
              host.port === detail.ssh_port &&
              host.username === detail.username
          );
          setEditingTunnel({ ...tunnel, mode: "full" });
          setFormData({
            name: detail.name || "",
            enabled: detail.enabled ?? true,
            mode: "full",
            host_id: matchedHost?.id || "",
            local_addr: "127.0.0.1",
            local_port: "22",
            remote_bind_addr: detail.remote_bind_addr || "127.0.0.1",
            remote_port: "",
            allow_public_bind: detail.remote_bind_addr === "0.0.0.0",
            strict_host_key_checking: !!detail.strict_host_key_checking,
            host_key_fingerprint: detail.host_key_fingerprint || "",
            connect_timeout_ms: detail.connect_timeout_ms?.toString() || "",
            keepalive_interval_ms: "",
            backoff_base_ms: "",
            backoff_max_ms: "",
            scan_interval_ms: detail.scan_interval_ms?.toString() || "",
            debounce_ms: detail.debounce_ms?.toString() || "",
            ports_filter_mode: detail.include_ports_enabled ? "include" : "exclude",
            include_ports_text: detail.include_ports?.join(", ") || "",
            exclude_ports_text: detail.exclude_ports?.join(", ") || "",
          });
        } catch (error) {
          addToast({ type: "error", message: error instanceof Error ? error.message : "加载失败" });
          return;
        }
      } else {
        const matchedHost = hostList.find(
          (host) =>
            host.host === tunnel.ssh_host &&
            host.port === tunnel.ssh_port &&
            host.username === tunnel.username
        );
        setEditingTunnel(tunnel);
        setFormData({
          name: tunnel.name || "",
          enabled: tunnel.enabled ?? true,
          mode: tunnel.mode,
          host_id: matchedHost?.id || "",
          local_addr: tunnel.local_addr || "127.0.0.1",
          local_port: tunnel.local_port?.toString() || "22",
          remote_bind_addr: tunnel.remote_bind_addr || "127.0.0.1",
          remote_port: tunnel.remote_port?.toString() || "",
          allow_public_bind: !!tunnel.allow_public_bind,
          strict_host_key_checking: !!tunnel.strict_host_key_checking,
          host_key_fingerprint: tunnel.host_key_fingerprint || "",
          connect_timeout_ms: tunnel.connect_timeout_ms?.toString() || "",
          keepalive_interval_ms: tunnel.keepalive_interval_ms?.toString() || "",
          backoff_base_ms: tunnel.backoff_base_ms?.toString() || "",
          backoff_max_ms: tunnel.backoff_max_ms?.toString() || "",
          scan_interval_ms: tunnel.scan_interval_ms?.toString() || "",
          debounce_ms: tunnel.debounce_ms?.toString() || "",
          ports_filter_mode: "exclude",
          include_ports_text: "",
          exclude_ports_text: tunnel.exclude_ports?.join(", ") || "",
        });
      }
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
      {(loadErrors.single || loadErrors.full) && (
        <Card className="border border-amber-200 bg-amber-50 p-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm text-amber-700 font-semibold mb-1">部分数据加载失败</p>
              <div className="text-sm text-amber-700">
                {loadErrors.single && <div>{loadErrors.single}</div>}
                {loadErrors.full && <div>{loadErrors.full}</div>}
              </div>
            </div>
            <Button variant="secondary" size="sm" onClick={() => void loadTunnels()}>
              重试
            </Button>
          </div>
        </Card>
      )}
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
          <Button onClick={() => void openModal()}>
            <Plus className="w-4 h-4" />
            添加隧道
          </Button>
        </div>
      </div>

      {/* Tunnel List */}
      <div className="grid gap-4">
        {displayedTunnels.map((tunnel) => {
          const isEnabled = tunnel.enabled ?? tunnel.status.state === "forwarding";
          const isRunning = tunnel.status.state === "forwarding";

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
                  {tunnel.mode === "full" && (
                    <Button variant="secondary" size="sm" onClick={() => handleViewDetails(tunnel)}>
                      <Eye className="w-4 h-4" />
                      查看
                    </Button>
                  )}
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
                  {tunnel.mode === "single" && (
                    <>
                      <span
                        className="inline-flex"
                        title={isRunning ? UI_TEXT.tunnels.testTooltip.running : UI_TEXT.tunnels.testTooltip.idle}
                      >
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleTest(tunnel)}
                          loading={testingTunnelId === tunnel.id}
                          disabled={isRunning}
                        >
                          <Activity className="w-4 h-4" />
                          测试
                        </Button>
                      </span>
                      {tunnelTestResults[tunnel.id] && (
                        <Badge variant={tunnelTestResults[tunnel.id].ok ? "success" : "error"}>
                          {tunnelTestResults[tunnel.id].latency_ms != null
                            ? `${tunnelTestResults[tunnel.id].latency_ms}ms`
                            : tunnelTestResults[tunnel.id].ok ? "成功" : "失败"}
                        </Badge>
                      )}
                    </>
                  )}
                  <Button variant="ghost" size="sm" onClick={() => void openModal(tunnel)}>
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

          <div>
            <label className="block text-sm font-semibold text-slate-700 mb-2">
              选择主机
            </label>
            <select
              value={formData.host_id}
              onChange={(e) => setFormData({ ...formData, host_id: e.target.value })}
              className="w-full h-11 px-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none"
            >
              <option value="" disabled>
                请选择主机
              </option>
              {hosts.map((host) => {
                const missingKey = host.auth_type === "private_key_path" && !host.private_key_path;
                const disabled = missingKey;
                const suffix = missingKey ? "（缺少私钥路径）" : "";
                return (
                  <option key={host.id} value={host.id} disabled={disabled}>
                    {host.name || host.host} ({host.username}@{host.host}:{host.port}){suffix}
                  </option>
                );
              })}
            </select>
            {hosts.length === 0 ? (
              <p className="text-xs text-slate-500 mt-2">请先在主机页面添加 SSH 主机</p>
            ) : null}
          </div>

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
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-semibold text-slate-700 mb-2">
                  端口筛选规则
                </label>
                <select
                  value={formData.ports_filter_mode}
                  onChange={(e) => setFormData({
                    ...formData,
                    ports_filter_mode: e.target.value as "exclude" | "include",
                  })}
                  className="w-full h-11 px-3 rounded-lg bg-white border border-slate-200 shadow-sm outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
                >
                  <option value="exclude">黑名单（不穿透以下端口）</option>
                  <option value="include">白名单（仅穿透以下端口）</option>
                </select>
              </div>
              {formData.ports_filter_mode === "include" ? (
                <div>
                  <label className="block text-sm font-semibold text-slate-700 mb-2">
                    穿透端口白名单（逗号/空格/换行分隔）
                  </label>
                  <textarea
                    value={formData.include_ports_text}
                    onChange={(e) => setFormData({ ...formData, include_ports_text: e.target.value })}
                    rows={3}
                    className="w-full px-4 py-3 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
                    placeholder="例如: 22, 80, 443"
                  />
                  {parsePortsText(formData.include_ports_text).length === 0 && (
                    <p className="text-xs text-amber-600 mt-2">不会穿透任何端口</p>
                  )}
                </div>
              ) : (
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
            </div>
          )}

          <div className="flex justify-end gap-3 pt-4">
            <Button variant="secondary" onClick={() => setShowModal(false)}>
              取消
            </Button>
            <Button onClick={handleSubmit} loading={loading} disabled={!formData.host_id}>
              保存
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        isOpen={showDetailsModal}
        onClose={() => {
          setShowDetailsModal(false);
          setDetailsTunnel(null);
          setDetailsTunnels([]);
        }}
        title={`全穿透详情${detailsTunnel?.name ? ` - ${detailsTunnel.name}` : ""}`}
        size="lg"
      >
        <div className="space-y-3">
          {detailsLoading ? (
            <div className="text-center py-6 text-slate-500">
              <div className="w-6 h-6 border-2 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
              <p className="mt-2 text-sm">加载中...</p>
            </div>
          ) : detailsTunnels.length === 0 ? (
            <div className="text-center py-6 text-slate-500">
              暂无穿透端口
            </div>
          ) : (
            <div className="overflow-auto rounded-lg border border-slate-100">
              <table className="min-w-full text-sm">
                <thead className="bg-slate-50 text-slate-600">
                  <tr>
                    <th className="text-left px-3 py-2">端口</th>
                    <th className="text-left px-3 py-2">远程绑定</th>
                    <th className="text-left px-3 py-2">状态</th>
                    <th className="text-left px-3 py-2">连接</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {detailsTunnels.map((t) => (
                    <tr key={t.id}>
                      <td className="px-3 py-2 font-mono">{t.remote_port ?? t.local_port}</td>
                      <td className="px-3 py-2 font-mono">{t.remote_bind_addr}</td>
                      <td className="px-3 py-2">
                        <Badge
                          variant={
                            t.status.state === "forwarding" ? "success" :
                            t.status.state === "connecting" ? "warning" :
                            t.status.state === "error" ? "error" : "default"
                          }
                        >
                          {t.status.state}
                        </Badge>
                        {t.status.last_error && (
                          <span className="ml-2 text-xs text-red-500">
                            {t.status.last_error.code}: {t.status.last_error.message}
                          </span>
                        )}
                      </td>
                      <td className="px-3 py-2">{t.status.active_conns}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </Modal>
    </div>
  );
}
