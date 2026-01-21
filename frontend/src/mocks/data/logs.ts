import type { LogEntry } from "@/types/api";

export const mockLogs: LogEntry[] = [
  {
    time: new Date(Date.now() - 60000).toISOString(),
    level: "info",
    message: "Service started successfully",
  },
  {
    time: new Date(Date.now() - 55000).toISOString(),
    level: "info",
    message: "Connected to proxy server HK-01",
  },
  {
    time: new Date(Date.now() - 50000).toISOString(),
    level: "debug",
    message: "DNS query: google.com -> 142.250.185.206",
  },
  {
    time: new Date(Date.now() - 45000).toISOString(),
    level: "info",
    message: "Traffic routing rule matched: DIRECT",
  },
  {
    time: new Date(Date.now() - 40000).toISOString(),
    level: "warning",
    message: "High latency detected on node JP-02: 250ms",
  },
  {
    time: new Date(Date.now() - 35000).toISOString(),
    level: "debug",
    message: "Health check passed for node SG-01",
  },
  {
    time: new Date(Date.now() - 30000).toISOString(),
    level: "info",
    message: "Subscription sync completed: 6 nodes updated",
  },
  {
    time: new Date(Date.now() - 25000).toISOString(),
    level: "error",
    message: "Connection timeout to node US-02",
  },
  {
    time: new Date(Date.now() - 20000).toISOString(),
    level: "info",
    message: "Auto-switched to backup node HK-02",
  },
  {
    time: new Date(Date.now() - 15000).toISOString(),
    level: "debug",
    message: "TCP tunnel established: port 2222 -> 22",
  },
];
