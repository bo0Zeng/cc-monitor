/**
 * 设置里填的「Claude 目录」存之前问本机后端一次（`agent-home-check`）：后端判在不在 · 是不是目录 · 有没有记录树，回码；这里按码取那一句。
 * 不在的目录照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它；没有 `projects/` 的目录读不出一条会话。
 */
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, peerVersionSaid, readJson, saidFrom } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";

/** 问一次的期限（本机，毫秒级；给 10 秒）。 */
const CHECK_BUDGET_MS = 10_000;

/** 那个目录能用 ⇒ `null`；不能用 ⇒ 给人看的一句（不在 · 不是目录 · 里面没有会话记录 · 问不到 · 回的认不出）。 */
export async function claudeDirProblem(dir: string): Promise<string | null> {
  let v: unknown;
  try {
    const body = jsonBody({ path: dir });
    const budget = budgetWithin(CHECK_BUDGET_MS);
    v = readJson(await chan.call(LOCAL_ORIGIN, "agent-home-check", body, budget));
  } catch (e) {
    return copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: saidFrom(e, LOCAL_ORIGIN) });
  }
  const state = v !== null && typeof v === "object" && !Array.isArray(v) && Object.keys(v).length === 1 ? (v as { state?: unknown }).state : undefined;
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
      console.warn(`claude-dir-check reply unreadable: ${JSON.stringify(v)}`);
      return copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: peerVersionSaid("reply_unreadable", LOCAL_ORIGIN) });
  }
}
