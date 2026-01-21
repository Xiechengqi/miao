"use client";

import { useEffect, useMemo, useState } from "react";
import { Card, CardHeader, CardContent, Button, Badge, TogglePower, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { useProxies, useStatus, useTraffic } from "@/hooks";
import { api } from "@/lib/api";
import { ManualNode, ManualNodeType, SubFilesResponse } from "@/types/api";
import { getDelayClass, getDelayText, formatUptime, formatSpeed } from "@/lib/utils";
import { RefreshCw, Zap, Search, Activity, Clock, Cpu, Wifi, Plus, Pencil, Trash2, Globe, ArrowDownUp, Bolt } from "lucide-react";

type NodeTypeOption = "hysteria2" | "anytls" | "ss" | "ssh";

const CIPHER_OPTIONS = [
  "2022-blake3-aes-128-gcm",
  "2022-blake3-aes-256-gcm",
  "2022-blake3-chacha20-poly1305",
  "aes-128-gcm",
  "aes-256-gcm",
  "chacha20-ietf-poly1305",
];

const CONNECTIVITY_SITES = [
  { name: "Google", url: "https://www.google.com" },
  { name: "GitHub", url: "https://github.com" },
  { name: "YouTube", url: "https://www.youtube.com" },
  { name: "Twitter", url: "https://x.com" },
  { name: "Telegram", url: "https://telegram.org" },
  { name: "OpenAI", url: "https://openai.com" },
];

const defaultForm = {
  tag: "",
  server: "",
  server_port: "443",
  user: "",
  password: "",
  sni: "",
  cipher: "2022-blake3-aes-128-gcm",
};

const formatNodeType = (type: ManualNodeType) => {
  if (type === "hysteria2") return "Hysteria2";
  if (type === "anytls") return "AnyTLS";
  if (type === "shadowsocks") return "SS";
  if (type === "ssh") return "SSH";
  return type;
};

type ConnectivityResult = {
  success: boolean;
  latency_ms?: number;
};

export default function ProxiesPage() {
  const {
    setLoading,
    loading,
    addToast,
    proxyGroups,
    setProxyGroups,
    setNodes: setProxyNodes,
    delays,
    setStatus,
    setDnsStatus,
  } = useStore();
  const { fetchProxies, testAllDelays, switchProxy, testDelay } = useProxies();
  const { status, dnsStatus, loadingAction, checkDnsNow, switchDnsActive, toggleService } = useStatus();
  const { traffic } = useTraffic();
  const [searchTerm, setSearchTerm] = useState("");
  const [subInfo, setSubInfo] = useState<SubFilesResponse | null>(null);
  const [subLoaded, setSubLoaded] = useState(false);
  const [nodes, setNodes] = useState<ManualNode[]>([]);
  const [nodesLoaded, setNodesLoaded] = useState(false);
  const [sortByDelay, setSortByDelay] = useState(false);
  const [testingGroup, setTestingGroup] = useState<string | null>(null);
  const [dnsSelected, setDnsSelected] = useState("");
  const [showNodeModal, setShowNodeModal] = useState(false);
  const [editingTag, setEditingTag] = useState<string | null>(null);
  const [nodeType, setNodeType] = useState<NodeTypeOption>("hysteria2");
  const [formData, setFormData] = useState(defaultForm);
  const [connectivityResults, setConnectivityResults] = useState<Record<string, ConnectivityResult>>({});
  const [testingConnectivity, setTestingConnectivity] = useState(false);
  const [currentTestingSite, setCurrentTestingSite] = useState<string | null>(null);

  useEffect(() => {
    const loadData = async () => {
      setLoading(true, "init");
      try {
        const [statusData, dnsData, { proxies, nodes: nodeList }] = await Promise.all([
          api.getStatus(),
          api.getDnsStatus().catch(() => null),
          api.getProxies(),
        ]);
        setStatus(statusData);
        if (dnsData) setDnsStatus(dnsData);
        setProxyGroups(proxies);
        setProxyNodes(nodeList);
      } catch (error) {
        console.error("Failed to load data:", error);
        addToast({ type: "error", message: "加载数据失败" });
      } finally {
        setLoading(false);
      }
    };
    loadData();
    loadSubFiles();
    loadNodes();
  }, []);

  useEffect(() => {
    if (!dnsSelected && dnsStatus?.candidates?.length) {
      setDnsSelected(dnsStatus.active || dnsStatus.candidates[0]);
    }
  }, [dnsStatus, dnsSelected]);

  const canSubmitNode = useMemo(() => {
    return formData.tag.trim() && formData.server.trim() && Number(formData.server_port) > 0;
  }, [formData]);

  const handleTestDelay = async (nodeName: string) => {
    const delay = await testDelay(nodeName);
    if (delay !== undefined) {
      addToast({ type: "success", message: `${nodeName}: ${delay}ms` });
    }
  };

  const selectorGroups = useMemo(() => {
    const groups: Record<string, (typeof proxyGroups)[string]> = {};
    for (const [name, group] of Object.entries(proxyGroups)) {
      if (group.type === "Selector" && !name.startsWith("_")) {
        groups[name] = group;
      }
    }
    return groups;
  }, [proxyGroups]);

  const getFilteredNodes = (list: string[]) => {
    if (!searchTerm.trim()) return list;
    const keyword = searchTerm.trim().toLowerCase();
    return list.filter((node) => node.toLowerCase().includes(keyword));
  };

  const getSortedNodes = (list: string[]) => {
    if (!sortByDelay) return list;
    return [...list].sort((a, b) => {
      const delayA = delays[a];
      const delayB = delays[b];
      const scoreA = delayA && delayA > 0 ? delayA : Number.POSITIVE_INFINITY;
      const scoreB = delayB && delayB > 0 ? delayB : Number.POSITIVE_INFINITY;
      return scoreA - scoreB;
    });
  };

  const handleReloadSubscription = async () => {
    setLoading(true, "reload");
    try {
      await api.reloadSubscription();
      addToast({ type: "success", message: "配置已重载" });
      fetchProxies();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重载失败" });
    } finally {
      setLoading(false);
    }
  };

  const loadSubFiles = async () => {
    try {
      const data = await api.getSubFiles();
      setSubInfo(data);
    } catch (error) {
      addToast({ type: "error", message: "获取订阅文件失败" });
    } finally {
      setSubLoaded(true);
    }
  };

  const reloadSubFiles = async () => {
    setLoading(true, "reloadSubFiles");
    try {
      await api.reloadSubFiles();
      addToast({ type: "success", message: "订阅文件已重载" });
      loadSubFiles();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "重载失败" });
    } finally {
      setLoading(false);
    }
  };

  const loadNodes = async () => {
    try {
      const data = await api.getNodes();
      setNodes(data);
    } catch (error) {
      addToast({ type: "error", message: "获取节点失败" });
    } finally {
      setNodesLoaded(true);
    }
  };

  const openCreateModal = () => {
    setEditingTag(null);
    setNodeType("hysteria2");
    setFormData(defaultForm);
    setShowNodeModal(true);
  };

  const openEditModal = async (tag: string) => {
    setLoading(true, "node-edit");
    try {
      const data = await api.getNode(tag);
      setEditingTag(tag);
      setNodeType(data.node_type === "shadowsocks" ? "ss" : data.node_type);
      setFormData({
        tag: data.tag,
        server: data.server,
        server_port: data.server_port?.toString() || "",
        user: data.user || "",
        password: "",
        sni: data.sni || "",
        cipher: data.cipher || "2022-blake3-aes-128-gcm",
      });
      setShowNodeModal(true);
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "加载节点失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleTestNodeFromModal = async (server: string, port: number) => {
    setLoading(true, "node-test");
    try {
      const data = await api.testNode(server, port);
      addToast({ type: "success", message: `连接成功 (${data.latency_ms}ms)` });
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "测试失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleTestNodeFromList = async (node: ManualNode) => {
    if (status.running) {
      const delay = await testDelay(node.tag);
      if (delay === undefined) {
        addToast({ type: "error", message: "测试失败" });
        return;
      }
      addToast({
        type: delay > 0 ? "success" : "error",
        message: delay > 0 ? `延迟 ${delay}ms` : "测试超时",
      });
      return;
    }
    handleTestNodeFromModal(node.server, node.server_port);
  };
  const handleDeleteNode = async (tag: string) => {
    if (!confirm(`确定要删除节点 "${tag}" 吗？`)) return;
    setLoading(true, "node-delete");
    try {
      await api.deleteNode(tag);
      addToast({ type: "success", message: "节点已删除" });
      loadNodes();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleSubmitNode = async () => {
    if (!canSubmitNode) return;
    setLoading(true, "node-save");
    try {
      const payload: Partial<ManualNode> = {
        tag: formData.tag.trim(),
        node_type: nodeType === "ss" ? "shadowsocks" : nodeType,
        server: formData.server.trim(),
        server_port: Number(formData.server_port),
      };

      if (nodeType === "ssh") {
        payload.user = formData.user.trim();
      }
      if (nodeType === "ss") {
        payload.cipher = formData.cipher;
      }
      if (nodeType === "hysteria2" || nodeType === "anytls") {
        if (formData.sni.trim()) {
          payload.sni = formData.sni.trim();
        }
      }
      if (formData.password.trim()) {
        payload.password = formData.password.trim();
      }

      if (editingTag) {
        await api.updateNode(editingTag, payload);
        addToast({ type: "success", message: "节点已更新" });
      } else {
        await api.createNode(payload as ManualNode);
        addToast({ type: "success", message: "节点已添加" });
      }
      setShowNodeModal(false);
      loadNodes();
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "保存失败" });
    } finally {
      setLoading(false);
    }
  };

  const renderSubFiles = () => {
    if (!subLoaded) {
      return (
        <div className="text-center py-10">
          <div className="w-10 h-10 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      );
    }

    if (!subInfo) {
      return (
        <div className="text-center py-10">
          <p className="text-slate-500">订阅文件不可用</p>
        </div>
      );
    }

    return (
      <div className="space-y-4">
        <div className="text-sm text-slate-600 space-y-1">
          <div>目录：<span className="font-mono">{subInfo.sub_dir || "-"}</span></div>
          {subInfo.sub_source && (
            <div>
              来源：
              {subInfo.sub_source.type === "git" && (
                <span className="font-mono ml-1">{subInfo.sub_source.url}</span>
              )}
              {subInfo.sub_source.type === "path" && (
                <span className="font-mono ml-1">{subInfo.sub_source.value}</span>
              )}
            </div>
          )}
          {subInfo.error && (
            <div className="text-red-600">{subInfo.error}</div>
          )}
        </div>

        {subInfo.files.length === 0 ? (
          <div className="text-center py-8 text-slate-500">目录下暂无订阅文件</div>
        ) : (
          <div className="space-y-3">
            {subInfo.files.map((file) => (
              <div
                key={file.file_path}
                className="flex items-center justify-between rounded-lg border border-slate-100 bg-slate-50 px-4 py-3"
              >
                <div>
                  <div className="font-semibold text-slate-900">{file.file_name}</div>
                  <div className="text-xs text-slate-500">
                    {file.node_count} 节点 · {file.loaded ? "已加载" : `跳过：${file.error || "解析失败"}`}
                  </div>
                </div>
                <Badge variant={file.loaded ? "success" : "warning"}>
                  {file.loaded ? "已加载" : "未加载"}
                </Badge>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  };

  const getConnectivityBadge = (result?: ConnectivityResult) => {
    if (!result) return "default";
    if (!result.success) return "error";
    if ((result.latency_ms || 0) < 300) return "success";
    if ((result.latency_ms || 0) < 800) return "warning";
    return "error";
  };

  const formatConnectivityDelay = (result?: ConnectivityResult) => {
    if (!result) return "-";
    if (!result.success) return "超时";
    return `${result.latency_ms}ms`;
  };

  const getDnsHealthVariant = (health?: "ok" | "bad" | "cooldown") => {
    if (health === "ok") return "success";
    if (health === "bad") return "error";
    if (health === "cooldown") return "warning";
    return "default";
  };

  const testConnectivity = async (site: { name: string; url: string }) => {
    if (currentTestingSite) return;
    setCurrentTestingSite(site.name);
    try {
      const result = await api.testConnectivity(site.url);
      setConnectivityResults((prev) => ({
        ...prev,
        [site.name]: result,
      }));
    } catch {
      setConnectivityResults((prev) => ({
        ...prev,
        [site.name]: { success: false },
      }));
    } finally {
      setCurrentTestingSite(null);
    }
  };

  const testAllConnectivity = async () => {
    if (testingConnectivity) return;
    setTestingConnectivity(true);
    const results: Record<string, ConnectivityResult> = {};
    for (const site of CONNECTIVITY_SITES) {
      setCurrentTestingSite(site.name);
      try {
        const result = await api.testConnectivity(site.url);
        results[site.name] = result;
      } catch {
        results[site.name] = { success: false };
      }
    }
    setConnectivityResults(results);
    setCurrentTestingSite(null);
    setTestingConnectivity(false);
  };

  const handleTestGroupDelays = async (groupName: string, list: string[]) => {
    if (testingGroup) return;
    setTestingGroup(groupName);
    for (const nodeName of list) {
      await testDelay(nodeName);
    }
    setTestingGroup(null);
    addToast({ type: "success", message: "测速完成" });
  };

  const handleSelectFastest = async (groupName: string, list: string[]) => {
    const candidates = list
      .map((nodeName) => ({
        name: nodeName,
        delay: delays[nodeName],
      }))
      .filter((item) => typeof item.delay === "number" && item.delay > 0) as Array<{ name: string; delay: number }>;
    if (candidates.length === 0) {
      addToast({ type: "warning", message: "没有可用的测速结果" });
      return;
    }
    const fastest = candidates.reduce((prev, curr) => (curr.delay < prev.delay ? curr : prev));
    await switchProxy(groupName, fastest.name);
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-3xl font-black">
            代理管理
          </h1>
          <p className="text-slate-500 mt-1">节点列表与延迟测试</p>
        </div>
        <div className="flex gap-3">
          <Button variant="secondary" onClick={handleReloadSubscription} loading={loading}>
            <RefreshCw className="w-4 h-4" />
            重载配置
          </Button>
          <Button onClick={testAllDelays} loading={loading}>
            <Zap className="w-4 h-4" />
            测试延迟
          </Button>
        </div>
      </div>

      <Card className="p-4">
        <CardHeader className="mb-3">
          <Activity className="w-5 h-5 text-indigo-600" />
          <span className="text-lg font-bold text-slate-900">Sing-box 状态</span>
          <div className="ml-auto flex items-center gap-3">
            <TogglePower
              running={status.running}
              loading={loading && (loadingAction === "start" || loadingAction === "stop")}
              onToggle={toggleService}
              size="md"
            />
          </div>
        </CardHeader>

        <CardContent>
          <div className="flex flex-wrap items-center gap-3">
            <Badge variant={status.running ? "success" : "error"} dot>
              {status.running ? "运行中" : "已停止"}
            </Badge>

            {status.running && (
              <>
                <div className="flex items-center gap-2 px-2.5 py-1 rounded-lg bg-slate-100 text-xs">
                  <Cpu className="w-3.5 h-3.5 text-slate-600" />
                  <span className="font-mono">{status.pid || "-"}</span>
                </div>
                <div className="flex items-center gap-2 px-2.5 py-1 rounded-lg bg-slate-100 text-xs">
                  <Clock className="w-3.5 h-3.5 text-slate-600" />
                  <span className="font-mono">{formatUptime(status.uptime_secs)}</span>
                </div>
              </>
            )}

            <div className="flex items-center gap-2 px-2.5 py-1 rounded-lg bg-slate-100 text-xs">
              <Wifi className="w-3.5 h-3.5 text-slate-600" />
              <span>{dnsStatus?.active || "-"}</span>
            </div>

            <button
              onClick={checkDnsNow}
              disabled={loading || loadingAction === "dns-check"}
              className="p-1.5 rounded-lg bg-slate-100 hover:bg-slate-200 transition-colors"
              title="刷新 DNS"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loadingAction === "dns-check" ? "animate-spin" : ""}`} />
            </button>
          </div>

          {dnsStatus?.candidates && dnsStatus.candidates.length > 0 && (
            <div className="mt-3 pt-3 border-t border-slate-100 space-y-3">
              <div className="flex flex-wrap items-center justify-end gap-2">
                <select
                  className="h-9 rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-700"
                  value={dnsSelected}
                  onChange={(e) => setDnsSelected(e.target.value)}
                >
                  {dnsStatus.candidates.map((candidate) => (
                    <option key={candidate} value={candidate}>
                      {candidate}
                    </option>
                  ))}
                </select>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => switchDnsActive(dnsSelected)}
                  loading={loadingAction === "dns-switch"}
                  disabled={!dnsSelected}
                >
                  切换 DNS
                </Button>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {dnsStatus.candidates.map((candidate) => {
                  const health = dnsStatus.health?.[candidate];
                  return (
                  <Badge key={candidate} variant={getDnsHealthVariant(health)}>
                      <span title={health ? `${candidate} (${health})` : candidate}>{candidate}</span>
                  </Badge>
                );
              })}
                {dnsStatus.last_check_secs_ago !== undefined && dnsStatus.last_check_secs_ago !== null && (
                  <span className="text-xs text-slate-500">
                    {dnsStatus.last_check_secs_ago}s ago
                  </span>
                )}
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {status.running && (
        <Card className="p-6" hoverEffect={false}>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="text-2xl font-bold text-slate-900">HTTPS 连通性测试</h2>
              <p className="text-slate-500 text-sm mt-1">测试代理对外站点可用性</p>
            </div>
            <Button variant="secondary" onClick={testAllConnectivity} loading={testingConnectivity}>
              <Globe className="w-4 h-4" />
              测试连接
            </Button>
          </div>

          <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {CONNECTIVITY_SITES.map((site) => {
              const result = connectivityResults[site.name];
              const isTesting = currentTestingSite === site.name;
              const isPending = testingConnectivity && currentTestingSite !== site.name && !connectivityResults[site.name];
              return (
                <button
                  key={site.name}
                  onClick={() => testConnectivity(site)}
                  disabled={!!currentTestingSite || testingConnectivity}
                  className="flex items-center justify-between rounded-lg border border-slate-100 bg-white px-4 py-3 text-left hover:border-indigo-200"
                >
                  <div>
                    <div className="font-semibold text-slate-900">{site.name}</div>
                    <div className="text-xs text-slate-500">{site.url}</div>
                  </div>
                  <Badge variant={getConnectivityBadge(result)}>
                    {isTesting ? "测试中" : isPending ? "等待中" : formatConnectivityDelay(result)}
                  </Badge>
                </button>
              );
            })}
          </div>
        </Card>
      )}

      {status.running && (
        <Card className="p-6" hoverEffect={false}>
          <CardHeader className="mb-3">
            <span className="text-lg font-bold text-slate-900">节点选择</span>
          </CardHeader>

          {Object.keys(selectorGroups).length === 0 ? (
            <div className="text-center py-8 text-slate-500">正在加载节点...</div>
          ) : (
            <>
              <div className="relative mb-4">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-slate-500" />
                <input
                  type="text"
                  placeholder="搜索节点..."
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  className="w-full h-12 pl-12 pr-4 rounded-lg bg-white border border-slate-200 shadow-sm border-0 outline-none focus:bg-white focus:shadow-[0_4px_20px_-2px_rgba(79,70,229,0.1)] focus:ring-4 focus:ring-indigo-500/20 transition-all"
                />
                {searchTerm && (
                  <button
                    onClick={() => setSearchTerm("")}
                    className="absolute right-4 top-1/2 -translate-y-1/2 text-xs text-slate-500"
                  >
                    清除
                  </button>
                )}
              </div>

              {Object.entries(selectorGroups).map(([groupName, group]) => {
                const displayName = groupName === "proxy" ? "选中节点" : groupName;
                const filteredNodes = getFilteredNodes(group.all);
                const displayNodes = getSortedNodes(filteredNodes);
                return (
                  <div key={groupName} className="mb-6">
                    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                      <div className="flex items-center gap-2">
                        <span className="text-base font-semibold text-slate-900">{displayName}</span>
                        {group.now && (
                          <span className="text-xs text-slate-500">当前: {group.now}</span>
                        )}
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <Button
                          variant={sortByDelay ? "primary" : "secondary"}
                          size="sm"
                          onClick={() => setSortByDelay((prev) => !prev)}
                        >
                          <ArrowDownUp className="w-4 h-4" />
                          排序
                        </Button>
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleSelectFastest(groupName, displayNodes)}
                          disabled={displayNodes.length === 0}
                        >
                          <Bolt className="w-4 h-4" />
                          最快
                        </Button>
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleTestGroupDelays(groupName, displayNodes)}
                          loading={testingGroup === groupName}
                          disabled={displayNodes.length === 0}
                        >
                          <Zap className="w-4 h-4" />
                          测速
                        </Button>
                      </div>
                    </div>

                    <div className="mt-3 flex flex-wrap gap-2">
                      {displayNodes.map((nodeName) => {
                        const isActive = group.now === nodeName;
                        const delay = delays[nodeName];

                        return (
                          <button
                            key={nodeName}
                            onClick={() => switchProxy(groupName, nodeName)}
                            onContextMenu={(e) => {
                              e.preventDefault();
                              handleTestDelay(nodeName);
                            }}
                            className={`group flex items-center gap-2 rounded-lg border px-3 py-2 text-left transition-all ${
                              isActive
                                ? "border-indigo-500 bg-indigo-50 text-indigo-700"
                                : "border-slate-200 bg-white hover:border-slate-300 hover:bg-slate-50"
                            }`}
                          >
                            <span className="max-w-[140px] truncate text-sm font-medium" title={nodeName}>
                              {nodeName}
                            </span>
                            {delay !== undefined && (
                              <span className={`text-xs font-mono ${getDelayClass(delay)}`}>
                                {delay === 0 ? "超时" : getDelayText(delay)}
                              </span>
                            )}
                          </button>
                        );
                      })}
                    </div>
                    {searchTerm && displayNodes.length === 0 && (
                      <div className="mt-3 text-sm text-slate-500">没有匹配的节点</div>
                    )}
                  </div>
                );
              })}

              <div className="flex items-center gap-4 text-sm text-slate-600">
                <div className="flex items-center gap-1.5">
                  <Zap className="w-4 h-4 text-emerald-600" />
                  <span className="font-mono">{formatSpeed(traffic.up)}</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <RefreshCw className="w-4 h-4 text-sky-600" />
                  <span className="font-mono">{formatSpeed(traffic.down)}</span>
                </div>
              </div>
            </>
          )}
        </Card>
      )}

      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-2xl font-bold text-slate-900">订阅文件</h2>
            <p className="text-slate-500 text-sm mt-1">管理 sing-box 订阅文件</p>
          </div>
          <Button variant="secondary" onClick={reloadSubFiles} loading={loading}>
            <RefreshCw className="w-4 h-4" />
            重载
          </Button>
        </div>
        <div className="mt-6">
          {renderSubFiles()}
        </div>
      </Card>

      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-2xl font-bold text-slate-900">手动节点</h2>
            <p className="text-slate-500 text-sm mt-1">管理手动添加的节点</p>
          </div>
          <Button onClick={openCreateModal}>
            <Plus className="w-4 h-4" />
            添加节点
          </Button>
        </div>

        <div className="mt-6">
          {!nodesLoaded ? (
            <div className="text-center py-10">
              <div className="w-10 h-10 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
              <p className="mt-4 text-slate-500">加载中...</p>
            </div>
          ) : nodes.length === 0 ? (
            <div className="text-center py-8 text-slate-500">暂无手动节点</div>
          ) : (
            <div className="space-y-3">
              {nodes.map((node) => (
                <div
                  key={node.tag}
                  className="flex flex-col gap-3 rounded-lg border border-slate-100 bg-white px-4 py-4 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-slate-900">{node.tag}</span>
                      <Badge variant="info">{formatNodeType(node.node_type)}</Badge>
                    </div>
                    <div className="text-sm text-slate-500 mt-1">
                      {node.server}:{node.server_port}
                      {node.sni && <span className="ml-2">SNI: {node.sni}</span>}
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleTestNodeFromList(node)}
                      loading={loading}
                    >
                      <Zap className="w-4 h-4" />
                      测试
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => openEditModal(node.tag)}
                    >
                      <Pencil className="w-4 h-4" />
                      编辑
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDeleteNode(node.tag)}
                    >
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
        isOpen={showNodeModal}
        onClose={() => setShowNodeModal(false)}
        title={editingTag ? "编辑节点" : "添加节点"}
        size="lg"
      >
        <div className="space-y-5">
          <div className="flex flex-wrap gap-2">
            {(["hysteria2", "anytls", "ss", "ssh"] as NodeTypeOption[]).map((type) => (
              <button
                key={type}
                onClick={() => setNodeType(type)}
                className={`px-4 py-2 rounded-lg text-sm font-semibold border transition ${
                  nodeType === type
                    ? "border-indigo-500 text-indigo-600 bg-indigo-50"
                    : "border-slate-200 text-slate-500 hover:border-indigo-200 hover:text-indigo-600"
                }`}
              >
                {type === "ss" ? "Shadowsocks" : type.toUpperCase()}
              </button>
            ))}
          </div>

          <Input
            label="节点名称"
            placeholder="例如: 我的节点"
            value={formData.tag}
            onChange={(e) => setFormData({ ...formData, tag: e.target.value })}
          />

          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            <Input
              label="服务器地址"
              placeholder="例如: example.com"
              value={formData.server}
              onChange={(e) => setFormData({ ...formData, server: e.target.value })}
            />
            <Input
              label="端口"
              type="number"
              value={formData.server_port}
              onChange={(e) => setFormData({ ...formData, server_port: e.target.value })}
            />
            {nodeType === "ssh" ? (
              <Input
                label="用户名"
                placeholder="root"
                value={formData.user}
                onChange={(e) => setFormData({ ...formData, user: e.target.value })}
              />
            ) : null}
          </div>

          {nodeType === "ss" && (
            <div>
              <label className="block text-sm font-semibold text-slate-700 mb-2">加密方式</label>
              <select
                value={formData.cipher}
                onChange={(e) => setFormData({ ...formData, cipher: e.target.value })}
                className="w-full h-11 rounded-lg border border-slate-200 bg-white px-4 text-slate-900 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
              >
                {CIPHER_OPTIONS.map((cipher) => (
                  <option key={cipher} value={cipher}>
                    {cipher}
                  </option>
                ))}
              </select>
            </div>
          )}

          {(nodeType === "hysteria2" || nodeType === "anytls") && (
            <Input
              label="SNI (可选)"
              placeholder="留空使用服务器地址"
              value={formData.sni}
              onChange={(e) => setFormData({ ...formData, sni: e.target.value })}
            />
          )}

          <Input
            label="密码"
            type="password"
            placeholder={editingTag ? "留空保持不变" : "可选"}
            value={formData.password}
            onChange={(e) => setFormData({ ...formData, password: e.target.value })}
          />

          <div className="flex flex-wrap justify-end gap-3 pt-2">
            <Button
              variant="secondary"
              onClick={() => handleTestNodeFromModal(formData.server.trim(), Number(formData.server_port))}
              loading={loading}
              disabled={!formData.server.trim() || !formData.server_port}
            >
              <Zap className="w-4 h-4" />
              测试
            </Button>
            <Button variant="secondary" onClick={() => setShowNodeModal(false)}>
              取消
            </Button>
            <Button onClick={handleSubmitNode} loading={loading} disabled={!canSubmitNode}>
              保存
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
