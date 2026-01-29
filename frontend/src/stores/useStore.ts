import { create } from "zustand";
import { Status, DnsStatus, Node, ProxyGroup, SyncConfig, TcpTunnel, TrafficData, LogEntry, ToastMessage, Terminal } from "@/types/api";

interface AppState {
  // Auth State
  authenticated: boolean;
  setupRequired: boolean;

  // Service State
  status: Status;
  dnsStatus: DnsStatus | null;
  loading: boolean;
  loadingAction: string | null;

  // Proxy State
  proxyGroups: Record<string, ProxyGroup>;
  nodes: Node[];
  delays: Record<string, number>;

  // Traffic State
  traffic: TrafficData;

  // Sync State
  syncs: SyncConfig[];
  syncsLoaded: boolean;

  // TCP Tunnel State
  tcpTunnels: TcpTunnel[];
  tcpTunnelsLoaded: boolean;
  tcpTunnelsSupported: boolean;

  // Terminal State
  terminals: Terminal[];
  terminalsLoaded: boolean;

  // Logs State
  logs: LogEntry[];
  logWsConnected: boolean;
  logKeys: Set<string>;

  // Toast State
  toasts: ToastMessage[];

  // Actions
  setAuthenticated: (value: boolean) => void;
  setSetupRequired: (value: boolean) => void;
  setStatus: (status: Status) => void;
  setDnsStatus: (status: DnsStatus | null) => void;
  setLoading: (loading: boolean, action?: string) => void;
  setProxyGroups: (groups: Record<string, ProxyGroup>) => void;
  setNodes: (nodes: Node[]) => void;
  setDelays: (delays: Record<string, number>) => void;
  setTraffic: (traffic: TrafficData) => void;
  setSyncs: (syncs: SyncConfig[]) => void;
  setSyncsLoaded: (loaded: boolean) => void;
  setTcpTunnels: (tunnels: TcpTunnel[]) => void;
  setTcpTunnelsLoaded: (loaded: boolean) => void;
  setTcpTunnelsSupported: (supported: boolean) => void;
  setTerminals: (terminals: Terminal[]) => void;
  setTerminalsLoaded: (loaded: boolean) => void;
  setLogs: (logs: LogEntry[]) => void;
  addLog: (log: LogEntry) => void;
  setLogWsConnected: (connected: boolean) => void;
  addToast: (toast: Omit<ToastMessage, "id">) => void;
  removeToast: (id: string) => void;
  clearToasts: () => void;

  // Computed
  getTcpDisplayItems: () => TcpTunnel[];
}

export const useStore = create<AppState>((set, get) => ({
  // Initial State
  authenticated: false,
  setupRequired: false,
  status: { running: false },
  dnsStatus: null,
  loading: false,
  loadingAction: null,
  proxyGroups: {},
  nodes: [],
  delays: {},
  traffic: { up: 0, down: 0 },
  syncs: [],
  syncsLoaded: false,
  tcpTunnels: [],
  tcpTunnelsLoaded: false,
  tcpTunnelsSupported: true,
  terminals: [],
  terminalsLoaded: false,
  logs: [],
  logWsConnected: false,
  logKeys: new Set(),
  toasts: [],

  // Actions
  setAuthenticated: (value) => set({ authenticated: value }),
  setSetupRequired: (value) => set({ setupRequired: value }),
  setStatus: (status) => set({ status }),
  setDnsStatus: (status) => set({ dnsStatus: status }),
  setLoading: (loading, action) => set({ loading, loadingAction: action || null }),
  setProxyGroups: (groups) => set({ proxyGroups: groups }),
  setNodes: (nodes) => set({ nodes }),
  setDelays: (delays) => set({ delays }),
  setTraffic: (traffic) => set({ traffic }),
  setSyncs: (syncs) => set({ syncs }),
  setSyncsLoaded: (loaded) => set({ syncsLoaded: loaded }),
  setTcpTunnels: (tunnels) => set({ tcpTunnels: tunnels }),
  setTcpTunnelsLoaded: (loaded) => set({ tcpTunnelsLoaded: loaded }),
  setTcpTunnelsSupported: (supported) => set({ tcpTunnelsSupported: supported }),
  setTerminals: (terminals) => set({ terminals }),
  setTerminalsLoaded: (loaded) => set({ terminalsLoaded: loaded }),
  setLogs: (logs) => {
    const keys = new Set(logs.map((entry) => `${entry.time}|${entry.level}|${entry.message}`));
    set({ logs, logKeys: keys });
  },
  addLog: (log) =>
    set((state) => {
      const key = `${log.time}|${log.level}|${log.message}`;
      if (state.logKeys.has(key)) {
        return state;
      }
      const newLogs = [log, ...state.logs];
      const newKeys = new Set(state.logKeys);
      newKeys.add(key);
      if (newLogs.length > 500) {
        const removed = newLogs.pop();
        if (removed) {
          const removedKey = `${removed.time}|${removed.level}|${removed.message}`;
          newKeys.delete(removedKey);
        }
      }
      return { logs: newLogs, logKeys: newKeys };
    }),
  setLogWsConnected: (connected) => set({ logWsConnected: connected }),
  addToast: (toast) =>
    set((state) => ({
      toasts: [...state.toasts, { ...toast, id: Date.now().toString() }],
    })),
  removeToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),
  clearToasts: () => set({ toasts: [] }),

  // Computed
  getTcpDisplayItems: () => {
    const { tcpTunnels } = get();
    return tcpTunnels;
  },
}));
