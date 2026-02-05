import type { Status, DnsStatus } from "@/types/api";

export const mockStatus: Status = {
  running: true,
  pid: 12345,
  uptime_secs: 3600,
  pending_restart: false,
};

export const mockDnsStatus: DnsStatus = {
  active: "cloudflare",
  candidates: ["cloudflare", "google", "quad9", "alidns"],
};
