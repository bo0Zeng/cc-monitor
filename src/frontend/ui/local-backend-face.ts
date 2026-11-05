/**
 * 本机后端没起来那一格怎么画（壳的 `local-backend`）：设置窗（导航点 · 列表那一行 · 卡头）与主窗口栏底机器条本机那一行共用这一份。
 * 字 · 修法 · 点画哪一档 · 语气都是壳那一格写好的；点的样子照机器那一张表（`machineDot`），这里只装成机器卡的样子。
 * 单住一份：主窗口引它不带进设置窗问题行的画法与样式。
 */
import type { LocalBackend } from "./generated/LocalBackend";
import type { Tone } from "./generated/Tone";
import { machineDot } from "./machine-dot";
import type { MachineFace } from "./settings/machine-state";

/** 壳写的语气 ⇒ 问题行的样子（壳只写这三种）。 */
const TONE: Partial<Record<Tone, MachineFace["tone"]>> = { busy: "busy", warn: "warn", fail: "error" };

/** 连着 ⇒ `null`（看本机后端那一台的成品）。 */
export function localBackendFace(l: LocalBackend): MachineFace | null {
  if (l.state === "up") return null;
  const fixes = l.state === "starting" ? [] : l.fixes;
  const next = l.state === "starting" ? "" : (l.next ?? "");
  return {
    dot: machineDot(l.dot),
    tone: TONE[l.tone] ?? "error",
    word: l.word,
    problem: l.said,
    bar: false,
    fixes,
    detail: "",
    next,
    route: "",
  };
}
