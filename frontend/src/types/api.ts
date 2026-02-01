// API Response Types
export interface ApiResponse<T> {
  data: T;
  success: boolean;
  error?: string;
}

// Status Types
export interface Status {
  running: boolean;
  pid?: number;
  uptime_secs?: number;
}

export interface SystemProcessor {
  frequency: number;
  vendor: string;
  brand: string;
}

export interface SystemGraphicCard {
  id: string;
  name: string;
  brand: string;
  memory: number;
  temperature: number;
}

export interface SystemDisk {
  name: string;
  fs: string;
  storageType: string;
  mountPoint: string;
  available: number;
  size: number;
}

export interface SystemInfo {
  osName: string;
  kernelVersion: string;
  osVersion: string;
  distribution: string;
  hostname: string;
  memory: number;
  processor: SystemProcessor;
  totalProcessors: number;
  graphics: SystemGraphicCard[];
  disks: SystemDisk[];
  cameras: { name: string; path: string }[];
  nvidia?: {
    driverVersion: string;
    nvmlVersion: string;
    cudaVersion: number;
  };
  vaapi: boolean;
  model?: string | null;
}

export interface SystemDiskUsage {
  name: string;
  used: number;
  total: number;
}

export interface GraphicsUsage {
  id: string;
  memoryUsage: number;
  memoryUsed: number;
  encoder: number;
  decoder: number;
  gpu: number;
  temperature: number;
}

export interface SystemStatus {
  timestamp: number;
  samplePeriodSecs: number;
  cpuPercent: number;
  memoryUsedKb: number;
  uptimeSecs?: number | null;
  graphics: GraphicsUsage[];
  disks: SystemDiskUsage[];
  nvidiaAvailable: boolean;
}

export interface SystemMetricsPoint {
  timestamp: number;
  cpuPercent: number;
  memoryUsedKb: number;
  gpuPercent?: number;
  diskUsedBytes: number;
  diskTotalBytes: number;
}

export interface SystemMetricsResponse {
  range: string;
  step: string;
  series: SystemMetricsPoint[];
}

export interface VersionInfo {
  current: string;
  latest?: string | null;
  has_update: boolean;
  download_url?: string | null;
  commit?: string | null;
  commit_date?: string | null;
}

export interface DnsCandidate {
  name: string;
  health: "ok" | "bad" | "cooldown";
}

export interface DnsStatus {
  active?: string;
  candidates?: Array<string | DnsCandidate>;
  health?: Record<string, "ok" | "bad" | "cooldown">;
  last_check_secs_ago?: number;
}

// Node Types
export type NodeType = "ssh" | "ss" | "anytls" | "hysteria2" | "tuic" | "vmess";

export interface Node {
  id: string;
  name: string;
  type: NodeType;
  address: string;
  port: number;
  delay?: number;
}

export interface ProxyGroup {
  name: string;
  type: string;
  now?: string;
  all: string[];
}

// Manual Node Types
export type ManualNodeType = "hysteria2" | "anytls" | "shadowsocks" | "ssh";

export interface ManualNode {
  tag: string;
  node_type: ManualNodeType;
  server: string;
  server_port: number;
  user?: string;
  password?: string;
  sni?: string;
  cipher?: string;
}

// Traffic Types
export interface TrafficData {
  up: number;
  down: number;
}

// Sync Types
export interface SyncConfig {
  id: string;
  name?: string | null;
  enabled: boolean;
  local_paths: string[];
  remote_path?: string | null;
  ssh: {
    host: string;
    port: number;
    username: string;
  };
  auth?: {
    type: "password";
    password?: string | null;
  };
  options?: {
    delete?: boolean;
    exclude?: string[];
    include?: string[];
    compression_level?: number;
    compression_threads?: number;
    incremental?: boolean;
    preserve_permissions?: boolean;
    follow_symlinks?: boolean;
  };
  schedule?: {
    enabled: boolean;
    cron?: string | null;
    timezone?: string | null;
  };
  status?: {
    state: "running" | "stopped" | "idle" | "error";
    last_error?: {
      message: string;
    };
  };
}

// TCP Tunnel Types
export interface TcpTunnel {
  id: string;
  name?: string;
  mode: "single" | "full";
  enabled?: boolean;
  remote_bind_addr: string;
  remote_port?: number;
  local_addr: string;
  local_port: number;
  username: string;
  auth_type?: "password" | "private_key_path";
  password?: string;
  private_key_path?: string;
  private_key_passphrase?: string;
  clear_private_key_passphrase?: boolean;
  allow_public_bind?: boolean;
  strict_host_key_checking?: boolean;
  host_key_fingerprint?: string;
  connect_timeout_ms?: number;
  keepalive_interval_ms?: number;
  backoff_base_ms?: number;
  backoff_max_ms?: number;
  scan_interval_ms?: number;
  debounce_ms?: number;
  exclude_ports?: number[];
  ssh_host: string;
  ssh_port: number;
  status: {
    state: "stopped" | "connecting" | "forwarding" | "error";
    active_conns: number;
    last_error?: {
      code: string;
      message: string;
    };
  };
}

// Terminal Types
export interface Terminal {
  id: string;
  name?: string;
  enabled?: boolean;
  addr: string;
  port: number;
  command?: string;
  command_args?: string[];
  auth_username?: string;
  auth_password?: string;
  extra_args?: string[];
  status: {
    running: boolean;
    pid?: number;
    uptime_secs?: number;
  };
}

// VNC Types
export interface VncSession {
  id: string;
  name?: string;
  enabled?: boolean;
  addr: string;
  port: number;
  display: string;
  resolution?: string;
  depth?: number;
  frame_rate?: number;
  password?: string;
  view_only?: boolean;
  status: {
    running: boolean;
    pid?: number;
    uptime_secs?: number;
  };
}

// App Types
export interface App {
  id: string;
  name?: string;
  type: "chromium" | "firefox" | "appimage";
  enabled?: boolean;
  vnc_session_id?: string;
  display?: string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  status: {
    running: boolean;
    pid?: number;
    uptime_secs?: number;
  };
  port?: number;
}

export interface AppTemplate {
  id: string;
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

// Log Types
export interface LogEntry {
  time: string;
  level: "debug" | "info" | "warning" | "error";
  message: string;
}

// Connectivity Types
export interface ConnectivitySite {
  name: string;
  url: string;
}

export interface ConnectivityResult {
  delay?: number;
  status: "pending" | "testing" | "success" | "error";
}

// Host Types
export type HostAuthType = "password" | "private_key_path";

export interface Host {
  id: string;
  name?: string | null;
  host: string;
  port: number;
  username: string;
  auth_type: HostAuthType;
  password?: string;
  private_key_path?: string;
  private_key_passphrase?: string;
  created_at?: number;
  updated_at?: number;
}

// Toast Types
export interface ToastMessage {
  id: string;
  type: "success" | "error" | "info" | "warning";
  message: string;
}
