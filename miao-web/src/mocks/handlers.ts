import { NextRequest, NextResponse } from "next/server";
import {
  mockAuth,
  mockSetupStatus,
  mockStatus,
  mockDnsStatus,
  mockSystemInfo,
  buildMockSystemStatus,
  buildMockMetrics,
  mockProxies,
  mockManualNodes,
  mockSyncs,
  mockSubFiles,
  mockTcpTunnels,
  mockTerminals,
  mockVncSessions,
  mockApps,
  mockAppTemplates,
  mockLogs,
} from "./data";

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";

interface MockResponse {
  data?: unknown;
  status?: number;
  success?: boolean;
  error?: string;
}

// In-memory state for mock data (allows mutations during dev)
let mockState = {
  status: { ...mockStatus },
  syncs: [...mockSyncs],
  tunnels: [...mockTcpTunnels],
  terminals: [...mockTerminals],
  vncSessions: [...mockVncSessions],
  apps: [...mockApps],
  manualNodes: [...mockManualNodes],
  subFiles: { ...mockSubFiles },
};

function jsonResponse(data: MockResponse, status: number = 200): NextResponse {
  return NextResponse.json(data, { status });
}

function successResponse(data: unknown): NextResponse {
  return jsonResponse({ data, success: true });
}

function errorResponse(error: string, status: number = 400): NextResponse {
  return jsonResponse({ success: false, error }, status);
}

