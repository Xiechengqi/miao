"use client";

import { useEffect, useMemo, useState, useRef, useCallback } from "react";
import { Card, CardHeader, CardContent, Button, Badge, TogglePower, Skeleton, SkeletonCard, ConfirmModal, Modal } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { useProxies, useStatus, useTraffic } from "@/hooks";
import { api, getSingBoxLogsWsUrl } from "@/lib/api";
import { formatUptime, formatSpeed, cn } from "@/lib/utils";
import { RefreshCw, Zap, Activity, Clock, Cpu, Wifi, Globe, Server, Plus, Check, Download, FileText } from "lucide-react";
import { Host, ManualNode, LogEntry, Subscription } from "@/types/api";
import { ansiToHtml, stripLogPrefix } from "@/lib/ansi";

const CONNECTIVITY_SITES = [
  { name: "Google", url: "https://www.google.com" },
  { name: "GitHub", url: "https://github.com" },
  { name: "YouTube", url: "https://youtube.com" },
  { name: "Twitter", url: "https://x.com" },
  { name: "Telegram", url: "https://telegram.org" },
  { name: "Baidu", url: "https://baidu.com" },
];

const CONNECTIVITY_STORAGE_KEY = "miao_connectivity_results";
const HOST_TEST_STORAGE_KEY = "miao_proxy_host_test_results";

type ConnectivityResult = {
  success: boolean;
  latency_ms?: number;
};

type ConnectivityCache = {
  results: Record<string, ConnectivityResult>;
};

type SSHHostTestResult = {
  success: boolean;
  latency_ms?: number;
  error?: string | null;
};

type HostTestCache = {
  results: Record<string, SSHHostTestResult>;
};

// 升级日志类型
type UpgradeLogEntry = {
  step: number;
  total_steps: number;
  message: string;
  level: string;
  progress?: number;
};

