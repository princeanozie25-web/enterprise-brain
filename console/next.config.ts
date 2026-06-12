import type { NextConfig } from "next";

// Static-export friendly: `next build` emits a fully local bundle (out/).
// No CDN scripts, no remote fonts, no analytics, no telemetry — everything
// the console ships is in this repo.
const nextConfig: NextConfig = {
  output: "export",
  reactStrictMode: true,
};

export default nextConfig;
