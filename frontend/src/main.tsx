import { Buffer } from "buffer";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";

// The Solana wallet-adapter stack assumes a Node-ish global. Browsers
// don't provide one, and not setting these on `window` causes the
// Phantom/Solflare auto-detect path to crash on first render.
(window as unknown as { Buffer: typeof Buffer }).Buffer = Buffer;
(window as unknown as { global: typeof globalThis }).global = window;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
