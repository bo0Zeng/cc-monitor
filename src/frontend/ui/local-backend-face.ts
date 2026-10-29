/**
 * 本机后端没起来那一格怎么画（壳的 `local-backend`）：设置窗（导航点 · 列表那一行 · 卡头）与主窗口栏底机器条本机那一行共用这一份。
 * 字与修法照壳那一格，这里只按那一样挑点与语气。单住一份：主窗口引它不带进设置窗问题行的画法与样式。
 */
import type { LocalBackend } from "./generated/LocalBackend";
import type { MachineFace } from "./settings/machine-state";

/** 连着 ⇒ `null`（看本机后端那一台的成品）。 */
export function localBackendFace(l: LocalBackend): MachineFace | null {
  if (l.state === "up") return null;
  const look = {
    starting: { dot: "unknown", tone: "busy" },
    down: { dot: "failed", tone: "error" },
    older: { dot: "needs-you", tone: "warn" },
    newer: { dot: "up", tone: "error" },
  } as const;
  return {
    ...look[l.state],
    word: l.word,
    problem: l.said,
    bar: false,
    fixes: l.state === "starting" ? [] : l.fixes,
    detail: "",
    next: l.state === "starting" ? "" : (l.next ?? ""),
    route: "",
  };
}
