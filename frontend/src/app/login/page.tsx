"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Input, ClayBlobs } from "@/components/ui";
import { api } from "@/lib/api";
import { Lock, AlertCircle } from "lucide-react";

export default function LoginPage() {
  const router = useRouter();
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      await api.login(password);
      router.push("/dashboard");
    } catch (err) {
      setError(err instanceof Error ? err.message : "登录失败");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-slate-50 flex items-center justify-center p-4">
      <ClayBlobs />

      <div className="relative w-full max-w-md">
        <div className="bg-white rounded-xl shadow-[0_20px_50px_-12px_rgba(79,70,229,0.25)] border border-slate-100 p-8">
          {/* Header */}
          <div className="text-center mb-8">
            <div className="inline-flex items-center justify-center w-16 h-16 rounded-xl bg-gradient-to-r from-indigo-600 to-violet-600 shadow-[0_4px_14px_0_rgba(79,70,229,0.3)] mb-4">
              <Lock className="w-8 h-8 text-white" />
            </div>
            <h1 className="text-3xl font-extrabold text-slate-900">
              Miao 登录
            </h1>
            <p className="mt-2 text-slate-500">请输入密码访问控制面板</p>
          </div>

          {/* Form */}
          <form onSubmit={handleSubmit} className="space-y-6">
            <div>
              <Input
                type="password"
                placeholder="请输入密码"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={loading}
                autoFocus
              />
            </div>

            {error && (
              <div className="flex items-center gap-2 p-3 rounded-lg bg-red-50 border border-red-100 text-red-700 text-sm">
                <AlertCircle className="w-4 h-4 flex-shrink-0" />
                <span>{error}</span>
              </div>
            )}

            <Button
              type="submit"
              className="w-full"
              loading={loading}
              disabled={!password || loading}
            >
              登录
            </Button>
          </form>
        </div>
      </div>
    </div>
  );
}
