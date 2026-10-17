/**
 * 「从 `~/.ssh/config` 导入」那一问**走通道，本机常驻后端出成品**：批量导入预览（同一台机器的多个地址聚成一组），
 * 帧命令 `ssh-config-import`，成品 `{groups:[…], partial}`（`partial`：有别名在期限内没读完，`groups` 只是读到的那一部分）。
 *
 * 解读与拨号同一个家（后端 `dial/ssh_config.rs`）；monitor 零 SSH。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 * 入参 · 回包都是后端类型生成的（`generated/`）；回包形状由握手时的形状指纹管，这里不逐格判。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import type { SshImportGroup } from "./generated/SshImportGroup";
import type { SshImported } from "./generated/SshImported";
import { saidOf } from "./kit/detail";

/** 批量导入预览的一组：聚合后的一台建议机器（含多地址与来源成员 · 是否已在机器列表里）。 */
export type ImportGroup = SshImportGroup;

/** 期限：批量 = 逐个 `ssh -G`（不建连接），别名数通常个位；30 秒盖住回程与起进程，不含握手（`<local>` 长连接早就连着）。 */
const SSH_IMPORT_BUDGET_MS = 30_000;

/** 那一问的那一句：通道三层 → 一句人话（本机后端不在 / 太旧 / 拒了）。 */
function said(e: unknown): Error {
  return new Error(saidOf(e));
}

/** 批量导入预览。`known` ＝ 机器列表里已有的那几台（后端据它标「已在列表里」）。 */
export async function importSshHosts(known: { host: string; user: string; port: number }[]): Promise<SshImported> {
  try {
    return await callTyped(LOCAL_ORIGIN, "ssh-config-import", { known }, budgetWithin(SSH_IMPORT_BUDGET_MS));
  } catch (e) {
    throw said(e);
  }
}
