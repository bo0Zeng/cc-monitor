/**
 * 首次运行「开始用 · 剩 N 步」那份数（本机后端 `readiness`）：三步各自打没打勾、剩几步都是后端判的；
 * 第三步「加一台远端机器」要的机器表台数住 monitor 那一侧 ⇒ 问的时候带上（`remotes`），后端判。
 * 设置窗「开始用」那一块与主窗口状态栏那一枚读这一个数。问不到 ⇒ `null`（不当 0，也不当没做完）。
 */
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../ipc/origin";
import { exactKeys, isObj } from "../ipc/decode";
import { readRemoteConfig } from "../remote-config";

export type ReadinessStep = "terminal" | "named" | "remote";

export interface Readiness {
  steps: { id: ReadinessStep; done: boolean }[];
  left: number;
}

const STEPS: readonly string[] = ["terminal", "named", "remote"];
/** 读几份启动文件 ＋ 一份账号库清单：10 秒。 */
const BUDGET_MS = 10_000;

/** 严格收；收不下 ⇒ `null`。 */
export function decodeReadiness(v: unknown): Readiness | null {
  if (!isObj(v) || !exactKeys(v, ["steps", "left"]) || !Array.isArray(v.steps)) return null;
  if (!(typeof v.left === "number" && Number.isInteger(v.left) && v.left >= 0)) return null;
  const steps: Readiness["steps"] = [];
  for (const s of v.steps) {
    if (!isObj(s) || !exactKeys(s, ["id", "done"]) || typeof s.id !== "string" || !STEPS.includes(s.id) || typeof s.done !== "boolean") return null;
    steps.push({ id: s.id as ReadinessStep, done: s.done });
  }
  return { steps, left: v.left };
}

/** 问本机后端一次（带上机器表台数）。 */
export async function readReadiness(): Promise<Readiness | null> {
  try {
    const remotes = (await readRemoteConfig()).hosts.length;
    const body = jsonBody({ remotes });
    const budget = budgetWithin(BUDGET_MS);
    return decodeReadiness(readJson(await chan.call(LOCAL_ORIGIN, "readiness", body, budget)));
  } catch (e) {
    console.warn("[readiness] 问不到「开始用」那份数：", e);
    return null;
  }
}

/** 「开始用」那一块点过「跳过」（界面偏好：这一台 cc-monitor 记住）。 */
export function firstRunSkipped(): boolean {
  try {
    return localStorage.getItem("cc-monitor.settings.first-run-skipped") === "1";
  } catch {
    return false;
  }
}

export function skipFirstRun(): void {
  try {
    localStorage.setItem("cc-monitor.settings.first-run-skipped", "1");
  } catch (e) {
    console.warn("[readiness] 记不下「跳过」：", e);
  }
}
