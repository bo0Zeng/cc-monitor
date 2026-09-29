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
// src/frontend/shell/tauri.conf.json 的 devUrl 改成同一端口。详见 src/doc/DEVELOPMENT.md。
// @ts-expect-error process is a nodejs global
const port = Number(process.env.VITE_PORT) || 24174;
const hmrPort = port + 1;

/**
 * 〔`设计/41 §3` · 层真包进去〕把**第三方**样式表（`node_modules` 里的 `.css`，今天是 highlight.js 主题
 * 与 KaTeX）整份包进 `@layer vendor`。
 *
 * 为什么必须包：`@layer` 的规则是「**无层的样式赢过所有有层的**」。我们自己的样式全部进了层之后，
 * 第三方那两份如果还是无层，就会**反过来压住我们所有的覆盖** —— 例如 `.code-block pre code.hljs`
 * 给代码块定的 `padding` / 透明底，会输给 hljs 主题的 `pre code.hljs { padding: 1em }` 与
 * `.hljs { background: … }`。包进 `vendor`（排在 `reset` 之后、我们所有层之前）⇒ 我们的规则一律压过第三方，
 * 与拆层之前「我们的覆盖靠更高特异度赢」的结果相同（离线核对见 `设计/41` 末尾追加的那一节）。
 *
 * 为什么在构建配置里包、而不是改成在 CSS 里 `@import … layer(vendor)`：这两份是 `src/frontend/ui/render.ts` 用
 * `import "<包>/….css"` 引进来的 —— `tests/evidence/S25-class-ledger.ts` 的「第三方类名」一族正是从这一形
 * 现读的，改走 CSS `@import` 会让那本账看不见 `.katex-*` 而误判死规则。
 *
 * `@font-face` / `@charset` / `@import` 留在层外（字体声明不参与层叠；后两者按语法必须在最前）。
 * 判据：`tests/frontend/ui/entry-graphs.vitest.ts`「构建产物里一条无层规则都没有」—— 这个插件没生效，那条当场红。
 */
const vendorLayer = {
  postcssPlugin: "cc-monitor-vendor-layer",
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  Once(root: any, helpers: any) {
    const file: string = root.source?.input?.file ?? "";
    if (!/[\\/]node_modules[\\/]/.test(file)) return;
    const keepOut = new Set(["font-face", "charset", "import", "layer"]);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const inner = root.nodes.filter((n: any) => !(n.type === "atrule" && keepOut.has(n.name)));
    if (inner.length === 0) return;
    const layer = new helpers.AtRule({ name: "layer", params: "vendor" });
    for (const n of inner) layer.append(n.remove());
    root.append(layer);
  },
};

// https://vite.dev/config/
export default defineConfig(async () => ({
  css: {
    postcss: { plugins: [vendorLayer] },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 〔仓库重组 2026-09-17〕构建输出挪出仓根，与两个 cargo target 同处 `.build/`
  //（理由住 `.cargo/config.toml` 头注：`src/` 里不许有生成代码，否则扫源码树的判据会
  // 安静地扫错东西）。`src/frontend/shell/tauri.conf.json` 的 `frontendDist` 同拍改成 `../../.build/dist`。
  build: {
    outDir: ".build/dist",
    emptyOutDir: true,
    // 〔三入口拆分 · `设计/01 §1.2`〕三个 html 各带一个入口模块，各自一张模块图：
    //   index.html    → src/frontend/ui/entry-main.ts      主窗口
    //   settings.html → src/frontend/ui/entry-settings.ts  只含设置面板 ＋ 主题 ＋ 键位
    //   viewer.html   → src/frontend/ui/entry-viewer.ts    只含 tab 管理 ＋ 渲染栈
    // Tauri 开窗那两处（`src/frontend/shell/src/lib.rs` 的 `open_settings_window` /
    // `open_session_in_new_window`）指向后两个 html。
    // 🔴 「设置窗里没有高亮/数学/tab 管理」由 `tests/frontend/ui/entry-graphs.vitest.ts` 对**本配置真跑出来的**
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
      // 3. tell Vite to ignore watching `src/frontend/shell`
      ignored: ["**/src/frontend/shell/**"],
    },
  },
}));
