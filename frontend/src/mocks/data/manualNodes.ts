import type { ManualNode } from "@/types/api";

export const mockManualNodes: ManualNode[] = [
  {
    tag: "hk-ss-01",
    node_type: "shadowsocks",
    server: "hk1.example.com",
    server_port: 443,
    cipher: "2022-blake3-aes-128-gcm",
  },
  {
    tag: "jp-ssh",
    node_type: "ssh",
    server: "jp.example.com",
    server_port: 22,
    user: "root",
  },
  {
    tag: "anytls-demo",
    node_type: "anytls",
    server: "edge.example.com",
    server_port: 443,
    sni: "edge.example.com",
  },
];
