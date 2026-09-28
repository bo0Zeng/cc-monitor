/**
 * 〔OSA · `设计/99 §1` V156 · 主会话 09-28 裁子步 4「拆」〕`$PROFILE` 备份在哪 —— **经通道直接问本机后端**。
 *
 * 从前是 monitor `data_paths.rs` 自己探 `$PROFILE` 的两个目录名（`$PROFILE` 在哪的第二个读者）。今天：
 * ① 候选启动文件由本机后端的方言答（`aliases-read`，PowerShell 那一臂 —— 全仓唯一答「`$PROFILE` 在哪」的地方）；
 * ② 它们所在的目录里有没有 `<名>.ccm-backup-…`（后端 `files-put` 备份的落点）问同一台的 `files-ls`。
 * `get_data_paths` 仍是 monitor 自己的（`99 §2.1 ⑬` Own::Log，不碰后端），只是不再带这一格。
 *
 * 只在本机说 PowerShell 时问（`machine-aliases.ts::localShell`：本机后端就跑在这台上）；别的平台没有 `$PROFILE` ⇒ 空。
 */
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody, readJson } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../backend-policy";
import { unreadable } from "../control-said";
import { readAliases } from "../alias-reads";
import { localShell } from "./machine-aliases";

/** 最多列几个（与原 `data_paths.rs` 同：前 3 个，去重）。 */
const MAX_DIRS = 3;
/** 列一个目录的期限（本机，毫秒级；给 10 秒）。 */
const LS_BUDGET_MS = 10_000;
/** 后端 `files-put` 备份的名字里那一截（`control/files_write.rs::land_backup`）。 */
const BACKUP_MARK = ".ccm-backup-";

/** 一个路径的父目录（两种分隔符都认；没有父目录 ⇒ `null`）。 */
function parentOf(p: string): string | null {
  const at = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return at > 0 ? p.slice(0, at) : null;
}

/** `files-ls` 一条的 `path`（UTF-8 串，或 `{b16}` 字节）⇒ 能不能认出备份那一截。 */
function isBackupEntry(path: unknown): boolean {
  if (typeof path === "string") return path.includes(BACKUP_MARK);
  if (
    path !== null &&
    typeof path === "object" &&
    typeof (path as { b16?: unknown }).b16 === "string"
  ) {
    const hex = (path as { b16: string }).b16;
    let s = "";
    for (let i = 0; i + 1 < hex.length; i += 2)
      s += String.fromCharCode(parseInt(hex.slice(i, i + 2), 16));
    return s.includes(BACKUP_MARK);
  }
  return false;
}

/** 这个目录里有没有备份。列不出来（目录不在 / 读不了）⇒ 没有（与原 monitor 那一份同）；应答形状不对 ⇒ 抛。 */
async function hasBackupIn(dir: string): Promise<boolean> {
  let v: unknown;
  try {
    const body = jsonBody({ path: dir });
    const budget = budgetWithin(LS_BUDGET_MS);
    v = readJson(await chan.call(LOCAL_ORIGIN, "files-ls", body, budget));
  } catch {
    return false;
  }
  const entries = (v as { entries?: unknown } | null)?.entries;
  if (!Array.isArray(entries))
    throw unreadable(LOCAL_ORIGIN, "files-ls", "entries");
  return entries.some((e) =>
    isBackupEntry((e as { path?: unknown } | null)?.path),
  );
}

/** 本机 `$PROFILE` 候选所在、里面有备份的那几个目录（按候选顺序、大小写不敏感去重，最多 3 个）。问不到 ⇒ 抛（调用方说读不到）。 */
export async function findProfileBackupDirs(): Promise<string[]> {
  if (localShell() !== "powershell") return [];
  const listing = await readAliases(LOCAL_ORIGIN, "powershell", null);
  const seen = new Set<string>();
  const out: string[] = [];
  for (const c of listing.rcCandidates) {
    const dir = parentOf(c.path);
    if (dir === null || seen.has(dir.toLowerCase())) continue;
    seen.add(dir.toLowerCase());
    if (await hasBackupIn(dir)) out.push(dir);
    if (out.length >= MAX_DIRS) break;
  }
  return out;
}
