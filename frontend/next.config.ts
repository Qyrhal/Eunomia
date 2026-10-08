import type { NextConfig } from "next";

const backend = process.env.BACKEND_INTERNAL_URL || "http://localhost:8001";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      { source: "/api/:path*", destination: `${backend}/api/:path*` },
      { source: "/mcp", destination: `${backend}/mcp` },
      // OAuth for MCP clients: discovery documents and the protocol endpoints (the consent page itself is /consent)
      { source: "/.well-known/:path*", destination: `${backend}/.well-known/:path*` },
      { source: "/oauth/:path*", destination: `${backend}/oauth/:path*` },
    ];
  },
};

export default nextConfig;
