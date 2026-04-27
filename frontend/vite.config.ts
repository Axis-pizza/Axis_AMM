import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Vite config for the Axis AMM devnet demo. We pin the dev port at 5173
// because the demo is purely client-side — no API routes, no SSR — and
// the e2e team prefers a stable local origin for screenshots / videos.
//
// The Solana wallet adapter uses Buffer; without the alias the
// `process is not defined` browser error fires on first wallet
// detection. We polyfill via a tiny shim in src/lib/buffer-shim.ts
// rather than pulling in `vite-plugin-node-polyfills` (which adds
// 1.2 MB to the bundle for no real reason).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: { port: 5173 },
  define: {
    "process.env": {},
    global: "globalThis",
  },
});
