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
// src/bridge/tauri.conf.json 的 devUrl 改成同一端口。详见 src/doc/DEVELOPMENT.md。
// @ts-expect-error process is a nodejs global
const port = Number(process.env.VITE_PORT) || 24174;
const hmrPort = port + 1;

// https://vite.dev/config/
export default defineConfig(async () => ({

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 〔仓库重组 2026-09-17〕构建输出挪出仓根，与两个 cargo target 同处 `.build/`
  //（理由住 `.cargo/config.toml` 头注：`src/` 里不许有生成代码，否则扫源码树的判据会
  // 安静地扫错东西）。`src/bridge/tauri.conf.json` 的 `frontendDist` 同拍改成 `../../.build/dist`。
  build: {
    outDir: ".build/dist",
    emptyOutDir: true,
    // 〔三入口拆分 · `设计/01 §1.2`〕三个 html 各带一个入口模块，各自一张模块图：
    //   index.html    → src/entry-main.ts      主窗口
    //   settings.html → src/entry-settings.ts  只含设置面板 ＋ 主题 ＋ 键位
    //   viewer.html   → src/entry-viewer.ts    只含 tab 管理 ＋ 渲染栈
    // Tauri 开窗那两处（`src/bridge/src/lib.rs` 的 `open_settings_window` /
    // `open_session_in_new_window`）指向后两个 html。
    // 🔴 「设置窗里没有高亮/数学/tab 管理」由 `tests/entry-graphs.vitest.ts` 对**本配置真跑出来的**
    //    构建产物做零命中断言 —— 它读的就是这张 `input` 表，改这里那边跟着变，不存在两份副本。
    rollupOptions: {
      input: {
        main: "index.html",
        settings: "settings.html",
        viewer: "viewer.html",
      },
    },
  },
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
      // 3. tell Vite to ignore watching `src/bridge`
      ignored: ["**/src/bridge/**"],
    },
  },
}));
