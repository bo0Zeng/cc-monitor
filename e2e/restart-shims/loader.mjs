// auto-e2e F-E3:ESM resolve 钩子——把 account-restart.ts 真源图里那道 Linux 结构性不可达的
// Tauri IPC 边界 `@tauri-apps/api/core` 重定向到 e2e core shim,并把 `./error-toast`（重 DOM）
// 换成写日志的 shim。**其余模块（accounts.ts / remote-launch.ts / remote-launch-run.ts /
// config.ts / agent-profile.ts …）全部加载真身**,它们内部的 invoke 也自然经此钩子落到真 tmux。
// 由 restart-cmd-driver.ts 在动态 import 真源之前 module.register 进来（tsx 转译钩子之上再叠一层）。
const CORE = new URL("./core.mjs", import.meta.url).href;
const TOAST = new URL("./error-toast.mjs", import.meta.url).href;

export async function resolve(spec, ctx, next) {
  if (spec === "@tauri-apps/api/core") return { url: CORE, shortCircuit: true };
  const r = await next(spec, ctx);
  if (r.url.endsWith("/error-toast.ts") || r.url.endsWith("/error-toast")) {
    return { url: TOAST, shortCircuit: true };
  }
  return r;
}
