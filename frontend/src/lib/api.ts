import {
  ApiResponse,
  Status,
  SystemInfo,
  SystemStatus,
  SystemMetricsResponse,
  DnsStatus,
  Node,
  ManualNode,
  ProxyGroup,
  SyncConfig,
  SyncLogEntry,
  TcpTunnel,
  Terminal,
  VncSession,
  App,
  AppTemplate,
  TrafficData,
  VersionInfo,
  Host,
  HostTestResult,
} from "@/types/api";

// 重试配置
const DEFAULT_MAX_RETRIES = 2;
const DEFAULT_RETRY_DELAY = 500; // ms

/**
 * 带重试的异步函数
 */
async function withRetry<T>(
  fn: () => Promise<T>,
  maxRetries = DEFAULT_MAX_RETRIES,
  retryDelay = DEFAULT_RETRY_DELAY
): Promise<T> {
  let lastError: Error | undefined;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;
      // 如果还有重试次数，等待后重试
      if (attempt < maxRetries) {
        await new Promise(resolve => setTimeout(resolve, retryDelay * (attempt + 1)));
      }
    }
  }

  throw lastError;
}
// 前端和后端在同一个域名端口下，使用空字符串表示相对路径
const API_BASE: string = "";

class ApiClient {
  private token: string | null = null;

  constructor() {
    if (typeof window !== "undefined") {
      this.token = localStorage.getItem("miao_token");
    }
  }

  setToken(token: string) {
    this.token = token;
    if (typeof window !== "undefined") {
      localStorage.setItem("miao_token", token);
    }
  }

  clearToken() {
    this.token = null;
    if (typeof window !== "undefined") {
      localStorage.removeItem("miao_token");
    }
  }

  private async fetch<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
    const headers: HeadersInit = {
      "Content-Type": "application/json",
      ...options.headers,
    };

