/**
 * 首次运行「开始用 · 剩 N 步」那份数（本机后端 `readiness`）：三步各自打没打勾、剩几步都是后端判的；
 * 第三步「加一台远端机器」要的机器表台数住 monitor 那一侧 ⇒ 问的时候带上（`remotes`），后端判。
 * 哪步必做 · 剩几步（只数必做的）· 点过「跳过」没有，也都是后端给的。设置窗「开始用」那一块与主窗口状态栏那一枚读这一份。
 * 问不到 ⇒ `null`（不当 0，也不当没做完）。
 */
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../ipc/origin";
import { exactKeys, isObj } from "../ipc/decode";
import { readRemoteConfig } from "../remote-config";
import { markChore } from "./data-reads";

export type ReadinessStep = "terminal" | "named" | "remote";

export interface Readiness {
  /** 三步；`required` ＝ 必做（后端定）。 */
  steps: { id: ReadinessStep; done: boolean; required: boolean }[];
  /** 必做而还没打勾的几步（状态栏那一枚只数它）。 */
  left: number;
  /** 「开始用」那一块点过「跳过」（后端记着）。 */
  skipped: boolean;
}

const STEPS: readonly string[] = ["terminal", "named", "remote"];
/** 读几份启动文件 ＋ 一份账号库清单：10 秒。 */
const BUDGET_MS = 10_000;

/** 严格收；收不下 ⇒ `null`。 */
export function decodeReadiness(v: unknown): Readiness | null {
  if (!isObj(v) || !exactKeys(v, ["steps", "left", "skipped"]) || !Array.isArray(v.steps) || typeof v.skipped !== "boolean") return null;
  if (!(typeof v.left === "number" && Number.isInteger(v.left) && v.left >= 0)) return null;
  const steps: Readiness["steps"] = [];
  for (const s of v.steps) {
    if (!isObj(s) || !exactKeys(s, ["id", "done", "required"]) || typeof s.id !== "string" || !STEPS.includes(s.id) || typeof s.done !== "boolean" || typeof s.required !== "boolean") return null;
    steps.push({ id: s.id as ReadinessStep, done: s.done, required: s.required });
  }
  return { steps, left: v.left, skipped: v.skipped };
}

/** 问本机后端一次（带上机器表台数：调用方手上有就给，没有就现读一次配置）。 */
export async function readReadiness(known?: number): Promise<Readiness | null> {
  try {
    const remotes = known ?? (await readRemoteConfig()).hosts.length;
    const body = jsonBody({ remotes });
    const budget = budgetWithin(BUDGET_MS);
    return decodeReadiness(readJson(await chan.call(LOCAL_ORIGIN, "readiness", body, budget)));
  } catch (e) {
    console.warn("[readiness] 问不到「开始用」那份数：", e);
    return null;
  }
}

/** 「开始用」那一块点了「跳过」：交本机后端记下（`chores-mark` 的 `skipStart`，写它自己的 `~/.cc-monitor/chores.json`）。失败抛一句人话。 */
export async function skipStart(): Promise<void> {
  await markChore(LOCAL_ORIGIN, { op: "skipStart" });
}
