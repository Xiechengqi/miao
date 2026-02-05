"use client";

import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import { Card, Button, Badge, ConfirmModal } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { cn } from "@/lib/utils";
import { Trash2, Download, Search, Filter, ArrowDown } from "lucide-react";

type LogLevel = "all" | "debug" | "info" | "warning" | "error";

export default function LogsPage() {
  const { logs, setLogs, logWsConnected: wsConnected } = useStore();
  const parentRef = useRef<HTMLDivElement>(null);
  const scrollTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // 过滤状态
  const [searchTerm, setSearchTerm] = useState("");
  const [levelFilter, setLevelFilter] = useState<LogLevel>("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const [showClearConfirm, setShowClearConfirm] = useState(false);

  // 过滤后的日志
  const filteredLogs = useMemo(() => {
    let result = logs;

    // 级别过滤
    if (levelFilter !== "all") {
      result = result.filter((log) => log.level === levelFilter);
    }

    // 搜索过滤
    if (searchTerm.trim()) {
      const term = searchTerm.toLowerCase();
      result = result.filter((log) => log.message.toLowerCase().includes(term));
    }

    return result;
  }, [logs, levelFilter, searchTerm]);

  // 滚动到底部函数
  const scrollToBottom = useCallback(() => {
    if (parentRef.current) {
      parentRef.current.scrollTop = parentRef.current.scrollHeight;
    }
    setAutoScroll(true);
  }, []);

  const getLevelVariant = (level: string): "default" | "success" | "warning" | "error" => {
    switch (level) {
      case "debug":
        return "default";
      case "info":
        return "success";
      case "warning":
        return "warning";
      case "error":
        return "error";
      default:
        return "default";
    }
  };

  const handleClear = () => {
    setShowClearConfirm(true);
  };

  const handleExport = () => {
    const content = logs
      .map((log) => `${log.time} [${log.level.toUpperCase()}] ${log.message}`)
      .join("\n");
    const blob = new Blob([content], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `miao-logs-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // 监听滚动，禁用自动滚动（当用户手动滚动时），带防抖
  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    if (scrollTimeoutRef.current) {
      clearTimeout(scrollTimeoutRef.current);
    }
    scrollTimeoutRef.current = setTimeout(() => {
      const element = e.currentTarget;
      const isAtBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 50;
      if (!isAtBottom && autoScroll) {
        setAutoScroll(false);
      } else if (isAtBottom && !autoScroll) {
        setAutoScroll(true);
      }
    }, 100);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-3xl font-black">日志</h1>
          <p className="text-slate-500 mt-1">
            查看实时日志
            <span
              className={cn(
                "ml-2 px-2 py-0.5 rounded-full text-xs",
                wsConnected ? "bg-emerald-100 text-emerald-600" : "bg-red-100 text-red-600"
              )}
            >
              {wsConnected ? "已连接" : "未连接"}
            </span>
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" onClick={handleExport}>
            <Download className="w-4 h-4" />
            导出
          </Button>
          <Button variant="secondary" onClick={handleClear}>
            <Trash2 className="w-4 h-4" />
            清空
          </Button>
        </div>
      </div>

      {/* 搜索和过滤工具栏 */}
      <Card className="p-3" hoverEffect={false}>
        <div className="flex flex-wrap items-center gap-3">
          {/* 搜索框 */}
          <div className="relative flex-1 min-w-[200px] max-w-sm">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
            <input
              type="text"
              placeholder="搜索日志内容..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full h-9 pl-10 pr-4 rounded-lg border border-slate-200 bg-white text-sm outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 transition-all"
            />
          </div>

          {/* 级别过滤 */}
          <div className="flex items-center gap-1">
            <Filter className="w-4 h-4 text-slate-500 mr-1" />
            {(["all", "debug", "info", "warning", "error"] as LogLevel[]).map((level) => (
              <button
                key={level}
                onClick={() => setLevelFilter(level)}
                className={cn(
                  "px-2 py-1 rounded text-xs font-medium transition-colors",
                  levelFilter === level
                    ? level === "all"
                      ? "bg-indigo-100 text-indigo-600"
                      : level === "debug"
                      ? "bg-gray-200 text-gray-700"
                      : level === "info"
                      ? "bg-emerald-100 text-emerald-600"
                      : level === "warning"
                      ? "bg-amber-100 text-amber-600"
                      : "bg-red-100 text-red-600"
                    : "bg-slate-100 text-slate-600 hover:bg-slate-200"
                )}
              >
                {level === "all" ? "全部" : level.toUpperCase()}
              </button>
            ))}
          </div>

          {/* 自动滚动开关 */}
          <button
            onClick={() => setAutoScroll(!autoScroll)}
            className={cn(
              "flex items-center gap-1.5 px-2 py-1 rounded text-xs font-medium transition-colors",
              autoScroll
                ? "bg-indigo-100 text-indigo-600"
                : "bg-slate-100 text-slate-600"
            )}
          >
            <ArrowDown className={cn("w-3.5 h-3.5", autoScroll && "animate-bounce")} />
            {autoScroll ? "自动滚动" : "已暂停"}
          </button>

          {/* 统计信息 */}
          <span className="ml-auto text-sm text-slate-500">
            {filteredLogs.length} / {logs.length} 条
            {searchTerm && levelFilter !== "all" && (
              <span className="ml-1 text-indigo-600">(已过滤)</span>
            )}
          </span>
        </div>
      </Card>

      {/* Log Container */}
      <Card className="p-0 flex flex-col max-h-[70vh]" hoverEffect={false}>
        <div
          className="flex-1 overflow-y-auto min-h-0"
          ref={parentRef}
          onScroll={handleScroll}
        >
          {filteredLogs.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-slate-500 py-8">
              {logs.length === 0 ? (
                <>
                  <p>暂无日志</p>
                  <p className="text-xs mt-1">等待日志数据...</p>
                </>
              ) : (
                <p>没有匹配的日志</p>
              )}
            </div>
          ) : (
            <div className="flex flex-col">
              {filteredLogs.map((log, index) => (
                <div
                  key={`${log.time}-${index}`}
                  className="flex items-start gap-2 px-2 py-1 rounded hover:bg-slate-500/5"
                >
                  <span className="text-slate-500 whitespace-nowrap text-xs font-mono shrink-0">
                    {log.time.slice(11, 19)}
                  </span>
                  <Badge variant={getLevelVariant(log.level)} className="text-[10px] px-1.5 py-0 shrink-0">
                    {log.level.toUpperCase()}
                  </Badge>
                  <span className="text-slate-900 whitespace-pre-wrap break-all font-mono text-sm leading-normal min-w-0">
                    {log.message}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 底部工具栏 */}
        <div className="bg-slate-50 px-4 py-2 border-t border-slate-200 flex items-center justify-between">
          <span className="text-sm text-slate-500">
            共 {logs.length} 条日志
          </span>
          {filteredLogs.length < logs.length && (
            <span className="text-sm text-amber-600">
              已隐藏 {logs.length - filteredLogs.length} 条
            </span>
          )}
          <Button variant="ghost" size="sm" onClick={scrollToBottom}>
            <ArrowDown className="w-4 h-4 mr-1" />
            滚动到底部
          </Button>
        </div>
      </Card>

      <ConfirmModal
        isOpen={showClearConfirm}
        onClose={() => setShowClearConfirm(false)}
        onConfirm={() => {
          setLogs([]);
          setShowClearConfirm(false);
        }}
        title="确认清空日志"
        message="确定要清空所有日志吗？"
        variant="danger"
      />
    </div>
  );

  // 清理防抖定时器
  useEffect(() => {
    return () => {
      if (scrollTimeoutRef.current) {
        clearTimeout(scrollTimeoutRef.current);
      }
    };
  }, []);
}
