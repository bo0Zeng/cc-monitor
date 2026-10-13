/**
 * 设置里填的「Claude 目录」存之前问本机后端一次（`agent-home-check`）：后端判在不在 · 是不是目录 · 有没有记录树，回码；这里按码取那一句。
 * 不在的目录照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它；没有 `projects/` 的目录读不出一条会话。
 */
import { budgetWithin, callTyped, peerVersionSaid, saidFrom } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";

/** 问一次的期限（本机，毫秒级；给 10 秒）。 */
const CHECK_BUDGET_MS = 10_000;

/** 那个目录能用 ⇒ `null`；不能用 ⇒ 给人看的一句（不在 · 不是目录 · 里面没有会话记录 · 问不到 · 回的认不出）。 */
export async function claudeDirProblem(dir: string): Promise<string | null> {
  let state: string;
  try {
    ({ state } = await callTyped(LOCAL_ORIGIN, "agent-home-check", { path: dir }, budgetWithin(CHECK_BUDGET_MS)));
  } catch (e) {
    return copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: saidFrom(e, LOCAL_ORIGIN) });
  }
  switch (state) {
    case "ok":
      return null;
    case "missing":
      return copyText("settingsPanel.claudeDir.missing", { path: dir });
    case "not_dir":
      return copyText("settingsPanel.claudeDir.notDir", { path: dir });
    case "no_records":
      return copyText("settingsPanel.claudeDir.noRecords", { path: dir });
    default:
      // 回包形状由握手指纹核过；这一格仍不当能用（码不认识 ⇒ 说认不出）。
      console.warn(`claude-dir-check state unknown: ${state}`);
      return copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: peerVersionSaid("reply_unreadable", LOCAL_ORIGIN) });
  }
}
