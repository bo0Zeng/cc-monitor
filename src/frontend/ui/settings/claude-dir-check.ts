/**
 * 设置里填的「Claude 目录」存之前先问本机后端那个目录在不在、像不像 Claude 的目录（里面有没有 `projects/`，`files-stat` 两问）：
 * 不在的目录照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它；没有 `projects/` 的目录读不出一条会话。
 */
import { chan, ChanError } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidOf } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";

/** 问一个路径的期限（本机，毫秒级；给 10 秒）。 */
const STAT_BUDGET_MS = 10_000;

/** 问那一条路径是什么（`dir` · `file` …）；不在 / 读不动 ⇒ `missing`；问不到 ⇒ 抛那一句。 */
async function kindOf(path: string, dir: string): Promise<unknown> {
  try {
    const body = jsonBody({ path });
    const budget = budgetWithin(STAT_BUDGET_MS);
    const v = readJson(await chan.call(LOCAL_ORIGIN, "files-stat", body, budget));
    return (v as { kind?: unknown } | null)?.kind;
  } catch (e) {
    const refused = e instanceof ChanError && e.error.layer === "peer" && e.error.why === "refused" ? refusalOf(e.error.body)?.code : undefined;
    if (refused === "unreadable" || refused === "bad_path") return "missing";
    throw new Error(copyText("settingsPanel.claudeDir.uncheckable", { path: dir, said: saidOf(e, copyText("chanCaller.said.error")) }));
  }
}

/** 那个目录能用 ⇒ `null`；不能用 ⇒ 给人看的一句（不在 · 不是目录 · 里面没有会话记录 · 问不到）。 */
export async function claudeDirProblem(dir: string): Promise<string | null> {
  try {
    const kind = await kindOf(dir, dir);
    if (kind === "missing") return copyText("settingsPanel.claudeDir.missing", { path: dir });
    if (kind !== "dir") return copyText("settingsPanel.claudeDir.notDir", { path: dir });
    const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
    const projects = `${dir.replace(/[\\/]+$/, "")}${sep}projects`;
    return (await kindOf(projects, dir)) === "dir" ? null : copyText("settingsPanel.claudeDir.noRecords", { path: dir });
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}
