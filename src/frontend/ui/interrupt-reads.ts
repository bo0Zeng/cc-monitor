/**
 * 「会打断什么」那一问（`session-interrupts`）：经通道问会话所在那台的后端，按形状收（不猜、不补）。
 * 判定只住后端；界面那一侧的 2 秒等待在 `kit/interrupts.ts`（通道这一跳的期限同值，到点就当没答）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { SessionInterrupts } from "./generated/SessionInterrupts";
import { INTERRUPTS_WITHIN_MS, type Interrupts } from "./kit/interrupts";

const KINDS = new Set(["turn", "agent", "task"]);

/** 应答 ⇒ 按族的清单；形状不对 ⇒ 抛（调用方当没答）。 */
export function decodeInterrupts(v: unknown): Interrupts {
  const r = v as Partial<SessionInterrupts> | null;
  if (!r || !Array.isArray(r.families)) throw new Error("session-interrupts: unreadable reply");
  return {
    families: r.families.map((f) => {
      if (!f || !KINDS.has(f.family) || !Array.isArray(f.names) || f.names.some((n) => typeof n !== "string")) {
        throw new Error("session-interrupts: unreadable family");
      }
      return { family: f.family, names: [...f.names] };
    }),
  };
}

/** 问 `origin` 那台：动 `sid` 这个会话会打断什么。 */
export async function askSessionInterrupts(origin: Origin, sid: string): Promise<Interrupts> {
  const budget = budgetWithin(INTERRUPTS_WITHIN_MS);
  const body = jsonBody({ sid });
  const reply = await chan.call(origin, "session-interrupts", body, budget);
  return decodeInterrupts(readJson(reply));
}
