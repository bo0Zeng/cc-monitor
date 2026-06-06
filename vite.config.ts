import { defineConfig } from "vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// 端口可通过 VITE_PORT 覆盖。HMR 端口 = VITE_PORT + 1。
// 默认选 24174：Windows 的 Hyper-V / WSL2 / WinNAT 会把一段段端口列入「动态保留」，
// 应用层 bind 时报 `EACCES: permission denied`（netstat 看不到占用进程，但 listen
// syscall 失败）。实测这些保留段落在较低区间（~1000–12500），而系统 ephemeral 段从
// 49152 起；故选 24174 这个「保留段之上、ephemeral 之下」的冷门高位端口，最不容易被占。
// 历史上踩过：1420（Tauri 默认，落 1366-1465 保留段）、5174（落 5110-5209 保留段）。
// 若 24174 仍被占，设环境变量例：$env:VITE_PORT=24500 后重跑，并把
// src-tauri/tauri.conf.json 的 devUrl 改成同一端口。详见 doc/DEVELOPMENT.md。
// @ts-expect-error process is a nodejs global
const port = Number(process.env.VITE_PORT) || 24174;
const hmrPort = port + 1;

// https://vite.dev/config/
export default defineConfig(async () => ({

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: hmrPort,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