// Route handlers
const handlers: Record<string, Record<HttpMethod, (req: NextRequest, pathParts: string[]) => Promise<NextResponse> | NextResponse>> = {
  // Auth
  "login": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse(mockAuth),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "setup/status": {
    GET: () => successResponse(mockSetupStatus),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "setup/init": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Status
  "status": {
    GET: () => successResponse(mockState.status),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "status/start": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => {
      mockState.status = { ...mockState.status, running: true, pid: 12345, uptime_secs: 0 };
      return successResponse(mockState.status);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "status/stop": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => {
      mockState.status = { ...mockState.status, running: false, pid: undefined, uptime_secs: undefined };
      return successResponse(mockState.status);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // System
  "system/info": {
    GET: () => successResponse(mockSystemInfo),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "system/status": {
    GET: () => successResponse(buildMockSystemStatus()),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "system/metrics": {
    GET: (req) => {
      const range = req.nextUrl.searchParams.get("range") ?? "1h";
      const step = req.nextUrl.searchParams.get("step") ?? undefined;
      return successResponse(buildMockMetrics(range, step));
    },
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // DNS
  "dns": {
    GET: () => successResponse(mockDnsStatus),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "dns/check": {
    GET: () => successResponse(mockDnsStatus),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "dns/switch": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Nodes
  "nodes": {
    GET: () => successResponse(mockState.manualNodes),
    POST: async (req) => {
      const body = await req.json();
      mockState.manualNodes.push(body);
      return successResponse(body);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: async (req) => {
      const body = await req.json().catch(() => ({}));
      const tag = body?.tag;
      if (!tag) return errorResponse("Tag required", 400);
      mockState.manualNodes = mockState.manualNodes.filter((node) => node.tag !== tag);
      return successResponse({});
    },
  },

  // Subscription files
  "sub-files": {
    GET: () => successResponse(mockState.subFiles),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "sub-files/reload": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Node test
  "node-test": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({ latency_ms: Math.floor(Math.random() * 120) + 20 }),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Connectivity
  "connectivity": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => {
      const latency = Math.floor(Math.random() * 900) + 50;
      return successResponse({ success: true, latency_ms: latency });
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Proxies (Clash)
  "clash/proxies": {
    GET: () => successResponse(mockProxies),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "clash/delays": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse(Math.floor(Math.random() * 150) + 20),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "clash/switch": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "clash/reload": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Syncs
  "syncs": {
    GET: () => successResponse(mockState.syncs),
    POST: async (req) => {
      const body = await req.json();
      const newSync = {
        ...body,
        id: String(Date.now()),
        status: { state: "stopped" as const },
      };
      mockState.syncs.push(newSync);
      return successResponse(newSync);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // TCP Tunnels
  "tcp-tunnels": {
    GET: () => successResponse(mockState.tunnels),
    POST: async (req) => {
      const body = await req.json();
      const newTunnel = {
        ...body,
        id: String(Date.now()),
        status: { state: "stopped" as const, active_conns: 0 },
      };
      mockState.tunnels.push(newTunnel);
      return successResponse(newTunnel);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Terminals
  "terminals": {
    GET: () => successResponse(mockState.terminals),
    POST: async (req) => {
      const body = await req.json();
      const newTerminal = {
        ...body,
        id: String(Date.now()),
        status: { running: false as const },
      };
      mockState.terminals.push(newTerminal);
      return successResponse(newTerminal);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "terminals/upgrade": {
    GET: () => errorResponse("Method not allowed", 405),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // VNC Sessions
  "vnc-sessions": {
    GET: () => successResponse(mockState.vncSessions),
    POST: async (req) => {
      const body = await req.json();
      const newSession = {
        ...body,
        id: String(Date.now()),
        status: { running: true as const, pid: 1234, uptime_secs: 0 },
      };
      mockState.vncSessions.push(newSession);
      return successResponse(newSession);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Apps
  "apps": {
    GET: () => successResponse(mockState.apps),
    POST: async (req) => {
      const body = await req.json();
      const newApp = {
        ...body,
        id: String(Date.now()),
        status: { running: false as const },
      };
      mockState.apps.push(newApp);
      return successResponse(newApp);
    },
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
  "apps/templates": {
    GET: () => successResponse({ templates: mockAppTemplates }),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Logs
  "logs": {
    GET: () => successResponse(mockLogs),
    POST: () => errorResponse("Method not allowed", 405),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },

  // Update
  "update": {
    GET: () => successResponse({ version: "1.2.0", url: "https://example.com/update" }),
    POST: () => successResponse({}),
    PUT: () => errorResponse("Method not allowed", 405),
    DELETE: () => errorResponse("Method not allowed", 405),
  },
};

// Dynamic route handlers (with path parameters)
function handleDynamicRoute(
  method: HttpMethod,
  pathParts: string[],
  req: NextRequest
): NextResponse | Promise<NextResponse> | null {
  // Handle syncs/:id routes
  if (pathParts[0] === "syncs" && pathParts.length >= 2) {
    const id = pathParts[1];
    const action = pathParts[2];

    if (method === "PUT" && !action) {
      // Update sync
      const syncIndex = mockState.syncs.findIndex((s) => s.id === id);
      if (syncIndex === -1) return errorResponse("Sync not found", 404);
      return req.json().then((body) => {
        mockState.syncs[syncIndex] = { ...mockState.syncs[syncIndex], ...body };
        return successResponse(mockState.syncs[syncIndex]);
      });
    }
    if (method === "DELETE" && !action) {
      // Delete sync
      const syncIndex = mockState.syncs.findIndex((s) => s.id === id);
      if (syncIndex === -1) return errorResponse("Sync not found", 404);
      mockState.syncs.splice(syncIndex, 1);
      return successResponse({});
    }
    if (method === "POST" && action === "start") {
      const syncIndex = mockState.syncs.findIndex((s) => s.id === id);
      if (syncIndex === -1) return errorResponse("Sync not found", 404);
      mockState.syncs[syncIndex].enabled = true;
      mockState.syncs[syncIndex].status = { state: "running" };
      return successResponse({});
    }
    if (method === "POST" && action === "stop") {
      const syncIndex = mockState.syncs.findIndex((s) => s.id === id);
      if (syncIndex === -1) return errorResponse("Sync not found", 404);
      mockState.syncs[syncIndex].enabled = false;
      mockState.syncs[syncIndex].status = { state: "stopped" };
      return successResponse({});
    }
    if (method === "POST" && action === "test") {
      const syncIndex = mockState.syncs.findIndex((s) => s.id === id);
      if (syncIndex === -1) return errorResponse("Sync not found", 404);
      mockState.syncs[syncIndex].status = { state: "running" };
      setTimeout(() => {
        mockState.syncs[syncIndex].status = { state: "stopped" };
      }, 800);
      return successResponse({});
    }
  }

  // Handle tcp-tunnels/:id routes
  if (pathParts[0] === "tcp-tunnels" && pathParts.length >= 2) {
    const id = pathParts[1];
    const action = pathParts[2];

    if (method === "PUT" && !action) {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      return req.json().then((body) => {
        mockState.tunnels[tunnelIndex] = { ...mockState.tunnels[tunnelIndex], ...body };
        return successResponse(mockState.tunnels[tunnelIndex]);
      });
    }
    if (method === "DELETE" && !action) {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      mockState.tunnels.splice(tunnelIndex, 1);
      return successResponse({});
    }
    if (method === "POST" && action === "start") {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      mockState.tunnels[tunnelIndex].status = { state: "forwarding", active_conns: 0 };
      return successResponse({});
    }
    if (method === "POST" && action === "stop") {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      mockState.tunnels[tunnelIndex].status = { state: "stopped", active_conns: 0 };
      return successResponse({});
    }
    if (method === "POST" && action === "restart") {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      mockState.tunnels[tunnelIndex].status = { state: "connecting", active_conns: 0 };
      setTimeout(() => {
        mockState.tunnels[tunnelIndex].status = { state: "forwarding", active_conns: 0 };
      }, 500);
      return successResponse({});
    }
    if (method === "POST" && action === "test") {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      return successResponse({});
    }
    if (method === "POST" && action === "copy") {
      const tunnelIndex = mockState.tunnels.findIndex((t) => t.id === id);
      if (tunnelIndex === -1) return errorResponse("Tunnel not found", 404);
      const copy = { ...mockState.tunnels[tunnelIndex], id: String(Date.now()), name: `${mockState.tunnels[tunnelIndex].name || id}-copy` };
      mockState.tunnels.push(copy);
      return successResponse(copy);
    }
  }

  // Handle terminals/:id routes
  if (pathParts[0] === "terminals" && pathParts.length >= 2) {
    const id = pathParts[1];
    const action = pathParts[2];

    if (method === "PUT" && !action) {
      const terminalIndex = mockState.terminals.findIndex((t) => t.id === id);
      if (terminalIndex === -1) return errorResponse("Terminal not found", 404);
      return req.json().then((body) => {
        mockState.terminals[terminalIndex] = { ...mockState.terminals[terminalIndex], ...body };
        return successResponse({});
      });
    }
    if (method === "DELETE" && !action) {
      const terminalIndex = mockState.terminals.findIndex((t) => t.id === id);
      if (terminalIndex === -1) return errorResponse("Terminal not found", 404);
      mockState.terminals.splice(terminalIndex, 1);
      return successResponse({});
    }
    if (method === "POST" && action === "start") {
      const terminalIndex = mockState.terminals.findIndex((t) => t.id === id);
      if (terminalIndex === -1) return errorResponse("Terminal not found", 404);
      mockState.terminals[terminalIndex].status = { running: true, pid: 1111, uptime_secs: 0 };
      return successResponse({});
    }
    if (method === "POST" && action === "stop") {
      const terminalIndex = mockState.terminals.findIndex((t) => t.id === id);
      if (terminalIndex === -1) return errorResponse("Terminal not found", 404);
      mockState.terminals[terminalIndex].status = { running: false };
      return successResponse({});
    }
    if (method === "POST" && action === "restart") {
      const terminalIndex = mockState.terminals.findIndex((t) => t.id === id);
      if (terminalIndex === -1) return errorResponse("Terminal not found", 404);
      mockState.terminals[terminalIndex].status = { running: true, pid: 2222, uptime_secs: 0 };
      return successResponse({});
    }
  }

  // Handle vnc-sessions/:id routes
  if (pathParts[0] === "vnc-sessions" && pathParts.length >= 2) {
    const id = pathParts[1];
    const action = pathParts[2];

    if (method === "PUT" && !action) {
      const sessionIndex = mockState.vncSessions.findIndex((s) => s.id === id);
      if (sessionIndex === -1) return errorResponse("VNC session not found", 404);
      return req.json().then((body) => {
        mockState.vncSessions[sessionIndex] = { ...mockState.vncSessions[sessionIndex], ...body };
        return successResponse({});
      });
    }
    if (method === "DELETE" && !action) {
      const sessionIndex = mockState.vncSessions.findIndex((s) => s.id === id);
      if (sessionIndex === -1) return errorResponse("VNC session not found", 404);
      mockState.vncSessions.splice(sessionIndex, 1);
      return successResponse({});
    }
    if (method === "POST" && action === "start") {
      const sessionIndex = mockState.vncSessions.findIndex((s) => s.id === id);
      if (sessionIndex === -1) return errorResponse("VNC session not found", 404);
      mockState.vncSessions[sessionIndex].status = { running: true, pid: 3333, uptime_secs: 0 };
      return successResponse({});
    }
    if (method === "POST" && action === "stop") {
      const sessionIndex = mockState.vncSessions.findIndex((s) => s.id === id);
      if (sessionIndex === -1) return errorResponse("VNC session not found", 404);
      mockState.vncSessions[sessionIndex].status = { running: false };
      return successResponse({});
    }
    if (method === "POST" && action === "restart") {
      const sessionIndex = mockState.vncSessions.findIndex((s) => s.id === id);
      if (sessionIndex === -1) return errorResponse("VNC session not found", 404);
      mockState.vncSessions[sessionIndex].status = { running: true, pid: 4444, uptime_secs: 0 };
      return successResponse({});
    }
  }

  // Handle apps/:id routes
  if (pathParts[0] === "apps" && pathParts.length >= 2) {
    const id = pathParts[1];
    const action = pathParts[2];

    if (method === "PUT" && !action) {
      const appIndex = mockState.apps.findIndex((a) => a.id === id);
      if (appIndex === -1) return errorResponse("App not found", 404);
      return req.json().then((body) => {
        mockState.apps[appIndex] = { ...mockState.apps[appIndex], ...body };
        return successResponse({});
      });
    }
    if (method === "DELETE" && !action) {
      const appIndex = mockState.apps.findIndex((a) => a.id === id);
      if (appIndex === -1) return errorResponse("App not found", 404);
      mockState.apps.splice(appIndex, 1);
      return successResponse({});
    }
    if (method === "POST" && action === "start") {
      const appIndex = mockState.apps.findIndex((a) => a.id === id);
      if (appIndex === -1) return errorResponse("App not found", 404);
      mockState.apps[appIndex].status = { running: true, pid: 7777, uptime_secs: 0 };
      return successResponse({});
    }
    if (method === "POST" && action === "stop") {
      const appIndex = mockState.apps.findIndex((a) => a.id === id);
      if (appIndex === -1) return errorResponse("App not found", 404);
      mockState.apps[appIndex].status = { running: false };
      return successResponse({});
    }
    if (method === "POST" && action === "restart") {
      const appIndex = mockState.apps.findIndex((a) => a.id === id);
      if (appIndex === -1) return errorResponse("App not found", 404);
      mockState.apps[appIndex].status = { running: true, pid: 8888, uptime_secs: 0 };
      return successResponse({});
    }
  }

  // Handle nodes/:id routes
  if (pathParts[0] === "nodes" && pathParts.length >= 2) {
    const tag = pathParts[1];
    if (method === "GET") {
      const node = mockState.manualNodes.find((item) => item.tag === tag);
      if (!node) return errorResponse("Node not found", 404);
      return successResponse(node);
    }
    if (method === "PUT") {
      return req.json().then((body) => {
        const index = mockState.manualNodes.findIndex((item) => item.tag === tag);
        if (index === -1) return errorResponse("Node not found", 404);
        mockState.manualNodes[index] = { ...mockState.manualNodes[index], ...body, tag };
        return successResponse(mockState.manualNodes[index]);
      });
    }
  }

  return null;
}

export async function mockHandler(
  method: HttpMethod,
  path: string,
  request: NextRequest
): Promise<NextResponse> {
  // Remove leading slash if present
  const cleanPath = path.startsWith("/") ? path.slice(1) : path;
  const pathParts = cleanPath.split("/");

  // Try exact match first
  const handler = handlers[cleanPath];
  if (handler && handler[method]) {
    const result = handler[method](request, pathParts);
    return result instanceof Promise ? await result : result;
  }

  // Try dynamic route
  const dynamicResult = handleDynamicRoute(method, pathParts, request);
  if (dynamicResult) {
    return dynamicResult instanceof Promise ? await dynamicResult : dynamicResult;
  }

  // Not found
  return errorResponse(`Mock not implemented: ${method} /${cleanPath}`, 404);
}