function SingBoxLogModal({
  isOpen,
  onClose,
}: {
  isOpen: boolean;
  onClose: () => void;
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
      const data = await api.getSingBoxLogs(200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load sing-box logs:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  const connectWs = useCallback(() => {
    let wsUrl: string;
    try {
      wsUrl = getSingBoxLogsWsUrl();
    } catch (error) {
      console.warn("Cannot connect to sing-box logs WebSocket:", error);
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
        console.error("Failed to parse sing-box log:", e);
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
  }, []);

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
    <Modal isOpen={isOpen} onClose={onClose} title="sing-box 日志" size="lg">
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

// 默认 DNS 候选列表
const DEFAULT_DNS_CANDIDATES = ["doh-cf", "doh-google"];

export default function ProxiesPage() {
  const {
    setLoading,
    loading,
    addToast,
    proxyGroups,
    setProxyGroups,
    setNodes: setProxyNodes,
    setStatus,
    dnsStatus,
    setDnsStatus,
  } = useStore();
  const { fetchProxies } = useProxies();
  const { status, loadingAction, switchDnsActive, toggleService } = useStatus();
  const { traffic } = useTraffic();
  const [connectivityResults, setConnectivityResults] = useState<Record<string, ConnectivityResult>>({});
  const [testingConnectivity, setTestingConnectivity] = useState(false);
  const [currentTestingSite, setCurrentTestingSite] = useState<string | null>(null);
  const [pendingRemoveNode, setPendingRemoveNode] = useState<string | null>(null);
  const [removingNode, setRemovingNode] = useState(false);

  // SSH 节点相关状态
  const [hosts, setHosts] = useState<Host[]>([]);
  const [hostsLoading, setHostsLoading] = useState(false);
  const [addingHostId, setAddingHostId] = useState<string | null>(null);
  const [testingHostId, setTestingHostId] = useState<string | null>(null);
  const [hostTestResults, setHostTestResults] = useState<Record<string, SSHHostTestResult>>({});
  const [existingNodeTags, setExistingNodeTags] = useState<Set<string>>(() => new Set());
  const [manualNodes, setManualNodes] = useState<ManualNode[]>([]);
  const [switchingNode, setSwitchingNode] = useState(false);

  // 订阅管理状态
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [subscriptionsLoading, setSubscriptionsLoading] = useState(false);
  const [showAddSubscription, setShowAddSubscription] = useState(false);
  const [newSubscriptionUrl, setNewSubscriptionUrl] = useState("");
  const [newSubscriptionName, setNewSubscriptionName] = useState("");
  const [addingSubscription, setAddingSubscription] = useState(false);
  const [deletingSubId, setDeletingSubId] = useState<string | null>(null);
  const [reloadingSubId, setReloadingSubId] = useState<string | null>(null);
  const [editingSubscription, setEditingSubscription] = useState<Subscription | null>(null);
  const [editSubscriptionUrl, setEditSubscriptionUrl] = useState("");
  const [editSubscriptionName, setEditSubscriptionName] = useState("");
  const [updatingSubscription, setUpdatingSubscription] = useState(false);

  // DNS 切换确认对话框
  const [showDnsConfirm, setShowDnsConfirm] = useState(false);
  const [pendingDns, setPendingDns] = useState<string>("");
  const dnsCandidates = useMemo(() => dnsStatus?.candidates || DEFAULT_DNS_CANDIDATES, [dnsStatus]);
  const activeDns = useMemo(() => dnsStatus?.active || dnsCandidates[0] || "", [dnsStatus, dnsCandidates]);
  const needsRestart = !!status.running && !!status.pending_restart;

  // Binary 安装状态
  const [singBoxInstalled, setSingBoxInstalled] = useState<boolean | null>(null);
  const [installingSingBox, setInstallingSingBox] = useState(false);

  // sing-box 升级状态
  const [showUpgradeModal, setShowUpgradeModal] = useState(false);
  const [upgrading, setUpgrading] = useState(false);
  const [upgradeLogs, setUpgradeLogs] = useState<UpgradeLogEntry[]>([]);
  const [upgradeProgress, setUpgradeProgress] = useState(0);
  const [upgradeStatus, setUpgradeStatus] = useState<"running" | "success" | "error">("running");
  const upgradeLogsRef = useRef<HTMLDivElement>(null);
  const [showSingBoxLogModal, setShowSingBoxLogModal] = useState(false);

  // 当前选中的代理节点
  const currentNode = useMemo(() => {
    return proxyGroups?.proxy?.now || null;
  }, [proxyGroups]);

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

    // 加载 SSH 节点测试结果
    const savedHostTests = safeGetItem<HostTestCache>(HOST_TEST_STORAGE_KEY, { results: {} });
    if (isMounted && Object.keys(savedHostTests.results).length > 0) {
      setHostTestResults(savedHostTests.results);
    }

    const loadData = async () => {
      if (!isMounted) return;
      setLoading(true, "init");
      let statusData = null;
      try {
        // 先检查 binary 状态
        const binStatus = await api.getBinariesStatus();
        if (!isMounted) return;
        setSingBoxInstalled(binStatus.sing_box.installed);

        // 如果 sing-box 未安装，不继续加载其他数据
        if (!binStatus.sing_box.installed) {
          setLoading(false);
          return;
        }

        // 加载核心状态
        [statusData] = await Promise.all([
          api.getStatus(),
        ]);
        if (!isMounted) return;
        setStatus(statusData);

        // 然后并行加载其他数据
        const [{ proxies, nodes: nodeList }] = await Promise.all([
          api.getProxies(),
        ]);
        if (!isMounted) return;
        setProxyGroups(proxies);
        setProxyNodes(nodeList);
      } catch (error) {
        if (!isMounted) return;
        console.error("Failed to load data:", error);
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

  // 同步 SSH 节点测试结果到 localStorage
  useEffect(() => {
    try {
      if (Object.keys(hostTestResults).length === 0) {
        localStorage.removeItem(HOST_TEST_STORAGE_KEY);
        return;
      }
      const payload: HostTestCache = { results: hostTestResults };
      localStorage.setItem(HOST_TEST_STORAGE_KEY, JSON.stringify(payload));
    } catch (error) {
      console.warn("Failed to save host test results:", error);
    }
  }, [hostTestResults]);

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

  // 加载订阅列表
  useEffect(() => {
    const loadSubscriptions = async () => {
      setSubscriptionsLoading(true);
      try {
        const subs = await api.getSubscriptions();
        setSubscriptions(subs);
      } catch (error) {
        console.error("Failed to load subscriptions:", error);
      } finally {
        setSubscriptionsLoading(false);
      }
    };
    loadSubscriptions();
  }, []);

  // 添加主机为 SSH 节点
  const handleAddHostAsNode = async (host: Host) => {
    setAddingHostId(host.id);
    try {
      const detail = await api.getHost(host.id);
      const tag = detail.name ? `${detail.name} (${detail.host})` : detail.host;
      const nodeData: Partial<ManualNode> & { tag: string; node_type: "ssh"; server: string; server_port: number; user: string } = {
        tag,
        node_type: "ssh",
        server: detail.host,
        server_port: detail.port || 22,
        user: detail.username,
      };
      // Use private key if available, otherwise use password
      if (detail.auth_type === "private_key_path") {
        if (!detail.private_key_path) {
          addToast({ type: "error", message: "主机缺少私钥路径" });
          return;
        }
        nodeData.private_key_path = detail.private_key_path;
        if (detail.private_key_passphrase) {
          nodeData.private_key_passphrase = detail.private_key_passphrase;
        }
      } else {
        if (!detail.password) {
          addToast({ type: "error", message: "主机缺少密码" });
          return;
        }
        nodeData.password = detail.password;
      }
      await api.createNode(nodeData as ManualNode);
      addToast({
        type: "success",
        message: status.running
          ? `SSH 节点 ${tag} 已添加（重启后生效）`
          : `SSH 节点 ${tag} 已添加（启动后生效）`,
      });
      const nodesData = await api.getNodes().catch(() => [] as ManualNode[]);
      setManualNodes(nodesData);
      if (status.running) {
        // 服务运行中可同步代理信息
        fetchProxies(true);
        const latestStatus = await api.getStatus().catch(() => null);
        if (latestStatus) {
          setStatus(latestStatus);
        }
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
      const result = await api.testPing(host.id);
      setHostTestResults(prev => ({
        ...prev,
        [host.id]: { success: result.success, latency_ms: result.avg_latency_ms, error: result.error }
      }));
      if (result.success) {
        addToast({ type: "success", message: `Ping 成功 (${result.avg_latency_ms?.toFixed(0) ?? 0}ms)` });
      } else {
        addToast({ type: "error", message: result.error || "Ping 失败" });
      }
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "测试失败" });
    } finally {
      setTestingHostId(null);
    }
  };

  // 切换当前使用的节点
  const handleSwitchNode = async (nodeTag: string) => {
    if (!status.running) {
      addToast({ type: "warning", message: "请先启动 sing-box" });
      return;
    }
    if (status.pending_restart) {
      addToast({ type: "warning", message: "请先重启 sing-box 使节点生效" });
      return;
    }
    setSwitchingNode(true);
    try {
      await api.switchProxy("proxy", nodeTag);
      await fetchProxies(true);
      addToast({ type: "success", message: `已切换到 ${nodeTag}` });
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "切换节点失败" });
    } finally {
      setSwitchingNode(false);
    }
  };

  // 删除 SSH 节点
  const handleRemoveSshNode = async (tag: string) => {
    try {
      setRemovingNode(true);
      await api.deleteNode(tag);
      addToast({
        type: "success",
        message: status.running ? "节点已删除（重启后生效）" : "节点已删除（启动后生效）",
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
        const [latestStatus] = await Promise.all([
          api.getStatus().catch(() => null),
          fetchProxies(true),
          updateTags(),
        ]);
        if (latestStatus) {
          setStatus(latestStatus);
        }
      } else {
        await updateTags();
      }
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除节点失败" });
    } finally {
      setRemovingNode(false);
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

  // 安装 sing-box
  const handleInstallSingBox = async () => {
    setInstallingSingBox(true);
    try {
      await api.installSingBox();
      setSingBoxInstalled(true);
      addToast({ type: "success", message: "sing-box 安装成功" });
      // 重新加载页面数据
      const statusData = await api.getStatus();
      setStatus(statusData);
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "安装失败",
      });
    } finally {
      setInstallingSingBox(false);
    }
  };

  // 升级 sing-box
  const handleUpgradeSingBox = () => {
    if (upgrading) return;
    if (!confirm("确定要更新 sing-box 吗？")) return;

    setUpgrading(true);
    setShowUpgradeModal(true);
    setUpgradeLogs([]);
    setUpgradeProgress(0);
    setUpgradeStatus("running");

    const token = localStorage.getItem("miao_token");
    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${wsProtocol}//${window.location.host}/api/binaries/upgrade/sing-box/ws?token=${token}`;

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

  // sing-box 未安装提示
  if (singBoxInstalled === false) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-black">代理管理</h1>
          <p className="text-slate-500 mt-1">管理SSH节点代理</p>
        </div>
        <Card className="p-6">
          <div className="text-center py-8">
            <Server className="w-16 h-16 mx-auto text-slate-300 mb-4" />
            <h2 className="text-xl font-bold text-slate-700 mb-2">sing-box 未安装</h2>
            <p className="text-slate-500 mb-6">
              当前环境没有 sing-box 程序，请点击下方按钮安装
            </p>
            <Button
              onClick={handleInstallSingBox}
              disabled={installingSingBox}
              className="px-6"
            >
              {installingSingBox ? (
                <>
                  <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
                  安装中...
                </>
              ) : (
                <>
                  <Plus className="w-4 h-4 mr-2" />
                  安装 sing-box
                </>
              )}
            </Button>
          </div>
        </Card>
      </div>
    );
  }

  // 渲染骨架屏（初始加载时）
  if (loading && loadingAction === "init" && Object.keys(status).length === 0) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-black">代理管理</h1>
          <p className="text-slate-500 mt-1">管理SSH节点代理</p>
        </div>
        <SkeletonCard />
        <SkeletonCard />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-3xl font-black">代理管理</h1>
          {singBoxInstalled && (
            <span className="px-2 py-0.5 text-xs font-medium bg-emerald-100 text-emerald-700 rounded">
              sing-box 已安装
            </span>
          )}
        </div>
        {singBoxInstalled && (
          <div className="flex items-center gap-2">
            <span
              className="inline-flex"
              title={status.running ? "查看 sing-box 运行日志" : "sing-box 未运行，无法查看日志"}
            >
              <Button
                variant="secondary"
                onClick={() => setShowSingBoxLogModal(true)}
                disabled={!status.running}
              >
                <FileText className="w-4 h-4" />
                日志
              </Button>
            </span>
            <Button variant="secondary" onClick={handleUpgradeSingBox} disabled={upgrading}>
              <Download className="w-4 h-4" />
              {upgrading ? "更新中..." : "更新 sing-box"}
            </Button>
          </div>
        )}
      </div>

      <Card className="p-4">
        <CardHeader className="mb-3">
          <Activity className="w-5 h-5 text-indigo-600" />
          <span className="text-lg font-bold text-slate-900">Sing-box 状态</span>
          <div className="ml-auto flex items-center gap-3">
            {status.running && (
              <>
                <div className="flex items-center gap-1.5 px-2 py-1 rounded-lg bg-emerald-50 text-xs">
                  <Zap className="w-3.5 h-3.5 text-emerald-600" />
                  <span className="font-mono text-emerald-700">{formatSpeed(traffic.up)}</span>
                </div>
                <div className="flex items-center gap-1.5 px-2 py-1 rounded-lg bg-sky-50 text-xs">
                  <RefreshCw className="w-3.5 h-3.5 text-sky-600" />
                  <span className="font-mono text-sky-700">{formatSpeed(traffic.down)}</span>
                </div>
              </>
            )}
            <TogglePower
              running={status.running}
              loading={loading && (loadingAction === "start" || loadingAction === "stop" || loadingAction === "restart")}
              onToggle={toggleService}
              size="md"
              mode={needsRestart ? "restart" : "start-stop"}
            />
          </div>
        </CardHeader>

        <CardContent>
          <div className="flex flex-wrap items-center gap-3">
            <Badge variant={status.running ? "success" : "error"} dot>
              {status.running ? "运行中" : "已停止"}
            </Badge>
            {needsRestart && (
              <Badge variant="warning">
                需重启生效
              </Badge>
            )}
            {needsRestart && (
              <span className="text-xs text-amber-600">之前操作需要重启生效</span>
            )}

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
              <select
                className="bg-transparent text-slate-700 font-mono text-xs cursor-pointer outline-none"
                value={activeDns}
                onChange={(e) => {
                  if (e.target.value && e.target.value !== activeDns) {
                    setPendingDns(e.target.value);
                    setShowDnsConfirm(true);
                  }
                }}
                disabled={loading || loadingAction === "dns-switch"}
              >
                {dnsCandidates.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </div>
          </div>

        </CardContent>
      </Card>

      {/* 订阅管理 */}
      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-lg font-bold text-slate-900">订阅管理</h2>
            <p className="text-slate-500 text-sm mt-1">管理代理节点订阅源</p>
          </div>
          <Button
            variant="primary"
            size="sm"
            onClick={() => setShowAddSubscription(true)}
          >
            <Plus className="w-4 h-4" />
            添加订阅
          </Button>
        </div>

        {subscriptionsLoading ? (
          <div className="mt-4 text-center py-4 text-slate-500">
            <div className="w-6 h-6 border-2 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
            <p className="mt-2 text-sm">加载中...</p>
          </div>
        ) : subscriptions.length === 0 ? (
          <div className="mt-4 text-center py-6 text-slate-500 bg-slate-50 rounded-lg">
            <Globe className="w-10 h-10 mx-auto mb-2 text-slate-300" />
            <p className="text-sm">暂无订阅</p>
            <p className="text-xs mt-1">点击"添加订阅"按钮添加代理节点订阅源</p>
          </div>
        ) : (
          <div className="mt-4 space-y-2">
            {subscriptions.map((sub) => (
              <div
                key={sub.id}
                className="flex flex-col gap-2 rounded-lg border border-slate-100 bg-white px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="flex items-center gap-3 flex-1 min-w-0">
                  <Globe className="w-5 h-5 text-slate-400 flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="font-medium text-slate-900 truncate">
                      {sub.name || "未命名订阅"}
                    </div>
                    <div className="text-xs text-slate-500 truncate">{sub.source.url}</div>
                  </div>
                  <Badge variant={sub.enabled ? "success" : "default"}>
                    {sub.enabled ? "已启用" : "已禁用"}
                  </Badge>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={async () => {
                      try {
                        const payload: any = {
                          enabled: !sub.enabled,
                          url: sub.source.url
                        };
                        if (sub.name) {
                          payload.name = sub.name;
                        }
                        await api.updateSubscription(sub.id, payload);
                        setSubscriptions(subs => subs.map(s => s.id === sub.id ? { ...s, enabled: !s.enabled } : s));
                        addToast({ type: "success", message: sub.enabled ? "订阅已禁用" : "订阅已启用" });
                      } catch (error) {
                        addToast({ type: "error", message: "操作失败" });
                      }
                    }}
                  >
                    {sub.enabled ? "禁用" : "启用"}
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      setEditingSubscription(sub);
                      setEditSubscriptionUrl(sub.source.url);
                      setEditSubscriptionName(sub.name || "");
                    }}
                  >
                    编辑
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={async () => {
                      if (!confirm("确定要删除此订阅吗？")) return;
                      setDeletingSubId(sub.id);
                      try {
                        await api.deleteSubscription(sub.id);
                        setSubscriptions(subs => subs.filter(s => s.id !== sub.id));
                        addToast({ type: "success", message: "订阅已删除" });
                      } catch (error) {
                        addToast({ type: "error", message: "删除失败" });
                      } finally {
                        setDeletingSubId(null);
                      }
                    }}
                    loading={deletingSubId === sub.id}
                  >
                    删除
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* 添加订阅对话框 */}
      <Modal
        isOpen={showAddSubscription}
        onClose={() => {
          setShowAddSubscription(false);
          setNewSubscriptionUrl("");
          setNewSubscriptionName("");
        }}
        title="添加订阅"
      >
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-slate-700 mb-1">
              订阅名称（可选）
            </label>
            <input
              type="text"
              value={newSubscriptionName}
              onChange={(e) => setNewSubscriptionName(e.target.value)}
              placeholder="例如：我的订阅"
              className="w-full px-3 py-2 border border-slate-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-slate-700 mb-1">
              订阅内容 *
            </label>
            <textarea
              value={newSubscriptionUrl}
              onChange={(e) => setNewSubscriptionUrl(e.target.value)}
              placeholder="粘贴订阅内容（base64编码的节点列表）"
              rows={6}
              className="w-full px-3 py-2 border border-slate-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 resize-none font-mono text-sm"
            />
          </div>
          <div className="flex justify-end gap-2">
            <Button
              variant="secondary"
              onClick={() => {
                setShowAddSubscription(false);
                setNewSubscriptionUrl("");
                setNewSubscriptionName("");
              }}
            >
              取消
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                if (!newSubscriptionUrl.trim()) {
                  addToast({ type: "error", message: "请输入订阅内容" });
                  return;
                }
                setAddingSubscription(true);
                try {
                  const newSub = await api.createSubscription({
                    url: newSubscriptionUrl.trim(),
                    name: newSubscriptionName.trim() || undefined,
                    enabled: true,
                  });
                  setSubscriptions(subs => [...subs, newSub]);
                  setShowAddSubscription(false);
                  setNewSubscriptionUrl("");
                  setNewSubscriptionName("");
                  addToast({ type: "success", message: "订阅已添加" });
                } catch (error) {
                  addToast({ type: "error", message: error instanceof Error ? error.message : "添加失败" });
                } finally {
                  setAddingSubscription(false);
                }
              }}
              loading={addingSubscription}
              disabled={!newSubscriptionUrl.trim()}
            >
              添加
            </Button>
          </div>
        </div>
      </Modal>

      {/* 编辑订阅对话框 */}
      <Modal
        isOpen={editingSubscription !== null}
        onClose={() => {
          setEditingSubscription(null);
          setEditSubscriptionUrl("");
          setEditSubscriptionName("");
        }}
        title="编辑订阅"
      >
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-slate-700 mb-1">
              订阅名称（可选）
            </label>
            <input
              type="text"
              value={editSubscriptionName}
              onChange={(e) => setEditSubscriptionName(e.target.value)}
              placeholder="例如：我的订阅"
              className="w-full px-3 py-2 border border-slate-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-slate-700 mb-1">
              订阅内容 *
            </label>
            <textarea
              value={editSubscriptionUrl}
              onChange={(e) => setEditSubscriptionUrl(e.target.value)}
              placeholder="粘贴订阅内容（base64编码的节点列表）"
              rows={6}
              className="w-full px-3 py-2 border border-slate-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 resize-none font-mono text-sm"
            />
          </div>
          <div className="flex justify-end gap-2">
            <Button
              variant="secondary"
              onClick={() => {
                setEditingSubscription(null);
                setEditSubscriptionUrl("");
                setEditSubscriptionName("");
              }}
            >
              取消
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                if (!editingSubscription || !editSubscriptionUrl.trim()) {
                  addToast({ type: "error", message: "请输入订阅内容" });
                  return;
                }
                setUpdatingSubscription(true);
                try {
                  const updated = await api.updateSubscription(editingSubscription.id, {
                    url: editSubscriptionUrl.trim(),
                    name: editSubscriptionName.trim() || undefined,
                    enabled: editingSubscription.enabled,
                  });
                  setSubscriptions(subs => subs.map(s => s.id === updated.id ? updated : s));
                  setEditingSubscription(null);
                  setEditSubscriptionUrl("");
                  setEditSubscriptionName("");
                  addToast({ type: "success", message: "订阅已更新" });
                } catch (error) {
                  addToast({ type: "error", message: error instanceof Error ? error.message : "更新失败" });
                } finally {
                  setUpdatingSubscription(false);
                }
              }}
              loading={updatingSubscription}
              disabled={!editSubscriptionUrl.trim()}
            >
              保存
            </Button>
          </div>
        </div>
      </Modal>

      {/* 节点列表 */}
      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-lg font-bold text-slate-900">代理节点</h2>
            <p className="text-slate-500 text-sm mt-1">从订阅解析的代理节点列表</p>
          </div>
        </div>

        {proxyGroups?.proxy?.all && proxyGroups.proxy.all.length > 0 ? (
          <div className="mt-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {proxyGroups.proxy.all.map((nodeName) => {
              const isActive = currentNode === nodeName;
              return (
                <button
                  key={nodeName}
                  onClick={() => handleSwitchNode(nodeName)}
                  disabled={switchingNode || !status.running}
                  className={cn(
                    "flex items-center justify-between rounded-lg border px-4 py-3 text-left transition-all",
                    isActive
                      ? "border-indigo-500 bg-indigo-50"
                      : "border-slate-100 bg-white hover:border-indigo-200",
                    (switchingNode || !status.running) && "opacity-50 cursor-not-allowed"
                  )}
                >
                  <span className={cn("font-medium", isActive ? "text-indigo-700" : "text-slate-900")}>
                    {nodeName}
                  </span>
                  {isActive && <Check className="w-4 h-4 text-indigo-600" />}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="mt-4 text-center py-6 text-slate-500 bg-slate-50 rounded-lg">
            <p className="text-sm">暂无节点</p>
            <p className="text-xs mt-1">请添加订阅或等待订阅加载</p>
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

      {/* sing-box 升级弹框 */}
      <Modal
        isOpen={showUpgradeModal}
        onClose={() => {
          if (upgradeStatus !== "running") {
            setShowUpgradeModal(false);
          }
        }}
        title="更新 sing-box"
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

      <SingBoxLogModal
        isOpen={showSingBoxLogModal}
        onClose={() => setShowSingBoxLogModal(false)}
      />
    </div>
  );
}
