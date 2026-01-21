"use client";

import { useEffect } from "react";
import { installMockFetch } from "@/lib/mockFetch";

export function MockProvider({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    // Install mock fetch on client side
    installMockFetch();
  }, []);

  return <>{children}</>;
}
