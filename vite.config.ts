import { fileURLToPath } from "node:url";
import process from "node:process";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // Generated TS bindings (ts-rs, committed). The UI imports contract types only via `@bindings/*`.
    alias: { "@bindings": fileURLToPath(new URL("./crates/ss-core/bindings", import.meta.url)) },
  },
  // Tauri expects a fixed port and must see Rust errors.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/target/**", "**/sidecar/**"] },
  },
});
