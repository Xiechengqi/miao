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
  SubFilesResponse,
  SyncConfig,
  TcpTunnel,
  Terminal,
  VncSession,
  App,
  AppTemplate,
  TrafficData,
  LogEntry,
} from "@/types/api";

// API 配置
// 前端和后端在同一个域名端口下，使用空字符串表示相对路径
const API_BASE = "";

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
      throw new Error(error?.error || `API Error: ${response.statusText}`);
    }

    return response.json();
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
    return this.fetch("/api/setup/status");
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
    const res = await this.fetch<{ data: SystemMetricsResponse }>(
      `/api/system/metrics?${params.toString()}`
    );
    return res.data;
  }

  async startService(): Promise<Status> {
    const res = await this.fetch<{ data: Status }>("/api/status/start", {
      method: "POST",
    });
    return res.data;
  }

  async stopService(): Promise<Status> {
    const res = await this.fetch<{ data: Status }>("/api/status/stop", {
      method: "POST",
    });
    return res.data;
  }

  // DNS
  async getDnsStatus(): Promise<DnsStatus> {
    const res = await this.fetch<{ data: DnsStatus }>("/api/dns");
    return res.data;
  }

  async checkDns(): Promise<DnsStatus> {
    const res = await this.fetch<{ data: DnsStatus }>("/api/dns/check");
    return res.data;
  }

  async switchDns(name: string): Promise<void> {
    await this.fetch("/api/dns/switch", {
      method: "POST",
      body: JSON.stringify({ name }),
    });
  }

  // Subscription Files
  async getSubFiles(): Promise<SubFilesResponse> {
    const res = await this.fetch<{ data: SubFilesResponse }>("/api/sub-files");
    return res.data;
  }

  async reloadSubFiles(): Promise<void> {
    await this.fetch("/api/sub-files/reload", {
      method: "POST",
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

  async testDelay(nodeName: string): Promise<number> {
    const res = await this.fetch<{ data: number }>("/api/clash/delays", {
      method: "POST",
      body: JSON.stringify({ name: nodeName }),
    });
    return res.data;
  }

  async switchProxy(group: string, name: string): Promise<void> {
    await this.fetch("/api/clash/switch", {
      method: "POST",
      body: JSON.stringify({ group, name }),
    });
  }

  // Sync (Backup)
  async getSyncs(): Promise<SyncConfig[]> {
    const res = await this.fetch<{ data: SyncConfig[] }>("/api/syncs");
    return res.data;
  }

  async createSync(config: Omit<SyncConfig, "id">): Promise<SyncConfig> {
    const res = await this.fetch<{ data: SyncConfig }>("/api/syncs", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateSync(id: string, config: Partial<SyncConfig>): Promise<void> {
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

  // TCP Tunnels
  async getTcpTunnels(): Promise<TcpTunnel[]> {
    const res = await this.fetch<{ data: TcpTunnel[] }>("/api/tcp-tunnels");
    return res.data;
  }

  async createTcpTunnel(config: Omit<TcpTunnel, "id" | "status">): Promise<TcpTunnel> {
    const res = await this.fetch<{ data: TcpTunnel }>("/api/tcp-tunnels", {
      method: "POST",
      body: JSON.stringify(config),
    });
    return res.data;
  }

  async updateTcpTunnel(id: string, config: Partial<TcpTunnel>): Promise<void> {
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

  async testTcpTunnel(id: string): Promise<void> {
    await this.fetch(`/api/tcp-tunnels/${id}/test`, {
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
    const res = await this.fetch<{ data: Terminal[] }>("/api/terminals");
    return res.data;
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
    await this.fetch("/api/terminals/upgrade", {
      method: "POST",
    });
  }

  // VNC
  async getVncSessions(): Promise<VncSession[]> {
    const res = await this.fetch<{ data: VncSession[] }>("/api/vnc-sessions");
    return res.data;
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
    const res = await this.fetch<{ data: App[] }>("/api/apps");
    return res.data;
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
  async checkUpdate(): Promise<{ version: string; url: string }> {
    const res = await this.fetch<{ data: { version: string; url: string } }>("/api/update");
    return res.data;
  }

  async performUpdate(): Promise<void> {
    await this.fetch("/api/update", {
      method: "POST",
    });
  }

  // Reload
  async reloadSubscription(): Promise<void> {
    await this.fetch("/api/clash/reload", {
      method: "POST",
    });
  }

  // Logs
  async getLogs(): Promise<LogEntry[]> {
    const res = await this.fetch<{ data: LogEntry[] }>("/api/logs");
    return res.data;
  }
}

export const api = new ApiClient();

// WebSocket helper functions
export function getTrafficWsUrl(): string {
  const token = localStorage.getItem("miao_token");
  if (!token) {
    throw new Error("No authentication token found. Please login first.");
  }
  const wsBase = API_BASE.replace("http", "ws");
  return `${wsBase}/api/clash/ws/traffic?token=${token}`;
}

export function getLogsWsUrl(): string {
  const token = localStorage.getItem("miao_token");
  if (!token) {
    throw new Error("No authentication token found. Please login first.");
  }
  const wsBase = API_BASE.replace("http", "ws");
  return `${wsBase}/api/clash/ws/logs?token=${token}`;
}
