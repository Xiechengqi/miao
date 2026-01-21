import type {
  SystemInfo,
  SystemMetricsResponse,
  SystemMetricsPoint,
  SystemStatus,
} from "@/types/api";

const GIB = 1024 * 1024 * 1024;

export const mockSystemInfo: SystemInfo = {
  osName: "Ubuntu",
  kernelVersion: "6.5.0-26-generic",
  osVersion: "22.04.4 LTS",
  distribution: "Ubuntu",
  hostname: "miao-dev",
  memory: 16 * GIB,
  processor: {
    frequency: 3200,
    vendor: "GenuineIntel",
    brand: "Intel(R) Core(TM) i7-10700K",
  },
  totalProcessors: 16,
  graphics: [
    {
      id: "gpu-0",
      name: "NVIDIA GeForce RTX 3060",
      brand: "NVIDIA",
      memory: 12 * GIB,
      temperature: 58,
    },
  ],
  disks: [
    {
      name: "/dev/nvme0n1p1",
      fs: "ext4",
      storageType: "ssd",
      mountPoint: "/",
      available: 180 * GIB,
      size: 512 * GIB,
    },
  ],
  cameras: [],
  nvidia: {
    driverVersion: "535.154",
    nvmlVersion: "12.535.154",
    cudaVersion: 12,
  },
  vaapi: true,
  model: "x86_64",
};

const baseStatus: Omit<SystemStatus, "timestamp"> = {
  samplePeriodSecs: 5,
  cpuPercent: 28,
  memoryUsedKb: Math.round((6 * GIB) / 1024),
  graphics: [
    {
      id: "gpu-0",
      memoryUsage: 0.42,
      memoryUsed: Math.round(4.5 * GIB),
      encoder: 12,
      decoder: 8,
      gpu: 35,
      temperature: 60,
    },
  ],
  disks: [
    {
      name: "/dev/nvme0n1p1",
      used: 280 * GIB,
      total: 512 * GIB,
    },
  ],
  nvidiaAvailable: true,
};

export function buildMockSystemStatus(nowSeconds: number = Math.floor(Date.now() / 1000)): SystemStatus {
  const phase = nowSeconds % 300;
  const cpuPercent = 25 + Math.round(12 * Math.sin((phase / 300) * Math.PI * 2));
  const gpuPercent = 30 + Math.round(10 * Math.cos((phase / 300) * Math.PI * 2));
  const memoryUsedKb = baseStatus.memoryUsedKb + Math.round(256 * 1024 * Math.sin((phase / 300) * Math.PI * 2));

  return {
    ...baseStatus,
    timestamp: nowSeconds,
    cpuPercent,
    memoryUsedKb,
    graphics: [
      {
        ...baseStatus.graphics[0],
        gpu: gpuPercent,
        temperature: 55 + Math.round(6 * Math.sin((phase / 300) * Math.PI * 2)),
      },
    ],
  };
}

function parseDurationSeconds(value: string | null, fallbackSeconds: number): number {
  if (!value) return fallbackSeconds;
  const match = value.trim().match(/^(\d+)([smhd])$/);
  if (!match) return fallbackSeconds;
  const amount = Number(match[1]);
  const unit = match[2];
  switch (unit) {
    case "s":
      return amount;
    case "m":
      return amount * 60;
    case "h":
      return amount * 3600;
    case "d":
      return amount * 86400;
    default:
      return fallbackSeconds;
  }
}

function formatDurationSeconds(seconds: number): string {
  if (seconds % 86400 === 0) return `${seconds / 86400}d`;
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}

function defaultStepSeconds(rangeSeconds: number): number {
  if (rangeSeconds <= 15 * 60) return 60;
  if (rangeSeconds <= 60 * 60) return 5 * 60;
  if (rangeSeconds <= 6 * 60 * 60) return 30 * 60;
  if (rangeSeconds <= 24 * 60 * 60) return 60 * 60;
  return 6 * 60 * 60;
}

export function buildMockMetrics(range: string, step?: string): SystemMetricsResponse {
  const rangeSeconds = parseDurationSeconds(range, 3600);
  const stepSeconds = step
    ? parseDurationSeconds(step, defaultStepSeconds(rangeSeconds))
    : defaultStepSeconds(rangeSeconds);
  const now = Math.floor(Date.now() / 1000);
  const count = Math.max(8, Math.min(60, Math.floor(rangeSeconds / stepSeconds) + 1));
  const start = now - rangeSeconds;
  const diskTotalBytes = 512 * GIB;
  const diskBaseUsed = 280 * GIB;
  const memoryBaseKb = Math.round((6 * GIB) / 1024);

  const series: SystemMetricsPoint[] = Array.from({ length: count }, (_, index) => {
    const ratio = count === 1 ? 0 : index / (count - 1);
    const phase = ratio * Math.PI * 2;
    const cpuPercent = 22 + Math.round(14 * Math.sin(phase));
    const gpuPercent = 25 + Math.round(18 * Math.cos(phase));
    const memoryUsedKb = memoryBaseKb + Math.round(512 * 1024 * (0.5 + 0.5 * Math.sin(phase)));
    const diskUsedBytes = diskBaseUsed + Math.round(6 * GIB * (0.5 + 0.5 * Math.cos(phase)));

    return {
      timestamp: start + index * stepSeconds,
      cpuPercent,
      memoryUsedKb,
      gpuPercent,
      diskUsedBytes,
      diskTotalBytes,
    };
  });

  return {
    range,
    step: step ? step : formatDurationSeconds(stepSeconds),
    series,
  };
}
