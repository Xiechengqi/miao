import type { Status, DnsStatus } from "@/types/api";

export const mockStatus: Status = {
  running: true,
  pid: 12345,
  uptime_secs: 3600,
};

export const mockDnsStatus: DnsStatus = {
  active: "cloudflare",
  candidates: ["cloudflare", "google", "quad9", "alidns"],
  health: {
    cloudflare: "ok",
    google: "ok",
    quad9: "ok",
    alidns: "bad",
  },
  last_check_secs_ago: 60,
};
