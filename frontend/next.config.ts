import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: false,
  skipTrailingSlashRedirect: true,
};

export default nextConfig;
