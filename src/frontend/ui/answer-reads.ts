/**
 * 答一条挂着的待答请求（帧命令 `needs-answer`，`src/doc/IPC-COMMANDS.md`）：经通道问会话所在那台的后端。
 * 结局三种（[`AnswerOutcome`]）：交到了 · 别处先答了 / 撤了（回包 `result: "gone"`，那一句是那台写好的）·
 * 没答成（被拒、够不着 —— 那一句 monitor 写好，块照旧可答）。入参 · 回包按类型（`callTyped`）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { settleTyped } from "./control-said";
import type { AnswerOutcome, AnswerRequest } from "./answer-block";
import { detailOf, saidOf } from "./kit/detail";

/** 这一跳等多久：答是就地登记、立刻回（hook 那一头另算），够不着的那几层按通道说。 */
const ANSWER_WITHIN_MS = 15_000;

export async function answerNeeds(origin: Origin, sid: string, req: AnswerRequest): Promise<AnswerOutcome> {
  const budget = budgetWithin(ANSWER_WITHIN_MS);
  try {
    // 回包两种结局（形状由握手指纹管）：答上了 · 别处已答（带那台写好的那一句）。
    const r = await settleTyped(origin, "needs-answer", callTyped(origin, "needs-answer", { sid, ...req }, budget));
    return r.result === "gone" ? { kind: "gone", said: r.said ?? "" } : { kind: "answered" };
  } catch (e) {
    return { kind: "failed", said: saidOf(e), detail: detailOf(e) };
  }
}
