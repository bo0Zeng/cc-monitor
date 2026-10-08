/**
 * 停下 / 重启 / 更新 / 卸载那台之前「会打断什么」：问那台后端（`machine-interrupts`：经它中转的活会话 · 活着的会话）
 * ＋ 问本机后端（同一条，`machine` = 那台：通往那台的端口转发），两份事实并排排成对话框里的两行（中断 · 保留）。
 * 什么都不会断 ⇒ 空，调用方就不弹框。这里不判什么，只排版。
 */
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "../ipc/chan-caller";
import { LOCAL_ORIGIN, isLocalOrigin } from "../ipc/origin";
import { copyText } from "../copy-table";
import type { DialogRow } from "../kit/dialog";
import { exactKeys, isObj } from "../ipc/decode";

/** 那两份事实并起来（问不到的那一格是 `null`，不当成 0）。 */
export interface Interrupts {
  relayedSessions: number | null;
  relayedMaybe: number;
  liveStreams: number;
  forwards: number | null;
}

/** 一份 `machine-interrupts` 应答（严格收：恰好四格、都是非负整数）。 */
export interface InterruptsReply {
  relayedSessions: number;
  relayedMaybe: number;
  liveStreams: number;
  forwards: number;
}

const KEYS = ["forwards", "liveStreams", "relayedMaybe", "relayedSessions"];

/** 严格收；收不下 ⇒ `null`（当成问不到）。 */
export function decodeInterrupts(v: unknown): InterruptsReply | null {
  if (!isObj(v) || !exactKeys(v, KEYS)) return null;
  const o = v;
  if (!KEYS.every((k) => Number.isInteger(o[k]) && (o[k] as number) >= 0)) return null;
  return o as unknown as InterruptsReply;
}

/** 那一问的期限：那台扫一遍活会话 ＋ 回程。 */
const INTERRUPTS_BUDGET_MS = 10_000;

/** 问一家（那台 / 本机）；问不到 ⇒ `null`。`appExit` ⇒ 问的是「重启 / 退出 cc-monitor」（那台后端按它自己的退出行为答）。 */
async function askOne(origin: string, machine: string | null, appExit = false): Promise<InterruptsReply | null> {
  try {
    const body = jsonBody(appExit ? { appExit: true } : machine === null ? {} : { machine });
    const budget = budgetWithin(INTERRUPTS_BUDGET_MS);
    const reply = await chan.call(origin, "machine-interrupts", body, budget);
    return decodeInterrupts(readJson(reply));
  } catch (e) {
    console.warn(`[interrupts] ${origin} 问不到会打断什么：`, e);
    return null;
  }
}

/** 那台的会话三格 ＋ 本机账上通往那台的转发（本机那一台只问一家）。 */
export async function askInterrupts(origin: string): Promise<Interrupts> {
  const local = isLocalOrigin(origin);
  const [there, here] = await Promise.all([askOne(origin, null), local ? Promise.resolve(null) : askOne(LOCAL_ORIGIN, origin)]);
  return {
    relayedSessions: there?.relayedSessions ?? null,
    relayedMaybe: there?.relayedMaybe ?? 0,
    liveStreams: there?.liveStreams ?? 0,
    forwards: local ? 0 : (here?.forwards ?? null),
  };
}

/**
 * 「现在重启 cc-monitor」之前：问本机与各台（`appExit`，各台照它自己的退出行为答会不会跟着停），加起来。
 * 本机问不到 ⇒ 会话那一格 `null`（说数不出）；远端问不到 ⇒ 那台本来就没连着，不算。
 */
export async function askAppExitInterrupts(remotes: readonly string[]): Promise<Interrupts> {
  const [here, ...there] = await Promise.all([askOne(LOCAL_ORIGIN, null, true), ...remotes.map((o) => askOne(o, null, true))]);
  const got = [here, ...there].filter((x): x is InterruptsReply => x !== null);
  const sum = (k: keyof InterruptsReply): number => got.reduce((n, x) => n + x[k], 0);
  return {
    relayedSessions: here === null ? null : sum("relayedSessions"),
    relayedMaybe: sum("relayedMaybe"),
    liveStreams: sum("liveStreams"),
    forwards: here === null ? null : sum("forwards"),
  };
}

/** 那一下要停多久：`stop` / `uninstall` 停到再启动；`restart` / `update` 停几秒。 */
export type InterruptAct = "stop" | "restart" | "update" | "uninstall";

/** 会断的那几项排成「中断 · 保留」两行；一项都没有 ⇒ `[]`。 */
export function interruptRows(i: Interrupts | null, machine: string, act: InterruptAct): DialogRow[] {
  const brief = act === "restart" || act === "update";
  const items: string[] = [];
  if (i === null || i.relayedSessions === null) items.push(copyText("interrupts.item.relayedUnknown"));
  else if (i.relayedSessions > 0) items.push(copyText("interrupts.item.relayed", { n: i.relayedSessions }));
  if (i !== null && i.relayedMaybe > 0) items.push(copyText("interrupts.item.relayedMaybe", { n: i.relayedMaybe }));
  if (i !== null && i.liveStreams > 0) {
    items.push(brief ? copyText("interrupts.item.liveBrief", { machine }) : copyText("interrupts.item.liveGone", { machine }));
  }
  if (i !== null && i.forwards === null) items.push(copyText("interrupts.item.forwardsUnknown"));
  else if (i !== null && i.forwards !== null && i.forwards > 0) items.push(copyText("interrupts.item.forwards", { n: i.forwards }));
  if (items.length === 0) return [];
  return [
    { label: brief ? copyText("interrupts.row.brief") : copyText("interrupts.row.untilStart"), items },
    { label: copyText("interrupts.row.keep"), items: [copyText("interrupts.item.sessionsRun")] },
  ];
}
