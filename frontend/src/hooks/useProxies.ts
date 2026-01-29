"use client";

import { useCallback } from "react";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";

export function useProxies() {
  const {
    delays,
    setProxyGroups,
    setNodes,
    setDelays,
    addToast,
  } = useStore();

  const fetchProxies = useCallback(async (silent = false) => {
    try {
      const { proxies, nodes: nodeList } = await api.getProxies();
      setProxyGroups(proxies);
      setNodes(nodeList);
    } catch (error) {
      if (!silent) {
        console.error("Failed to fetch proxies:", error);
        addToast({
          type: "error",
          message: "获取节点列表失败",
        });
      }
    }
  }, [setProxyGroups, setNodes, addToast]);

  const testDelay = useCallback(async (nodeName: string, url?: string, signal?: AbortSignal) => {
    try {
      const delay = await api.testDelay(nodeName, url, signal);
      setDelays({ [nodeName]: delay } as Record<string, number>);
      return delay;
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        return undefined;
      }
      console.error("Failed to test delay:", error);
      return undefined;
    }
  }, [setDelays]);

  return {
    fetchProxies,
    testDelay,
    setDelays,
  };
}
