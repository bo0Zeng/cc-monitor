/**
 * 设置里填的「Claude 目录」存之前问本机后端一次（`agent-home-check`）：后端判在不在 · 是不是目录 · 有没有记录树，回写好的那一句；这里照抄。
 * 不在的目录照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它；没有 `projects/` 的目录读不出一条会话。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL } from "../ipc/origin";
import { copyText } from "../copy-table";
import { saidOf } from "../kit/detail";

/** 问一次的期限（本机，毫秒级；给 10 秒）。 */
const CHECK_BUDGET_MS = 10_000;

/** 那个目录能用 ⇒ `null`；不能用 ⇒ 后端写好的那一句；问不到 ⇒ 说没法确认。 */
export async function claudeDirProblem(dir: string): Promise<string | null> {
  try {
    return (await callTyped(LOCAL, "agent-home-check", { path: dir }, budgetWithin(CHECK_BUDGET_MS))).said;
  } catch (e) {
    return copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: saidOf(e) });
  }
}
