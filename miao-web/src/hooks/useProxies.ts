"use client";

import { useCallback } from "react";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { Node, ProxyGroup } from "@/types/api";

export function useProxies() {
  const {
    proxyGroups,
    nodes,
    delays,
    setProxyGroups,
    setNodes,
    setDelays,
    addToast,
  } = useStore();

  const fetchProxies = useCallback(async () => {
    try {
      const { proxies, nodes: nodeList } = await api.getProxies();
      setProxyGroups(proxies);
      setNodes(nodeList);
    } catch (error) {
      console.error("Failed to fetch proxies:", error);
      addToast({
        type: "error",
        message: "获取节点列表失败",
      });
    }
  }, [setProxyGroups, setNodes, addToast]);

  const testDelay = useCallback(async (nodeName: string) => {
    try {
      const delay = await api.testDelay(nodeName);
      setDelays({ ...delays, [nodeName]: delay });
      return delay;
    } catch (error) {
      console.error("Failed to test delay:", error);
      return undefined;
    }
  }, [delays, setDelays]);

  const testAllDelays = useCallback(async () => {
    const allNodes: string[] = [];

    // Collect all nodes from proxy groups
    Object.values(proxyGroups).forEach((group) => {
      group.all.forEach((node) => {
        if (!allNodes.includes(node)) {
          allNodes.push(node);
        }
      });
    });

    // Also test standalone nodes
    nodes.forEach((node) => {
      if (!allNodes.includes(node.name)) {
        allNodes.push(node.name);
      }
    });

    const newDelays: Record<string, number> = { ...delays };

    for (const nodeName of allNodes) {
      try {
        const delay = await api.testDelay(nodeName);
        newDelays[nodeName] = delay;
      } catch {
        newDelays[nodeName] = 0;
      }
    }

    setDelays(newDelays);
    addToast({
      type: "success",
      message: "延迟测试完成",
    });
  }, [proxyGroups, nodes, delays, setDelays, addToast]);

  const switchProxy = useCallback(async (group: string, name: string) => {
    try {
      await api.switchProxy(group, name);
      await fetchProxies();
      addToast({
        type: "success",
        message: `已切换到 ${name}`,
      });
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "切换失败",
      });
    }
  }, [fetchProxies, addToast]);

  const selectFastest = useCallback(async () => {
    const allNodes: Array<{ name: string; delay: number }> = [];

    Object.entries(proxyGroups).forEach(([, group]) => {
      if (group.now) {
        group.all.forEach((nodeName) => {
          const delay = delays[nodeName];
          if (delay !== undefined && delay > 0) {
            allNodes.push({ name: nodeName, delay });
          }
        });
      }
    });

    if (allNodes.length === 0) return;

    // Find the fastest node
    const fastest = allNodes.reduce((prev, curr) =>
      curr.delay < prev.delay ? curr : prev
    );

    // Find which group this node belongs to
    for (const [groupName, group] of Object.entries(proxyGroups)) {
      if (group.all.includes(fastest.name)) {
        await switchProxy(groupName, fastest.name);
        break;
      }
    }
  }, [proxyGroups, delays, switchProxy]);

  return {
    proxyGroups,
    nodes,
    delays,
    fetchProxies,
    testDelay,
    testAllDelays,
    switchProxy,
    selectFastest,
  };
}
