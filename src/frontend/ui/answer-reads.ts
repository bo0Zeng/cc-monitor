/**
 * 答一条挂着的待答请求（帧命令 `needs-answer`，`src/doc/IPC-COMMANDS.md`）：经通道问会话所在那台的后端。
 * 结局按码分三种（[`AnswerOutcome`]）：交到了 · 别处先答了 / 撤了（`already_answered` · `closed_elsewhere`，那一句是那台写好的）·
 * 没答成（别的码、够不着、回的认不出 —— 那一句照 `control-said.ts` 那一层说，块照旧可答）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, refusalOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { ControlError, asSaid, saidOfControl, settle } from "./control-said";
import { copyText } from "./copy-table";
import type { AnswerOutcome, AnswerRequest } from "./answer-block";

/** 这一跳等多久：答是就地登记、立刻回（hook 那一头另算），够不着的那几层按通道说。 */
const ANSWER_WITHIN_MS = 15_000;

/** 别处先答了 / 那一头撤了的两个码。 */
const GONE = new Set(["already_answered", "closed_elsewhere"]);

export async function answerNeeds(origin: Origin, sid: string, req: AnswerRequest): Promise<AnswerOutcome> {
  const body = jsonBody({ sid, ...req });
  try {
    const v = await settle(origin, "needs-answer", chan.call(origin, "needs-answer", body, budgetWithin(ANSWER_WITHIN_MS)), asSaid(() => copyText("askBlock.fail.noReason")));
    if (v === null || typeof v !== "object" || (v as { result?: unknown }).result !== "answered") {
      return { kind: "failed", said: copyText("askBlock.fail.noReason"), detail: "" };
    }
    return { kind: "answered" };
  } catch (e) {
    const err = e instanceof ControlError ? e.error : undefined;
    const code = err && err.layer === "peer" && err.why === "refused" ? refusalOf(err.body)?.code : undefined;
    if (code !== undefined && GONE.has(code)) return { kind: "gone", said: saidOfControl(e) };
    return { kind: "failed", said: saidOfControl(e), detail: e instanceof ControlError ? e.detail : "" };
  }
}
