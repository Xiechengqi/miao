import type { Metadata } from "next";
import "./globals.css";
import "@/components/onboarding/driver-custom.css";
import { MockProvider } from "@/components/MockProvider";
import { OnboardingProvider, OnboardingTour } from "@/components/onboarding";

export const metadata: Metadata = {
  title: "Miao 控制面板",
  description: "Miao - 代理服务管理面板",
  icons: {
    icon: "/icon.svg",
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="zh-CN">
      <body
        className="antialiased"
        style={{ fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif" }}
      >
        <MockProvider>
          <OnboardingProvider>
            {children}
            <OnboardingTour />
          </OnboardingProvider>
        </MockProvider>
      </body>
    </html>
  );
}
