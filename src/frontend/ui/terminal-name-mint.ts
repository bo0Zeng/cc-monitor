/**
 * **起会话时那个 tmux 会话名从哪来 —— 本机与远端同一个家。**
 *
 * 守的要求：「**一个判定只有一个家**」＋ D4「**一条都不许静默忽略**」；
 * 「tmux 名派生 ＋ 撞名避让只留后端」。
 *
 * # 名字是那台后端铸的
 *
 * 「从 cwd 派生基名 → 撞名避让」只在后端（`control/ccm/plan.rs`，终端里敲 `ccm` 也走它）：要名字就问**那台**后端的
 * `terminal-name-mint`（`{cwd}` ⇒ `<项目名>-cc`，按那台那张会话快照避让）—— 规则与名单都在那一台，
 * 于是界面铸出来的名字与终端里在同一目录敲 `ccm` 铸的是同一个。分叉出来的那一条由那台在起会话时自己铸（`sessions-start` 的 `fresh_terminal`）。
 *
 * # 规则只有一条：问不到 ⇒ 不铸名
 *
 * 问不到（链路断 · 那台后端比这一问老 · 回的形状不认）⇒ [`MintOutcome`] `ok:false` 带原因，**不许**自己拼一个名字顶上 ——
 * 不避让的名字就是 issue #76 的形状：同一个 cwd 派生出同一个名字，撞上已有会话，**静默接进第一个会话，而用户以为开了新的**。
 * 「不铸名」之后怎么办归调用方：今天只有本机那一处问（把 `null` 交给后端，它有一条如实的「不进容器」旧路）。
 */
import type { Origin } from "./ipc/origin";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, peerVersionSaid, readJson, saidFrom } from "./ipc/chan-caller";

export type MintOutcome = { ok: true; name: string } | { ok: false; why: string };

/** 期限：那台后端问一次会话快照（起一次 `tmux ls`）就答；15 s 盖住回程（远端没连着还要握手），与列终端名单同一档。 */
const MINT_BUDGET_MS = 15_000;

/** `terminal-name-mint` 的成品 `{name}` ⇒ 名字；形状不认 ⇒ `null`。严格收（恰好一个键、非空串）。 */
export function decodeMinted(v: unknown): string | null {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  const keys = Object.keys(o);
  return keys.length === 1 && keys[0] === "name" && typeof o.name === "string" && o.name !== "" ? o.name : null;
}

/** 问那台后端铸一个名字。**不抛**。 */
async function askMint(origin: Origin, args: { cwd: string }): Promise<MintOutcome> {
  let reply: Uint8Array;
  try {
    const body = jsonBody(args);
    const budget = budgetWithin(MINT_BUDGET_MS);
    reply = await chan.call(origin, "terminal-name-mint", body, budget);
  } catch (e) {
    return { ok: false, why: saidFrom(e, origin) };
  }
  let name: string | null = null;
  try {
    name = decodeMinted(readJson(reply));
  } catch {
    name = null;
  }
  return name !== null ? { ok: true, name } : { ok: false, why: peerVersionSaid("reply_unreadable", origin) };
}

/** 起新会话 / 全新 resume：那台按 cwd 派生 `<项目名>-cc`、按它的会话快照避让。 */
export function mintFreshTmuxName(origin: Origin, cwd: string): Promise<MintOutcome> {
  return askMint(origin, { cwd });
}
