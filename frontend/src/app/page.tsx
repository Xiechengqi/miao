"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { ClayBlobs } from "@/components/ui";

export default function HomePage() {
  const router = useRouter();
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);

    const checkAuth = async () => {
      const token = localStorage.getItem("miao_token");

      if (!token) {
        router.push("/login");
        return;
      }

      router.push("/dashboard");
    };

    checkAuth();
  }, [router]);

  if (!mounted) {
    return (
      <div className="min-h-screen bg-slate-50 flex items-center justify-center">
        <ClayBlobs />
        <div className="text-center">
          <div className="w-12 h-12 border-4 border-indigo-200 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      </div>
    );
  }

  return null;
}
