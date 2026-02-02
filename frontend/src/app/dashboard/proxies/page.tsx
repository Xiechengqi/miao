"use client";

import { useEffect, useMemo, useState, useCallback, useRef } from "react";
import { Card, CardHeader, CardContent, Button, Badge, TogglePower, Skeleton, SkeletonCard, SkeletonNodeGrid, SkeletonConnectivity, SkeletonStatusCards, ConfirmModal } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { useProxies, useStatus, useTraffic } from "@/hooks";
import { api } from "@/lib/api";
import { getDelayClass, getDelayText, formatUptime, formatSpeed } from "@/lib/utils";
import { RefreshCw, Zap, Search, Activity, Clock, Cpu, Wifi, Globe, Pin, Pause, Server, Plus, Check } from "lucide-react";
import { NodeCard, FavoriteNodeCard } from "@/components/dashboard/NodeCard";
import { Host, ManualNode, DnsCandidate } from "@/types/api";

const CONNECTIVITY_SITES = [
  { name: "Google", url: "https://www.google.com" },
  { name: "GitHub", url: "https://github.com" },
  { name: "YouTube", url: "https://youtube.com" },
  { name: "Twitter", url: "https://x.com" },
  { name: "Telegram", url: "https://telegram.org" },
  { name: "OpenAI", url: "https://openai.com" },
];

const DEFAULT_TEST_URL = "http://www.gstatic.com/generate_204";
const DELAY_CACHE_DURATION = 30 * 1000; // 延迟缓存30秒
const CONNECTIVITY_STORAGE_KEY = "miao_connectivity_results";

type ConnectivityResult = {
  success: boolean;
  latency_ms?: number;
};

type ConnectivityCache = {
  results: Record<string, ConnectivityResult>;
};