    if (this.token) {
      (headers as Record<string, string>)["Authorization"] = `Bearer ${this.token}`;
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers,
    });

    // Handle 401 - Unauthorized
    if (response.status === 401) {
      this.clearToken();
      if (typeof window !== "undefined") {
        window.location.href = "/login";
      }
      throw new Error("Unauthorized");
    }

    if (!response.ok) {
      const error = await response.json().catch(() => ({ error: response.statusText }));
      throw new Error(error?.error || error?.message || `API Error: ${response.statusText}`);
    }

    const data = await response.json();

    // Check for application-level errors (success: false)
    if (data && typeof data.success === 'boolean' && !data.success) {
      throw new Error(data.message || 'API request failed');
    }

    return data;
  }

  // Auth
  async login(password: string): Promise<{ token: string }> {
    const res = await this.fetch<{ data: { token: string } }>("/api/login", {
      method: "POST",
      body: JSON.stringify({ password }),
    });
    this.setToken(res.data.token);
    return res.data;
  }

  async setup(password: string): Promise<void> {
    await this.fetch("/api/setup/init", {
      method: "POST",
      body: JSON.stringify({ password }),
    });
  }

  async checkSetupRequired(): Promise<{ required: boolean }> {
    const res = await this.fetch<{ data: { required: boolean } }>("/api/setup/status");
    return res.data;
  }

  // Status
  async getStatus(): Promise<Status> {
    const res = await this.fetch<{ data: Status }>("/api/status");
    return res.data;
  }

  async getSystemInfo(): Promise<SystemInfo> {
    const res = await this.fetch<{ data: SystemInfo }>("/api/system/info");
    return res.data;
  }

  async getSystemStatus(): Promise<SystemStatus> {
    const res = await this.fetch<{ data: SystemStatus }>("/api/system/status");
    return res.data;
  }

  async getSystemMetrics(range: string, step?: string): Promise<SystemMetricsResponse> {
    const params = new URLSearchParams({ range });
    if (step) {
      params.set("step", step);
    }
    const res = await this.fetch<{
      data: {
        range: string;
        step: string;
        series: Array<{
          timestamp: number;
          cpu_percent: number;
          memory_used_kb: number;
          gpu_percent?: number;
          disk_used_bytes: number;
          disk_total_bytes: number;
        }>;
      };
    }>(
      `/api/system/metrics?${params.toString()}`
    );
    return {
      range: res.data.range,
      step: res.data.step,
      series: res.data.series.map((point) => ({
        timestamp: point.timestamp,
        cpuPercent: point.cpu_percent,
        memoryUsedKb: point.memory_used_kb,
        gpuPercent: point.gpu_percent,
        diskUsedBytes: point.disk_used_bytes,
        diskTotalBytes: point.disk_total_bytes,
      })),
    };
  }

  async startService(): Promise<void> {
    await this.fetch("/api/service/start", {
      method: "POST",
    });
  }

  async stopService(): Promise<void> {
    await this.fetch("/api/service/stop", {
      method: "POST",
    });
  }

  // DNS
  async getDnsStatus(): Promise<DnsStatus> {
    const res = await this.fetch<{ data: DnsStatus }>("/api/dns/status");
    return res.data;
  }

  async checkDns(): Promise<void> {
    await this.fetch("/api/dns/check", {
      method: "POST",
    });
  }

  async switchDns(name: string): Promise<void> {
    await this.fetch("/api/dns/switch", {
      method: "POST",
      body: JSON.stringify({ tag: name }),
    });
  }

  // Nodes
  async getNodes(): Promise<ManualNode[]> {
    const res = await this.fetch<{ data: ManualNode[] }>("/api/nodes");
    return res.data;
  }

  async getNode(tag: string): Promise<ManualNode> {
    const res = await this.fetch<{ data: ManualNode }>(`/api/nodes/${encodeURIComponent(tag)}`);
    return res.data;
  }

  async createNode(config: ManualNode): Promise<void> {
    await this.fetch("/api/nodes", {
      method: "POST",
      body: JSON.stringify(config),
    });
  }

  async updateNode(tag: string, config: Partial<ManualNode>): Promise<void> {
    await this.fetch(`/api/nodes/${encodeURIComponent(tag)}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async deleteNode(tag: string): Promise<void> {
    await this.fetch("/api/nodes", {
      method: "DELETE",
      body: JSON.stringify({ tag }),
    });
  }

  async testNode(server: string, server_port: number, timeout_ms: number = 3000): Promise<{ latency_ms: number }> {
    const res = await this.fetch<{ data: { latency_ms: number } }>("/api/node-test", {
      method: "POST",
      body: JSON.stringify({ server, server_port, timeout_ms }),
    });
    return res.data;
  }

  // Connectivity
  async testConnectivity(url: string): Promise<{ success: boolean; latency_ms?: number }> {
    const res = await this.fetch<{ data: { success: boolean; latency_ms?: number } }>("/api/connectivity", {
      method: "POST",
      body: JSON.stringify({ url }),
    });
    return res.data;
  }

  // Proxies
  async getProxies(): Promise<{ proxies: Record<string, ProxyGroup>; nodes: Node[] }> {
    const res = await this.fetch<{ data: { proxies: Record<string, ProxyGroup>; nodes: Node[] } }>("/api/clash/proxies");
    return res.data;
  }

  async testDelay(nodeName: string, url?: string, signal?: AbortSignal): Promise<number> {
    let endpoint = `/api/clash/proxies/${encodeURIComponent(nodeName)}/delay`;
    const params = new URLSearchParams();
    if (url) params.set("url", url);
    if (params.toString()) {
      endpoint += `?${params.toString()}`;
    }

    return withRetry(async () => {
      const res = await this.fetch<{ data: number }>(endpoint, {
        method: "GET",
        signal,
      });
      return res.data;
    }, 2, 300);
  }

  async testBatchDelay(nodes: string[], url?: string, timeout?: number): Promise<Record<string, number>> {
    return withRetry(async () => {
      const res = await this.fetch<{
        data: {
          results: Array<{ node: string; delay: number | null; success: boolean }>;
        };
      }>("/api/clash/proxies/delay", {
        method: "POST",
        body: JSON.stringify({ nodes, url, timeout }),
      });
      const result: Record<string, number> = {};
      for (const item of res.data.results) {
        result[item.node] = item.delay ?? 0;
      }
      return result;
    }, 2, 500);
  }

  async switchProxy(group: string, name: string): Promise<void> {
    await this.fetch(`/api/clash/proxies/${encodeURIComponent(group)}`, {
      method: "PUT",
      body: JSON.stringify({ name }),
    });
  }

  // Sync (Backup)
  async getSyncs(): Promise<SyncConfig[]> {
    const res = await this.fetch<{
      data: {
        items: Array<{
          id: string;
          name?: string | null;
          enabled: boolean;
          local_paths: Array<{ path: string }>;
          remote_path?: string | null;
          ssh: { host: string; port: number; username: string };
          auth?: { type: "password"; password?: string | null } | { type: "private_key_path"; path: string };
          options?: SyncConfig["options"];
          schedule?: SyncConfig["schedule"];
          status?: SyncConfig["status"];
        }>;
      };
    }>("/api/syncs");

    return res.data.items.map((item) => {
      const auth = item.auth
        ? item.auth.type === "password"
          ? { type: "password" as const, password: item.auth.password ?? null }
          : undefined
        : undefined;

      return {
        id: item.id,
        name: item.name ?? null,
        enabled: item.enabled,
        local_paths: (item.local_paths || []).map((entry) => entry.path),
        remote_path: item.remote_path ?? null,
        ssh: item.ssh,
        auth,
        options: item.options,
        schedule: item.schedule,
        status: item.status,
      };
    });
  }

  async createSync(config: Record<string, unknown>): Promise<SyncConfig> {
    const res = await this.fetch<{ data: SyncConfig }>("/api/syncs", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateSync(id: string, config: Record<string, unknown>): Promise<void> {
    await this.fetch(`/api/syncs/${id}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async deleteSync(id: string): Promise<void> {
    await this.fetch(`/api/syncs/${id}`, {
      method: "DELETE",
    });
  }

  async startSync(id: string): Promise<void> {
    await this.fetch(`/api/syncs/${id}/start`, {
      method: "POST",
    });
  }

  async stopSync(id: string): Promise<void> {
    await this.fetch(`/api/syncs/${id}/stop`, {
      method: "POST",
    });
  }

  async testSync(id: string): Promise<void> {
    await this.fetch(`/api/syncs/${id}/test`, {
      method: "POST",
    });
  }

  async runSync(id: string): Promise<void> {
    await this.fetch(`/api/syncs/${id}/run`, {
      method: "POST",
    });
  }

  async toggleScheduleSync(id: string): Promise<{ enabled: boolean }> {
    const res = await this.fetch<{ data: { enabled: boolean } }>(`/api/syncs/${id}/schedule`, {
      method: "POST",
    });
    return res.data;
  }

  async getSyncLogs(id: string, limit?: number): Promise<SyncLogEntry[]> {
    const params = new URLSearchParams();
    if (limit) params.set("limit", limit.toString());
    const res = await this.fetch<{ data: SyncLogEntry[] }>(`/api/syncs/${id}/logs?${params.toString()}`);
    return res.data;
  }

  // TCP Tunnels
  async getTcpTunnels(): Promise<{ supported: boolean; items: TcpTunnel[] }> {
    const res = await this.fetch<{
      data: {
        supported: boolean;
        items: Array<{
          id: string;
          name?: string | null;
          enabled: boolean;
          local_addr: string;
          local_port: number;
          remote_bind_addr: string;
          remote_port: number;
          ssh_host: string;
          ssh_port: number;
          username: string;
          auth:
            | { type: "password"; password?: string }
            | { type: "private_key_path"; path: string };
          strict_host_key_checking: boolean;
          host_key_fingerprint: string;
          allow_public_bind: boolean;
          connect_timeout_ms: number;
          keepalive_interval_ms: number;
          reconnect_backoff_ms: { base_ms: number; max_ms: number };
          status: TcpTunnel["status"];
        }>;
      };
    }>("/api/tcp-tunnels");

    const items = res.data.items.map((item) => {
      const authType = item.auth?.type ?? "password";
      const password = item.auth?.type === "password" ? item.auth.password : undefined;
      const privateKeyPath = item.auth?.type === "private_key_path" ? item.auth.path : undefined;

      return {
        id: item.id,
        name: item.name ?? undefined,
        mode: "single" as const,
        enabled: item.enabled,
        local_addr: item.local_addr,
        local_port: item.local_port,
        remote_bind_addr: item.remote_bind_addr,
        remote_port: item.remote_port,
        ssh_host: item.ssh_host,
        ssh_port: item.ssh_port,
        username: item.username,
        auth_type: authType,
        password,
        private_key_path: privateKeyPath,
        strict_host_key_checking: item.strict_host_key_checking,
        host_key_fingerprint: item.host_key_fingerprint,
        allow_public_bind: item.allow_public_bind,
        connect_timeout_ms: item.connect_timeout_ms,
        keepalive_interval_ms: item.keepalive_interval_ms,
        backoff_base_ms: item.reconnect_backoff_ms?.base_ms,
        backoff_max_ms: item.reconnect_backoff_ms?.max_ms,
        status: item.status,
      };
    });

    const supported = res.data.supported ?? true;
    return { supported, items };
  }

  async createTcpTunnel(config: Record<string, unknown>): Promise<TcpTunnel> {
    const res = await this.fetch<{ data: TcpTunnel }>("/api/tcp-tunnels", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateTcpTunnel(id: string, config: Record<string, unknown>): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async deleteTcpTunnel(id: string): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}`, {
      method: "DELETE",
    });
  }

  async restartTcpTunnel(id: string): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}/restart`, {
      method: "POST",
    });
  }

  async startTcpTunnel(id: string): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}/start`, {
      method: "POST",
    });
  }

  async stopTcpTunnel(id: string): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}/stop`, {
      method: "POST",
    });
  }

  async testTcpTunnel(id: string): Promise<{ ok: boolean; latency_ms?: number }> {
    return this.fetch(`/api/tcp-tunnels/${id}/test`, {
      method: "POST",
    });
  }

  async copyTcpTunnel(id: string): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}/copy`, {
      method: "POST",
    });
  }

  // Terminals
  async getTerminals(): Promise<Terminal[]> {
    const res = await this.fetch<{ data: { items: Terminal[] } }>("/api/terminals");
    return res.data.items;
  }

  async createTerminal(config: Omit<Terminal, "id" | "status">): Promise<Terminal> {
    const res = await this.fetch<{ data: Terminal }>("/api/terminals", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateTerminal(id: string, config: Partial<Terminal>): Promise<void> {
    await this.fetch(`/api/terminals/${id}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async deleteTerminal(id: string): Promise<void> {
    await this.fetch(`/api/terminals/${id}`, {
      method: "DELETE",
    });
  }

  async startTerminal(id: string): Promise<void> {
    await this.fetch(`/api/terminals/${id}/start`, {
      method: "POST",
    });
  }

  async stopTerminal(id: string): Promise<void> {
    await this.fetch(`/api/terminals/${id}/stop`, {
      method: "POST",
    });
  }

  async restartTerminal(id: string): Promise<void> {
    await this.fetch(`/api/terminals/${id}/restart`, {
      method: "POST",
    });
  }

  async upgradeGotty(): Promise<void> {
    await this.fetch("/api/gotty/upgrade", {
      method: "POST",
    });
  }

  // VNC
  async getVncSessions(): Promise<VncSession[]> {
    const res = await this.fetch<{ data: { items: VncSession[] } }>("/api/vnc-sessions");
    return res.data.items;
  }

  async createVncSession(config: Record<string, unknown>): Promise<VncSession> {
    const res = await this.fetch<{ data: VncSession }>("/api/vnc-sessions", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateVncSession(id: string, config: Partial<VncSession>): Promise<void> {
    await this.fetch(`/api/vnc-sessions/${id}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async deleteVncSession(id: string): Promise<void> {
    await this.fetch(`/api/vnc-sessions/${id}`, {
      method: "DELETE",
    });
  }

  async startVncSession(id: string): Promise<void> {
    await this.fetch(`/api/vnc-sessions/${id}/start`, {
      method: "POST",
    });
  }

  async stopVncSession(id: string): Promise<void> {
    await this.fetch(`/api/vnc-sessions/${id}/stop`, {
      method: "POST",
    });
  }

  async restartVncSession(id: string): Promise<void> {
    await this.fetch(`/api/vnc-sessions/${id}/restart`, {
      method: "POST",
    });
  }

  // Apps
  async getApps(): Promise<App[]> {
    const res = await this.fetch<{ data: { items: App[] } }>("/api/apps");
    return res.data.items;
  }

  async getAppTemplates(): Promise<{ templates: AppTemplate[] }> {
    const res = await this.fetch<{ data: { templates: AppTemplate[] } }>("/api/apps/templates");
    return res.data;
  }

  async createApp(config: Record<string, unknown>): Promise<App> {
    const res = await this.fetch<{ data: App }>("/api/apps", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateApp(id: string, config: Partial<App>): Promise<void> {
    await this.fetch(`/api/apps/${id}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async startApp(id: string): Promise<void> {
    await this.fetch(`/api/apps/${id}/start`, {
      method: "POST",
    });
  }

  async stopApp(id: string): Promise<void> {
    await this.fetch(`/api/apps/${id}/stop`, {
      method: "POST",
    });
  }

  async restartApp(id: string): Promise<void> {
    await this.fetch(`/api/apps/${id}/restart`, {
      method: "POST",
    });
  }

  async deleteApp(id: string): Promise<void> {
    await this.fetch(`/api/apps/${id}`, {
      method: "DELETE",
    });
  }

  // Update
  async getVersion(): Promise<VersionInfo> {
    const res = await this.fetch<{ data: VersionInfo }>("/api/version");
    return res.data;
  }

  async upgrade(): Promise<string> {
    const res = await this.fetch<{ data: string }>("/api/upgrade", {
      method: "POST",
    });
    return res.data;
  }

  async updatePassword(password: string): Promise<void> {
    await this.fetch("/api/password", {
      method: "POST",
      body: JSON.stringify({ password }),
    });
  }

  // Hosts
  async getHosts(): Promise<Host[]> {
    const res = await this.fetch<{ data: { items: Host[] } }>("/api/hosts");
    return res.data.items;
  }

  async getHostDefaultKeyPath(): Promise<string | null> {
    const res = await this.fetch<{ data: { path: string | null } }>("/api/hosts/default-key-path");
    return res.data.path;
  }

  async testHostConfig(config: Partial<Host> & { auth_type: string }): Promise<void> {
    await this.fetch("/api/hosts/test", {
      method: "POST",
      body: JSON.stringify(config),
    });
  }

  async getHost(id: string): Promise<Host> {
    const res = await this.fetch<{ data: Host }>(`/api/hosts/${id}`);
    return res.data;
  }

  async createHost(config: Omit<Host, "id">): Promise<Host> {
    const res = await this.fetch<{ data: Host }>("/api/hosts", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateHost(id: string, config: Partial<Host>): Promise<void> {
    await this.fetch(`/api/hosts/${id}`, {
      method: "PUT",
      body: JSON.stringify(config),
    });
  }

  async deleteHost(id: string): Promise<void> {
    await this.fetch(`/api/hosts/${id}`, {
      method: "DELETE",
    });
  }

  async testHost(id: string): Promise<HostTestResult> {
    const res = await this.fetch<{ data: HostTestResult }>(`/api/hosts/${id}/test`, {
      method: "POST",
    });
    return res.data;
  }
}

export const api = new ApiClient();

// WebSocket helper functions
function getWsBase(): string {
  if (API_BASE) {
    return API_BASE.replace(/^http/, "ws");
  }
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  return `${protocol}://${window.location.host}`;
}

export function getTrafficWsUrl(): string {
  const token = localStorage.getItem("miao_token");
  if (!token) {
    throw new Error("No authentication token found. Please login first.");
  }
  const wsBase = getWsBase();
  return `${wsBase}/api/clash/ws/traffic?token=${token}`;
}

export function getLogsWsUrl(): string {
  const token = localStorage.getItem("miao_token");
  if (!token) {
    throw new Error("No authentication token found. Please login first.");
  }
  const wsBase = getWsBase();
  return `${wsBase}/api/clash/ws/logs?token=${token}`;
}
