import type { Metadata } from "next";
import { Plus_Jakarta_Sans } from "next/font/google";
import "./globals.css";
import { MockProvider } from "@/components/MockProvider";

const plusJakartaSans = Plus_Jakarta_Sans({
  subsets: ["latin"],
  weight: ["400", "500", "600", "700", "800"],
  variable: "--font-plus-jakarta",
  display: "swap",
});

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
        className={`${plusJakartaSans.variable} antialiased`}
        style={{ fontFamily: "var(--font-plus-jakarta), -apple-system, BlinkMacSystemFont, sans-serif" }}
      >
        <MockProvider>{children}</MockProvider>
      </body>
    </html>
  );
}