export default function ProxiesPage() {
  const {
    setLoading,
    loading,
    addToast,
    proxyGroups,
    setProxyGroups,
    setNodes: setProxyNodes,
    nodes: storeNodes,
    delays,
    setStatus,
    setDnsStatus,
  } = useStore();
  const { fetchProxies, testDelay, setDelays } = useProxies();
  const { status, dnsStatus, loadingAction, checkDnsNow, switchDnsActive, toggleService } = useStatus();
  const { traffic } = useTraffic();
  const [searchTerm, setSearchTerm] = useState("");
  const [sortByDelay, setSortByDelay] = useState(false);
  const [dnsSelected, setDnsSelected] = useState("");
  const [connectivityResults, setConnectivityResults] = useState<Record<string, ConnectivityResult>>({});
  const [testingConnectivity, setTestingConnectivity] = useState(false);
  const [currentTestingSite, setCurrentTestingSite] = useState<string | null>(null);
  const [testingAllNodes, setTestingAllNodes] = useState(false);
  const [speedTestProgress, setSpeedTestProgress] = useState({ current: 0, total: 0 });
  const [speedTestCancelled, setSpeedTestCancelled] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);
  const [favoriteNodes, setFavoriteNodes] = useState<string[]>([]);
  const [selectedGroup, setSelectedGroup] = useState<string>("all");
  const [testUrl, setTestUrl] = useState(DEFAULT_TEST_URL);
  const [pendingRemoveNode, setPendingRemoveNode] = useState<string | null>(null);
  const [removingNode, setRemovingNode] = useState(false);

  // SSH 节点相关状态
  const [hosts, setHosts] = useState<Host[]>([]);
  const [hostsLoading, setHostsLoading] = useState(false);
  const [addingHostId, setAddingHostId] = useState<string | null>(null);
  const [testingHostId, setTestingHostId] = useState<string | null>(null);
  const [existingNodeTags, setExistingNodeTags] = useState<Set<string>>(() => new Set());
  const [manualNodes, setManualNodes] = useState<ManualNode[]>([]);

  // DNS 切换确认对话框
  const [showDnsConfirm, setShowDnsConfirm] = useState(false);
  const [pendingDns, setPendingDns] = useState<string>("");
  const normalizedDnsCandidates = useMemo(() => {
    if (!dnsStatus?.candidates) return [];
    return (dnsStatus.candidates as Array<string | DnsCandidate>).map((candidate) => {
      if (typeof candidate === "string") {
        return { name: candidate, health: dnsStatus.health?.[candidate] };
      }
      const name = candidate?.name ?? String(candidate);
      return { name, health: candidate?.health ?? dnsStatus.health?.[name] };
    });
  }, [dnsStatus]);

  // 延迟缓存 - 使用 useRef 避免频繁重建回调
  const [delayTimestamps, setDelayTimestamps] = useState<Record<string, number>>({});
  const delayTimestampsRef = useRef<Record<string, number>>({});

  // 同步 state 到 ref
  useEffect(() => {
    delayTimestampsRef.current = delayTimestamps;
  }, [delayTimestamps]);

  // 初始化加载
  useEffect(() => {
    let isMounted = true;

    // 安全读取 localStorage
    const safeGetItem = <T,>(key: string, fallback: T): T => {
      try {
        const item = localStorage.getItem(key);
        if (!item) return fallback;
        return JSON.parse(item) as T;
      } catch {
        console.warn(`Failed to read ${key} from localStorage`);
        return fallback;
      }
    };

    // 加载收藏节点
    const savedFavorites = safeGetItem("miao_favorite_nodes", [] as string[]);
    if (isMounted && savedFavorites.length > 0) {
      setFavoriteNodes(savedFavorites);
    }

    // 加载测速URL
    const savedUrl = safeGetItem("miao_test_url", DEFAULT_TEST_URL);
    if (isMounted && savedUrl !== DEFAULT_TEST_URL) {
      setTestUrl(savedUrl);
    }

    // 加载连通性测试结果
    const savedConnectivity = safeGetItem<ConnectivityCache>(CONNECTIVITY_STORAGE_KEY, { results: {} });
    if (isMounted && Object.keys(savedConnectivity.results).length > 0) {
      const allowedUrls = new Set(CONNECTIVITY_SITES.map((site) => site.url));
      const filteredResults = Object.fromEntries(
        Object.entries(savedConnectivity.results).filter(([url]) => allowedUrls.has(url))
      );
      if (Object.keys(filteredResults).length > 0) {
        setConnectivityResults(filteredResults);
      }
    }

    const loadData = async () => {
      if (!isMounted) return;
      setLoading(true, "init");
      let statusData = null;
      try {
        // 先加载核心状态（快速响应）
        [statusData] = await Promise.all([
          api.getStatus(),
        ]);
        if (!isMounted) return;
        setStatus(statusData);

        // 然后并行加载其他数据
        const [dnsData, { proxies, nodes: nodeList }] = await Promise.all([
          api.getDnsStatus().catch(() => null),
          api.getProxies(),
        ]);
        if (!isMounted) return;
        if (dnsData) setDnsStatus(dnsData);
        setProxyGroups(proxies);
        setProxyNodes(nodeList);
      } catch (error) {
        if (!isMounted) return;
        console.error("Failed to load data:", error);
        // 只在 sing-box 运行中且加载失败时才显示错误
        // 如果 sing-box 未运行，UI 会显示友好提示，不需要额外的错误 toast
        if (statusData?.running) {
          addToast({ type: "error", message: "加载数据失败" });
        }
      } finally {
        if (isMounted) {
          setLoading(false);
        }
      }
    };
    loadData();

    return () => {
      isMounted = false;
    };
  }, [setStatus, setProxyGroups, setProxyNodes, setDnsStatus, setLoading, addToast]);

  // 同步连通性测试结果到 localStorage
  useEffect(() => {
    try {
      if (Object.keys(connectivityResults).length === 0) {
        localStorage.removeItem(CONNECTIVITY_STORAGE_KEY);
        return;
      }
      const payload: ConnectivityCache = { results: connectivityResults };
      localStorage.setItem(CONNECTIVITY_STORAGE_KEY, JSON.stringify(payload));
    } catch (error) {
      console.warn("Failed to save connectivity results:", error);
    }
  }, [connectivityResults]);

  // 加载 hosts 和已存在的节点标签
  useEffect(() => {
    const loadHostsAndNodes = async () => {
      setHostsLoading(true);
      try {
        // 并行加载
        const [hostsData, nodesData] = await Promise.all([
          api.getHosts().catch(() => [] as Host[]),
          api.getNodes().catch(() => [] as ManualNode[]),
        ]);
        setHosts(hostsData);
        setManualNodes(nodesData);
        // 记录已存在的节点标签
        const tags = new Set<string>();
        for (const node of nodesData) {
          tags.add(node.tag);
        }
        setExistingNodeTags(tags);
      } catch (error) {
        console.error("Failed to load hosts or nodes:", error);
      } finally {
        setHostsLoading(false);
      }
    };
    loadHostsAndNodes();
  }, []);

  // 添加主机为 SSH 节点
  const handleAddHostAsNode = async (host: Host) => {
    setAddingHostId(host.id);
    try {
      const tag = host.name ? `${host.name} (${host.host})` : host.host;
      const nodeData: Partial<ManualNode> & { tag: string; node_type: "ssh"; server: string; server_port: number; user: string } = {
        tag,
        node_type: "ssh",
        server: host.host,
        server_port: host.port || 22,
        user: host.username,
      };
      // Use private key if available, otherwise use password
      if (host.auth_type === "private_key_path" && host.private_key_path) {
        nodeData.private_key_path = host.private_key_path;
      } else {
        nodeData.password = host.password || "";
      }
      await api.createNode(nodeData as ManualNode);
      addToast({
        type: "success",
        message: status.running
          ? `SSH 节点 ${tag} 已添加`
          : `SSH 节点 ${tag} 已添加（启动后生效）`,
      });
      const nodesData = await api.getNodes().catch(() => [] as ManualNode[]);
      setManualNodes(nodesData);
      if (status.running) {
        // 服务运行中可同步代理信息
        fetchProxies(true);
      }
      // 更新已存在节点标签
      setExistingNodeTags(prev => new Set(prev).add(tag));
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "添加 SSH 节点失败" });
    } finally {
      setAddingHostId(null);
    }
  };

  const handleTestHost = async (host: Host) => {
    setTestingHostId(host.id);
    try {
      await api.testHost(host.id);
      addToast({ type: "success", message: "连接测试成功" });
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "连接测试失败" });
    } finally {
      setTestingHostId(null);
    }
  };

  // 删除 SSH 节点
  const handleRemoveSshNode = async (tag: string) => {
    try {
      setRemovingNode(true);
      await api.deleteNode(tag);
      addToast({
        type: "success",
        message: status.running ? "节点已删除" : "节点已删除（启动后生效）",
      });
      // 并行刷新：fetchProxies 返回 nodes 数据，直接使用
      const updateTags = async () => {
        try {
          const nodeList = await api.getNodes();
          const tags = new Set<string>();
          for (const node of nodeList) { tags.add(node.tag); }
          setExistingNodeTags(tags);
          setManualNodes(nodeList);
        } catch { /* 忽略错误 */ }
      };
      if (status.running) {
        await Promise.all([fetchProxies(true), updateTags()]);
      } else {
        await updateTags();
      }
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除节点失败" });
    } finally {
      setRemovingNode(false);
    }
  };

  useEffect(() => {
    if (!dnsSelected && normalizedDnsCandidates.length) {
      const activeName = dnsStatus?.active;
      setDnsSelected(activeName || normalizedDnsCandidates[0].name);
    }
  }, [dnsStatus, dnsSelected, normalizedDnsCandidates]);

  // 当 sing-box 停止时，不清空连通性测试结果（允许离线测试）
  // 只取消正在进行的批量测速任务（节点延迟测试依赖 sing-box）
  useEffect(() => {
    if (!status.running) {
      // 取消正在进行的节点延迟测速任务（依赖 sing-box API）
      if (testingAllNodes && abortControllerRef.current) {
        abortControllerRef.current.abort();
        abortControllerRef.current = null;
        setTestingAllNodes(false);
        setSpeedTestProgress({ current: 0, total: 0 });
      }
      // 注意：不重置 testingConnectivity 和 currentTestingSite
      // 因为连通性测试（测试外部网站）不依赖 sing-box
    }
  }, [status.running, testingAllNodes]);

  // 收集所有节点
  const allNodes = useMemo(() => {
    const nodeSet = new Set<string>();
    const groupNames = new Set(Object.keys(proxyGroups));

    // 从 proxyGroups 收集
    Object.values(proxyGroups).forEach((group) => {
      if (Array.isArray(group.all)) {
        group.all.forEach((node) => {
          // 过滤掉以下划线开头的名称（如 _dns）和与组名同名的条目
          if (node && !node.startsWith("_") && !groupNames.has(node)) {
            nodeSet.add(node);
          }
        });
      }
    });

    // 从手动节点列表收集（包括 SSH 节点）
    manualNodes.forEach((node) => {
      if (node.tag) nodeSet.add(node.tag);
    });

    return Array.from(nodeSet);
  }, [proxyGroups, manualNodes]);

  const manualNodeTags = useMemo(() => {
    return manualNodes.map((node) => node.tag).filter(Boolean) as string[];
  }, [manualNodes]);

  // 获取所有代理组名称
  const proxyGroupNames = useMemo(() => {
    return Object.keys(proxyGroups);
  }, [proxyGroups]);

  // 按组筛选
  const nodesByGroup = useMemo(() => {
    if (selectedGroup === "all") {
      return allNodes;
    }
    const group = proxyGroups[selectedGroup];
    if (Array.isArray(group?.all)) {
      return group.all;
    }
    return allNodes;
  }, [allNodes, proxyGroups, selectedGroup]);

  const filteredNodes = useMemo(() => {
    let nodes = nodesByGroup;
    if (!searchTerm.trim()) return nodes;
    const keyword = searchTerm.trim().toLowerCase();
    return nodes.filter((node) => node.toLowerCase().includes(keyword));
  }, [nodesByGroup, searchTerm]);

  const sortedNodes = useMemo(() => {
    if (!sortByDelay) return filteredNodes;
    return [...filteredNodes].sort((a, b) => {
      const delayA = delays[a];
      const delayB = delays[b];
      const scoreA = delayA && delayA > 0 ? delayA : Number.POSITIVE_INFINITY;
      const scoreB = delayB && delayB > 0 ? delayB : Number.POSITIVE_INFINITY;
      return scoreA - scoreB;
    });
  }, [filteredNodes, sortByDelay, delays]);

  // 检查延迟是否在缓存期内 - 使用 ref 避免依赖更新
  const isDelayCached = useCallback((nodeName: string) => {
    const timestamp = delayTimestampsRef.current[nodeName];
    if (!timestamp) return false;
    return Date.now() - timestamp < DELAY_CACHE_DURATION;
  }, []); // 空依赖，引用稳定

  // 测试单个节点延迟（带缓存）
  const handleTestDelay = async (nodeName: string) => {
    if (isDelayCached(nodeName)) {
      addToast({ type: "info", message: `${nodeName}: 缓存中 (${delays[nodeName]}ms)` });
      return;
    }
    const delay = await testDelay(nodeName, testUrl);
    if (delay !== undefined) {
      setDelayTimestamps(prev => ({ ...prev, [nodeName]: Date.now() }));
      addToast({ type: "success", message: `${nodeName}: ${delay}ms` });
    }
  };

  // 切换收藏状态
  const toggleFavorite = (nodeName: string) => {
    const newFavorites = favoriteNodes.includes(nodeName)
      ? favoriteNodes.filter(n => n !== nodeName)
      : [...favoriteNodes, nodeName];
    setFavoriteNodes(newFavorites);
    try {
      localStorage.setItem("miao_favorite_nodes", JSON.stringify(newFavorites));
    } catch (error) {
      console.warn("Failed to save favorite nodes:", error);
      addToast({ type: "warning", message: "收藏状态保存失败" });
    }
  };

  // 暂停/取消测速
  const cancelSpeedTest = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setSpeedTestCancelled(true);
    setTestingAllNodes(false);
    addToast({ type: "info", message: "已取消测速" });
  };

  // 使用后端批量测速 API
  const handleTestAllDelays = async () => {
    if (testingAllNodes) {
      cancelSpeedTest();
      return;
    }

    setSpeedTestCancelled(false);
    setTestingAllNodes(true);
    setSpeedTestProgress({ current: 0, total: sortedNodes.length });
    abortControllerRef.current = new AbortController();

    try {
      // 使用后端批量测速 API
      const batchDelays = await api.testBatchDelay(sortedNodes, testUrl);

      const newDelays: Record<string, number> = { ...delays, ...batchDelays };
      const newTimestamps: Record<string, number> = {};

      // 更新所有时间戳
      const now = Date.now();
      for (const nodeName of sortedNodes) {
        newTimestamps[nodeName] = now;
      }

      setDelays(newDelays);
      setDelayTimestamps(prev => ({ ...prev, ...newTimestamps }));
      setSpeedTestProgress({ current: sortedNodes.length, total: sortedNodes.length });

      const successCount = Object.values(batchDelays).filter(d => d > 0).length;
      addToast({ type: "success", message: `测速完成 (${successCount}/${sortedNodes.length} 成功)` });
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        addToast({ type: "info", message: "已取消测速" });
      } else {
        console.error("Batch delay test failed:", error);
        addToast({ type: "error", message: "批量测速失败" });
      }
    } finally {
      setTestingAllNodes(false);
      setSpeedTestProgress({ current: 0, total: 0 });
      abortControllerRef.current = null;
    }
  };

  // 并行测试所有连通性
  const testAllConnectivity = async () => {
    if (testingConnectivity) return;
    setTestingConnectivity(true);

    await Promise.all(
      CONNECTIVITY_SITES.map(async (site) => {
        try {
          const result = await api.testConnectivity(site.url);
          setConnectivityResults((prev) => ({
            ...prev,
            [site.url]: result,
          }));
        } catch {
          setConnectivityResults((prev) => ({
            ...prev,
            [site.url]: { success: false },
          }));
        }
      })
    );

    setTestingConnectivity(false);
  };

  // 重置连通性测试
  const resetConnectivity = () => {
    setConnectivityResults({});
  };

  // 自定义测速URL变更
  const handleTestUrlChange = (url: string) => {
    setTestUrl(url);
    try {
      localStorage.setItem("miao_test_url", url);
    } catch (error) {
      console.warn("Failed to save test URL:", error);
    }
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
        [site.url]: result,
      }));
    } catch {
      setConnectivityResults((prev) => ({
        ...prev,
        [site.url]: { success: false },
      }));
    } finally {
      setCurrentTestingSite(null);
    }
  };

  // 键盘快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        e.preventDefault();
        document.getElementById('node-search')?.focus();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // 渲染骨架屏（初始加载时）
  if (loading && loadingAction === "init" && Object.keys(status).length === 0) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-black">代理管理</h1>
          <p className="text-slate-500 mt-1">节点列表与延迟测试</p>
        </div>
        <SkeletonCard />
        <div className="space-y-4">
          <Skeleton className="h-8 w-48" />
          <SkeletonNodeGrid count={12} />
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-3xl font-black">代理管理</h1>
          <p className="text-slate-500 mt-1">节点列表与延迟测试</p>
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

          {normalizedDnsCandidates.length > 0 && (
            <div className="mt-3 pt-3 border-t border-slate-100 space-y-3">
              <div className="flex flex-wrap items-center justify-end gap-2">
                <select
                  className="h-9 rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-700"
                  value={dnsSelected}
                  onChange={(e) => setDnsSelected(e.target.value)}
                >
                  {normalizedDnsCandidates.map((candidate) => (
                    <option key={candidate.name} value={candidate.name}>
                      {candidate.name}
                    </option>
                  ))}
                </select>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    setPendingDns(dnsSelected);
                    setShowDnsConfirm(true);
                  }}
                  loading={loadingAction === "dns-switch"}
                  disabled={!dnsSelected || dnsSelected === dnsStatus?.active}
                >
                  切换 DNS
                </Button>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {normalizedDnsCandidates.map((candidate) => {
                  const health = candidate.health;
                  return (
                    <Badge key={candidate.name} variant={getDnsHealthVariant(health)}>
                      <span title={health ? `${candidate.name} (${health})` : candidate.name}>{candidate.name}</span>
                    </Badge>
                  );
                })}
                {dnsStatus?.last_check_secs_ago !== undefined && dnsStatus?.last_check_secs_ago !== null && (
                  <span className="text-xs text-slate-500">{dnsStatus.last_check_secs_ago}s ago</span>
                )}
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* SSH 节点管理 */}
      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-lg font-bold text-slate-900">SSH 节点</h2>
            <p className="text-slate-500 text-sm mt-1">从主机配置添加 SSH 代理节点</p>
          </div>
        </div>

        {hostsLoading ? (
          <div className="mt-4 text-center py-4 text-slate-500">
            <div className="w-6 h-6 border-2 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
            <p className="mt-2 text-sm">加载中...</p>
          </div>
        ) : hosts.length === 0 ? (
          <div className="mt-4 text-center py-6 text-slate-500 bg-slate-50 rounded-lg">
            <Server className="w-10 h-10 mx-auto mb-2 text-slate-300" />
            <p className="text-sm">暂无主机配置</p>
            <p className="text-xs mt-1">请先在"主机管理"页面添加主机配置</p>
          </div>
        ) : (
          <div className="mt-4 space-y-2">
            {hosts.map((host) => {
              const displayName = host.name ? `${host.name} (${host.host})` : host.host;
              const isExisting = existingNodeTags.has(displayName);
              return (
                <div
                  key={host.id}
                  className="flex flex-col gap-2 rounded-lg border border-slate-100 bg-white px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div className="flex items-center gap-3">
                    <Server className="w-5 h-5 text-slate-400" />
                    <div>
                      <div className="font-medium text-slate-900">{displayName}</div>
                      <div className="text-xs text-slate-500">
                        {host.username}@{host.host}:{host.port}
                        <Badge variant="info" className="ml-2">
                          {host.auth_type === "password" ? "密码" : "私钥"}
                        </Badge>
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleTestHost(host)}
                      loading={testingHostId === host.id}
                    >
                      <Zap className="w-4 h-4" />
                      测试
                    </Button>
                    {isExisting ? (
                      <>
                        <Badge variant="success" className="gap-1">
                          <Check className="w-3 h-3" />
                          已添加
                        </Badge>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => setPendingRemoveNode(displayName)}
                        >
                          删除
                        </Button>
                      </>
                    ) : (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleAddHostAsNode(host)}
                        loading={addingHostId === host.id}
                      >
                        <Plus className="w-4 h-4" />
                        添加
                      </Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-lg font-bold text-slate-900">HTTPS 连通性测试</h2>
            <p className="text-slate-500 text-sm mt-1">测试代理对外站点可用性</p>
          </div>
          <div className="flex gap-2">
            {Object.keys(connectivityResults).length > 0 && (
              <Button variant="secondary" size="sm" onClick={resetConnectivity}>
                重置
              </Button>
            )}
            <Button variant="secondary" onClick={testAllConnectivity} loading={testingConnectivity}>
              <Globe className="w-4 h-4" />
              {testingConnectivity ? "测试中..." : "测试连接"}
            </Button>
          </div>
        </div>

        <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {CONNECTIVITY_SITES.map((site) => {
              const result = connectivityResults[site.url];
              const isTesting = currentTestingSite === site.name;
              return (
                <button
                  key={site.name}
                  onClick={() => testConnectivity(site)}
                  disabled={!!currentTestingSite || testingConnectivity}
                  className="flex items-center justify-between rounded-lg border border-slate-100 bg-white px-4 py-3 text-left hover:border-indigo-200 hover-select disabled:opacity-50 transition-all"
                >
                  <div>
                    <div className="font-semibold text-slate-900">{site.name}</div>
                    <div className="text-xs text-slate-500">{site.url}</div>
                  </div>
                  <Badge variant={getConnectivityBadge(result)}>
                    {isTesting ? "测试中" : formatConnectivityDelay(result)}
                  </Badge>
                </button>
              );
            })}
          </div>
      </Card>

      <Card className="p-6" hoverEffect={false}>
        <CardHeader className="mb-3">
          <span className="text-lg font-bold text-slate-900">节点列表</span>
        </CardHeader>

        {!status.running ? (
          <div className="space-y-3">
            <div className="text-center py-8 text-slate-500 bg-slate-50 rounded-lg">
              <p>sing-box 未运行，请先启动服务以加载节点</p>
            </div>
            {manualNodeTags.length > 0 && (
              <div className="flex flex-wrap items-center gap-2">
                {manualNodeTags.map((tag) => (
                  <Badge key={tag} variant="info">
                    {tag}
                  </Badge>
                ))}
              </div>
            )}
          </div>
        ) : allNodes.length === 0 ? (
          <div className="text-center py-8 text-slate-500">
            {loading && loadingAction === "init" ? <SkeletonNodeGrid count={8} /> : "正在加载节点..."}
          </div>
        ) : (
          <>
            <div className="flex flex-wrap items-center gap-2 mb-4">
              {/* 分组筛选器 */}
              <select
                className="h-9 rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-700"
                value={selectedGroup}
                onChange={(e) => setSelectedGroup(e.target.value)}
              >
                <option value="all">全部分组 ({allNodes.length})</option>
                {proxyGroupNames.map((group) => {
                  const count = proxyGroups[group]?.all?.length || 0;
                  return (
                    <option key={group} value={group}>
                      {group} ({count})
                    </option>
                  );
                })}
              </select>

              {/* 搜索框 */}
              <div className="relative flex-1 min-w-[200px] max-w-md">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  id="node-search"
                  type="text"
                  placeholder="搜索节点... (Ctrl+F)"
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  className="w-full h-9 pl-10 pr-8 rounded-lg border border-slate-200 bg-white text-sm outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 transition-all"
                />
                {searchTerm && (
                  <button
                    onClick={() => setSearchTerm("")}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500 hover:text-slate-700"
                  >
                    清除
                  </button>
                )}
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 mb-4">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setSortByDelay(!sortByDelay)}
                title={sortByDelay ? "取消排序" : "按延迟排序"}
              >
                <RefreshCw className={`w-4 h-4 ${sortByDelay ? "text-indigo-600 animate-spin" : ""}`} />
                {sortByDelay ? "已排序" : "按延迟排序"}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={handleTestAllDelays}
                loading={testingAllNodes}
              >
                {testingAllNodes ? (
                  <>
                    <Pause className="w-4 h-4" />
                    取消
                  </>
                ) : (
                  <>
                    <Zap className="w-4 h-4" />
                    测速
                  </>
                )}
              </Button>
              {testingAllNodes && speedTestProgress.total > 0 && (
                <span className="text-sm text-slate-500">
                  {speedTestProgress.current}/{speedTestProgress.total}
                </span>
              )}
              <span className="text-sm text-slate-500 ml-auto">
                共 {sortedNodes.length} 个节点
                {favoriteNodes.length > 0 && ` (收藏: ${favoriteNodes.length})`}
              </span>
            </div>

            {/* 收藏节点优先显示 */}
            {favoriteNodes.length > 0 && (
              <div className="mb-3">
                <div className="text-xs font-medium text-slate-500 mb-2">收藏节点</div>
                <div className="flex flex-wrap gap-2">
                  {favoriteNodes.filter(n => allNodes.includes(n)).map((nodeName) => (
                    <FavoriteNodeCard
                      key={nodeName}
                      nodeName={nodeName}
                      delay={delays[nodeName]}
                      onTestDelay={handleTestDelay}
                    />
                  ))}
                </div>
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              {sortedNodes.map((nodeName) => (
                <NodeCard
                  key={nodeName}
                  nodeName={nodeName}
                  delay={delays[nodeName]}
                  isFavorite={favoriteNodes.includes(nodeName)}
                  isCached={isDelayCached(nodeName)}
                  onTestDelay={handleTestDelay}
                  onToggleFavorite={toggleFavorite}
                />
              ))}
            </div>
            {searchTerm && sortedNodes.length === 0 && (
              <div className="mt-3 text-sm text-slate-500">没有匹配的节点</div>
            )}

            <div className="flex items-center gap-4 text-sm text-slate-600 mt-6 pt-4 border-t border-slate-100">
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

      <ConfirmModal
        isOpen={pendingRemoveNode !== null}
        onClose={() => setPendingRemoveNode(null)}
        onConfirm={async () => {
          if (!pendingRemoveNode) return;
          await handleRemoveSshNode(pendingRemoveNode);
          setPendingRemoveNode(null);
        }}
        title="确认删除节点"
        message={pendingRemoveNode ? `确定要删除节点 ${pendingRemoveNode} 吗？` : ""}
        variant="danger"
        loading={removingNode}
      />

      {/* DNS 切换确认对话框 */}
      <ConfirmModal
        isOpen={showDnsConfirm}
        onClose={() => setShowDnsConfirm(false)}
        onConfirm={() => {
          switchDnsActive(pendingDns);
          setShowDnsConfirm(false);
        }}
        title="确认切换 DNS"
        message={`确定要将 DNS 切换到 ${pendingDns} 吗？`}
        variant="warning"
        loading={loadingAction === "dns-switch"}
      />
    </div>
  );
}
