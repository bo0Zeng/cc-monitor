// auto-e2e F-E3:ESM resolve 钩子——把 account-restart.ts 真源图里那道 Linux 结构性不可达的
// Tauri IPC 边界 `@tauri-apps/api/core` 重定向到 e2e core shim,并把 `./error-toast`（重 DOM）
// 换成写日志的 shim。**其余模块（accounts.ts / remote-launch.ts / remote-launch-run.ts /
// config.ts / agent-profile.ts …）全部加载真身**,它们内部的 invoke 也自然经此钩子落到真 tmux。
// 由 restart-cmd-driver.ts 在动态 import 真源之前 module.register 进来（tsx 转译钩子之上再叠一层）。
const CORE = new URL("./core.mjs", import.meta.url).href;
const TOAST = new URL("./error-toast.mjs", import.meta.url).href;
// 换号重启等「那台报出会话起来了」：事件那一层换成站在主窗口位置的替身（`event.mjs` 头注）。
const EVENT = new URL("./event.mjs", import.meta.url).href;

export async function resolve(spec, ctx, next) {
  if (spec === "@tauri-apps/api/core") return { url: CORE, shortCircuit: true };
  if (spec === "@tauri-apps/api/event") return { url: EVENT, shortCircuit: true };
  const r = await next(spec, ctx);
  if (r.url.endsWith("/error-toast.ts") || r.url.endsWith("/error-toast")) {
    return { url: TOAST, shortCircuit: true };
  }
  return r;
}

// CSS 模块（`import s from "./x.module.css"`）node 自己载不了（`ERR_UNKNOWN_FILE_EXTENSION`），
// 而真源图里有一个：`account-restart.ts` → `ask-dialog.ts` → `ask-dialog.module.css`（重构时加进来的，
// 从那天起本驱动器一行都跑不起来，restart / restart-frames 两套在本机与 CI 同红）。
// ⇒ 载成「类名映射到自身」的替身：命令级驱动器不渲染 DOM，类名是什么都不影响判的东西；
//   vitest 那侧有自己的 CSS 模块处理，与这里无关。只认 `.module.css`，别的扩展照旧交给下一层（不吞错）。
export async function load(url, ctx, next) {
  if (url.endsWith(".module.css")) {
    return {
      format: "module",
      source: "export default new Proxy({}, { get: (_, k) => String(k) });",
      shortCircuit: true,
    };
  }
  return next(url, ctx);
}
