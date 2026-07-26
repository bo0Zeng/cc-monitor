// auto-e2e F-E4:ESM resolve/load 钩子——让 `src/tabs.ts` 真源图能在 headless node(tsx) 下加载,
// 以便 orphan-cmd-driver.ts 驱动**真的** `findOrphanTmux` + `cleanupOrphanTmux`(不重写一份)。
//
// 三处重定向(其余模块全加载真身,它们内部的 invoke 也自然经此钩子落到真 tmux):
//   1. `@tauri-apps/api/core`   → ./core.mjs   (invoke 重定向到**真 tmux**;并补 tabs.ts 图里
//                                 其它模块要的 Channel/convertFileSrc 等导出,否则 import 报缺导出)。
//   2. `./error-toast`          → 复用 restart-shims/error-toast.mjs (真身操作 DOM,headless 崩;
//                                 换成写 $CCM_TOAST_LOG,套件据此断言 toast 结果)。
//   3. `*.css`                  → 空模块 (tabs.ts 经 ./cards 拉 highlight.js 的 .css;node 不识别
//                                 .css 扩展会抛 "Unknown file extension")。
// 诚实层级:Linux headless 下 Tauri IPC + GUI 触发结构性不可达;本钩子只替换那道本该由后端 Rust
// 执行 tmux 的 IPC 边界 → 被测代码是真的 tabs.ts 孤儿判据 + 清理编排 + 真 tmux 效果。红线:daemon
// 零改(不跑它);真 tmux 操作**严格限定在套件本轮建的 fixture 会话**(core.mjs 白名单,见其头注)。
const CORE = new URL("./core.mjs", import.meta.url).href;
const TOAST = new URL("../restart-shims/error-toast.mjs", import.meta.url).href;

export async function resolve(spec, ctx, next) {
  if (spec === "@tauri-apps/api/core") return { url: CORE, shortCircuit: true };
  const r = await next(spec, ctx);
  if (r.url.endsWith("/error-toast.ts") || r.url.endsWith("/error-toast")) {
    return { url: TOAST, shortCircuit: true };
  }
  return r;
}

export async function load(url, ctx, next) {
  if (url.endsWith(".css")) {
    return { format: "module", source: "export default {};", shortCircuit: true };
  }
  return next(url, ctx);
}
