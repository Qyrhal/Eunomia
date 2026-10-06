import type { NextConfig } from "next";

const backend = process.env.BACKEND_INTERNAL_URL || "http://localhost:8001";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      { source: "/api/:path*", destination: `${backend}/api/:path*` },
      { source: "/mcp", destination: `${backend}/mcp` },
    ];
  },
};

export default nextConfig;
