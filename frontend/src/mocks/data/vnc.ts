import type { VncSession } from "@/types/api";

export const mockVncSessions: VncSession[] = [
  {
    id: "1",
    name: "Desktop Session 1",
    enabled: true,
    addr: "0.0.0.0",
    port: 5901,
    display: ":10",
    resolution: "1920x1080",
    depth: 24,
    frame_rate: 30,
    password: "secret",
    view_only: false,
    status: { running: true, pid: 9012, uptime_secs: 5400 },
  },
  {
    id: "2",
    name: "Remote Workstation",
    enabled: false,
    addr: "127.0.0.1",
    port: 5902,
    display: ":12",
    resolution: "1280x720",
    depth: 24,
    frame_rate: 24,
    password: "",
    view_only: true,
    status: { running: false },
  },
];
