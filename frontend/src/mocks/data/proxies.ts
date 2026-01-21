import type { Node, ProxyGroup } from "@/types/api";

export const mockNodes: Node[] = [
  {
    id: "1",
    name: "HK-01",
    type: "ss",
    address: "hk1.example.com",
    port: 443,
    delay: 45,
  },
  {
    id: "2",
    name: "HK-02",
    type: "vmess",
    address: "hk2.example.com",
    port: 443,
    delay: 68,
  },
  {
    id: "3",
    name: "JP-01",
    type: "ss",
    address: "jp1.example.com",
    port: 443,
    delay: 85,
  },
  {
    id: "4",
    name: "JP-02",
    type: "hysteria2",
    address: "jp2.example.com",
    port: 443,
    delay: 92,
  },
  {
    id: "5",
    name: "US-01",
    type: "vmess",
    address: "us1.example.com",
    port: 443,
    delay: 180,
  },
  {
    id: "6",
    name: "SG-01",
    type: "tuic",
    address: "sg1.example.com",
    port: 443,
    delay: 55,
  },
];

export const mockProxyGroups: Record<string, ProxyGroup> = {
  Proxy: {
    name: "Proxy",
    type: "Selector",
    now: "HK-01",
    all: ["HK-01", "HK-02", "JP-01", "JP-02", "US-01", "SG-01"],
  },
  Auto: {
    name: "Auto",
    type: "URLTest",
    now: "HK-01",
    all: ["HK-01", "HK-02", "JP-01", "SG-01"],
  },
  Streaming: {
    name: "Streaming",
    type: "Selector",
    now: "JP-01",
    all: ["HK-01", "HK-02", "JP-01", "JP-02", "US-01"],
  },
};

export const mockProxies = {
  proxies: mockProxyGroups,
  nodes: mockNodes,
};
