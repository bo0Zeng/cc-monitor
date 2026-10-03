/**
 * 设置里填的「Claude 数据目录」存之前先问本机后端那个目录在不在（`files-stat`）：
 * 不在的目录照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它。
 */
import { chan, ChanError } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidOf } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";

/** 问一个路径的期限（本机，毫秒级；给 10 秒）。 */
const STAT_BUDGET_MS = 10_000;

/** 那个目录能用 ⇒ `null`；不能用 ⇒ 给人看的一句（不在 · 不是目录 · 问不到）。 */
export async function claudeDirProblem(dir: string): Promise<string | null> {
  let kind: unknown;
  try {
    const body = jsonBody({ path: dir });
    const budget = budgetWithin(STAT_BUDGET_MS);
    const v = readJson(await chan.call(LOCAL_ORIGIN, "files-stat", body, budget));
    kind = (v as { kind?: unknown } | null)?.kind;
  } catch (e) {
    const refused = e instanceof ChanError && e.error.layer === "peer" && e.error.why === "refused" ? refusalOf(e.error.body)?.code : undefined;
    if (refused === "unreadable" || refused === "bad_path") return copyText("settingsPanel.claudeDir.missing", { path: dir });
    return copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: saidOf(e, copyText("chanCaller.said.error")) });
  }
  return kind === "dir" ? null : copyText("settingsPanel.claudeDir.notDir", { path: dir });
}
