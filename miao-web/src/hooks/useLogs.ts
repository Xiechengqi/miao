"use client";

import { useEffect, useRef, useCallback } from "react";
import { useStore } from "@/stores/useStore";
import { getLogsWsUrl } from "@/lib/api";
import { LogEntry } from "@/types/api";

export function useLogs() {
  const { logs, logWsConnected, setLogs, addLog, setLogWsConnected } = useStore();
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const reconnectAttemptsRef = useRef(0);

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    const wsUrl = getLogsWsUrl();
    const ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      reconnectAttemptsRef.current = 0;
      setLogWsConnected(true);
    };

    ws.onmessage = (event) => {
      try {
        const data: LogEntry = JSON.parse(event.data);
        addLog(data);
      } catch (error) {
        console.error("Failed to parse log entry:", error);
      }
    };

    ws.onclose = () => {
      setLogWsConnected(false);
      // Attempt to reconnect after delay
      const delay = Math.min(1000 * Math.pow(2, reconnectAttemptsRef.current), 30000);
      reconnectTimeoutRef.current = setTimeout(() => {
        reconnectAttemptsRef.current++;
        connect();
      }, delay);
    };

    ws.onerror = (error) => {
      console.error("Logs WebSocket error:", error);
    };

    wsRef.current = ws;
  }, [addLog, setLogWsConnected]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
    }
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    setLogWsConnected(false);
  }, [setLogWsConnected]);

  const clearLogs = useCallback(() => {
    setLogs([]);
  }, [setLogs]);

  useEffect(() => {
    connect();
    return () => disconnect();
  }, [connect, disconnect]);

  return {
    logs,
    logWsConnected,
    connect,
    disconnect,
    clearLogs,
  };
}
