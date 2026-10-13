/**
 * 首次运行「开始用 · 剩 N 步」那份数（本机后端 `first-run`）：三步各自打没打勾、剩几步都是后端判的；
 * 第三步「加一台远端机器」要的机器表台数住 monitor 那一侧 ⇒ 问的时候带上（`remotes`），后端判。
 * 哪步必做 · 剩几步（只数必做的）· 点过「跳过」没有，也都是后端给的。设置窗「开始用」那一块与主窗口状态栏那一枚读这一份。
 * 问不到 ⇒ `null`（不当 0，也不当没做完）。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../ipc/origin";
import { readRemoteConfig } from "../remote-config";
import { markChore } from "./data-reads";
import type { FirstRun } from "../generated/FirstRun";
import type { FirstRunStepId } from "../generated/FirstRunStepId";

export type ReadinessStep = FirstRunStepId;

/** 三步（`required` ＝ 必做，后端定）· 必做而还没打勾的几步（状态栏那一枚只数它）· 「开始用」那一块点过「跳过」没有（后端记着）。形状由握手指纹核过。 */
export type Readiness = FirstRun;

/** 读几份启动文件 ＋ 一份账号库清单：10 秒。 */
const BUDGET_MS = 10_000;

/** 问本机后端一次（带上机器表台数：调用方手上有就给，没有就现读一次配置）。 */
export async function readReadiness(known?: number): Promise<Readiness | null> {
  try {
    const remotes = known ?? (await readRemoteConfig()).hosts.length;
    return await callTyped(LOCAL_ORIGIN, "first-run", { remotes }, budgetWithin(BUDGET_MS));
  } catch (e) {
    console.warn("[readiness] 问不到「开始用」那份数：", e);
    return null;
  }
}

/** 「开始用」那一块点了「跳过」：交本机后端记下（`chores-mark` 的 `skipStart`，写它自己的 `~/.cc-monitor/chores.json`）。失败抛一句人话。 */
export async function skipStart(): Promise<void> {
  await markChore(LOCAL_ORIGIN, { op: "skipStart" });
}
