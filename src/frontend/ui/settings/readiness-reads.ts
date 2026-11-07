/**
 * 首次运行「开始用」那一份（本机后端 `first-run`）：每步打没打勾 · 名字 · 那一句 · 按钮的字与去处 · 点过「跳过」没有，都是后端给的；
 * 「加一台远端机器」那一步要的机器表台数住 monitor 那一侧 ⇒ 问的时候带上（`remotes`），后端判。主窗口空态的「开始用」卡读这一份。
 * 问不到 ⇒ `null`（不当做完，也不当没做完）。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL } from "../ipc/origin";
import { readRemoteConfig } from "../remote-config";
import { markChore } from "./data-reads";
import type { FirstRun } from "../generated/FirstRun";

/** 每步（后端写好的字与去处）· 点过「跳过」没有（后端记着）。形状由握手指纹核过。 */
export type Readiness = FirstRun;

/** 读几份启动文件：10 秒。 */
const BUDGET_MS = 10_000;

/** 问本机后端一次（带上机器表台数：调用方手上有就给，没有就现读一次配置）。 */
export async function readReadiness(known?: number): Promise<Readiness | null> {
  try {
    const remotes = known ?? (await readRemoteConfig()).hosts.length;
    return await callTyped(LOCAL, "first-run", { remotes }, budgetWithin(BUDGET_MS));
  } catch (e) {
    console.warn("[readiness] 问不到「开始用」那份数：", e);
    return null;
  }
}

/** 「开始用」那一块点了「跳过」：交本机后端记下（`chores-mark` 的 `skipStart`，写它自己的 `~/.cc-monitor/chores.json`）。失败抛一句人话。 */
export async function skipStart(): Promise<void> {
  await markChore(LOCAL, { op: "skipStart" });
}
