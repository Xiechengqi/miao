"use client";

import { memo, useCallback } from "react";
import { Pin } from "lucide-react";
import { getDelayClass, getDelayText } from "@/lib/utils";

interface NodeCardProps {
  nodeName: string;
  delay?: number;
  isFavorite: boolean;
  isCached: boolean;
  onTestDelay: (nodeName: string) => void;
  onToggleFavorite: (nodeName: string) => void;
}

export const NodeCard = memo(function NodeCard({
  nodeName,
  delay,
  isFavorite,
  isCached,
  onTestDelay,
  onToggleFavorite,
}: NodeCardProps) {
  const handleClick = useCallback(() => {
    onTestDelay(nodeName);
  }, [nodeName, onTestDelay]);

  const handleFavoriteClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onToggleFavorite(nodeName);
  }, [nodeName, onToggleFavorite]);

  return (
    <div
      onClick={handleClick}
      className={`group relative flex items-center gap-2 rounded-lg border bg-white px-3 py-2 text-left cursor-pointer transition-all hover-select ${
        isFavorite
          ? "border-amber-200 hover:border-amber-300"
          : "border-slate-200 hover:border-slate-300"
      }`}
    >
      {isFavorite && <Pin className="w-3.5 h-3.5 text-amber-500" />}
      <span className="max-w-[120px] truncate text-sm font-medium" title={nodeName}>
        {nodeName}
      </span>
      {delay !== undefined && (
        <span className={`text-xs font-mono ${getDelayClass(delay)}`}>
          {delay === 0 ? "超时" : getDelayText(delay)}
        </span>
      )}
      {isCached && delay !== undefined && (
        <span className="text-[10px] text-slate-400" title="缓存中">缓存</span>
      )}
      <button
        onClick={handleFavoriteClick}
        className={`opacity-0 group-hover:opacity-100 transition-opacity ${
          isFavorite ? "text-amber-500" : "text-slate-400 hover:text-amber-500"
        }`}
        title={isFavorite ? "取消收藏" : "添加收藏"}
      >
        <Pin className={`w-3.5 h-3.5 ${isFavorite ? "fill-current" : ""}`} />
      </button>
    </div>
  );
});

// 收藏节点卡片组件
export const FavoriteNodeCard = memo(function FavoriteNodeCard({
  nodeName,
  delay,
  onTestDelay,
}: {
  nodeName: string;
  delay?: number;
  onTestDelay: (nodeName: string) => void;
}) {
  const handleClick = useCallback(() => {
    onTestDelay(nodeName);
  }, [nodeName, onTestDelay]);

  return (
    <button
      onClick={handleClick}
      className="group flex items-center gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-left hover:border-amber-300 hover:bg-amber-100 transition-all"
    >
      <Pin className="w-3.5 h-3.5 text-amber-500" />
      <span className="max-w-[120px] truncate text-sm font-medium" title={nodeName}>
        {nodeName}
      </span>
      {delay !== undefined && (
        <span className={`text-xs font-mono ${getDelayClass(delay)}`}>
          {delay === 0 ? "超时" : getDelayText(delay)}
        </span>
      )}
    </button>
  );
});
