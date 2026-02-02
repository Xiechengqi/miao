"use client";

import { useEffect, useState } from "react";
import { ClayBlobs } from "@/components/ui";
import { Box, GitCommit, Clock, ExternalLink } from "lucide-react";
import { cn } from "@/lib/utils";

interface BuildInfo {
  version: string;
  commit: string;
  commitDate: string;
  commitMessage: string;
  buildTime: string;
}

async function getBuildInfo(): Promise<BuildInfo> {
  try {
    const res = await fetch("/build-info.json");
    if (res.ok) {
      return await res.json();
    }
  } catch {
    // ignore
  }
  return {
    version: "",
    commit: "",
    commitDate: "",
    commitMessage: "",
    buildTime: "",
  };
}

export default function AboutPage() {
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchData = async () => {
      try {
        const build = await getBuildInfo();
        setBuildInfo(build);
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, []);

  if (loading) {
    return (
      <div className="min-h-[60vh] flex items-center justify-center">
        <ClayBlobs />
        <div className="text-center">
          <div className="w-10 h-10 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      </div>
    );
  }

  const displayCommit = buildInfo?.commit || "Unknown";

  return (
    <div className="max-w-2xl mx-auto">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-slate-800">版本信息</h1>
        <p className="text-slate-500 mt-1">查看当前运行的版本和构建信息</p>
      </div>

      <div className="bg-white/70 backdrop-blur-xl rounded-2xl shadow-[0_4px_20px_-2px_rgba(79,70,229,0.1)] overflow-hidden">
        {/* Header */}
        <div className="bg-gradient-to-br from-[#A78BFA]/20 to-[#7C3AED]/10 px-8 py-10 text-center">
          <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-[#A78BFA] to-[#7C3AED] flex items-center justify-center mx-auto shadow-[0_4px_14px_0_rgba(79,70,229,0.3)]">
            <Box className="w-8 h-8 text-white" />
          </div>
          <h2 className="text-3xl font-black text-slate-800 mt-4">Miao</h2>
          <p className="text-slate-500 mt-1">代理服务管理面板</p>
        </div>

        {/* Info List */}
        <div className="divide-y divide-slate-100">
          {/* Commit */}
          <div className="px-8 py-5 flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-emerald-50 flex items-center justify-center">
                <GitCommit className="w-5 h-5 text-emerald-600" />
              </div>
              <div>
                <p className="text-sm text-slate-500">Commit</p>
                <p className="font-mono font-semibold text-slate-800">
                  {displayCommit}
                </p>
                {buildInfo?.commitMessage && (
                  <p className="text-sm text-slate-500 mt-1">{buildInfo.commitMessage}</p>
                )}
              </div>
            </div>
            {displayCommit && displayCommit !== "Unknown" && (
              <a
                href={`https://github.com/Xiechengqi/miao/commit/${displayCommit}`}
                target="_blank"
                rel="noopener noreferrer"
                className={cn(
                  "px-4 py-2 rounded-lg text-sm font-semibold text-emerald-600",
                  "bg-emerald-50 hover:bg-emerald-100 transition-colors cursor-pointer",
                  "flex items-center gap-2"
                )}
              >
                查看详情
                <ExternalLink className="w-4 h-4" />
              </a>
            )}
          </div>

          {/* Build Time */}
          {buildInfo?.buildTime && (
            <div className="px-8 py-5 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-amber-50 flex items-center justify-center">
                <Clock className="w-5 h-5 text-amber-600" />
              </div>
              <div>
                <p className="text-sm text-slate-500">构建时间 (UTC+8)</p>
                <p className="font-semibold text-slate-800">{buildInfo.buildTime}</p>
              </div>
            </div>
          )}

          {/* Commit Date */}
          {buildInfo?.commitDate && (
            <div className="px-8 py-5 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-rose-50 flex items-center justify-center">
                <GitCommit className="w-5 h-5 text-rose-600" />
              </div>
              <div>
                <p className="text-sm text-slate-500">提交时间 (UTC+8)</p>
                <p className="font-semibold text-slate-800">{buildInfo.commitDate}</p>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Footer */}
      <div className="mt-8 text-center text-slate-400 text-sm">
        <p>Miao 控制面板 - 代理服务管理面板</p>
        <p className="mt-1">
          <a
            href="https://github.com/Xiechengqi/miao"
            target="_blank"
            rel="noopener noreferrer"
            className="text-indigo-600 hover:text-indigo-700"
          >
            GitHub
          </a>
        </p>
      </div>
    </div>
  );
}
