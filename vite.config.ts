import { defineConfig } from "vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// 端口可通过 VITE_PORT 覆盖（默认 5174，Vite 自身默认值，绝大多数机器不冲突）。
// HMR 端口 = VITE_PORT + 1。
// 历史上曾用 1420（Tauri 默认），实测 Windows Hyper-V 把 1366-1465 列入动态保留段，
// 在很多机器上启动报 EACCES: permission denied。详见 README "故障排查" 段。
// 若 5174 也被占用，设环境变量例：$env:VITE_PORT=3000 后重跑；同时把
// src-tauri/tauri.conf.json 的 devUrl 改成同一端口。
// @ts-expect-error process is a nodejs global
const port = Number(process.env.VITE_PORT) || 5174;
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
