/**
 * 要求：推送公钥是本机后端帧命令 `pubkey-push {machine}`，与 `remote-probe` 同形（「monitor 零 SSH」）。
 *
 * F50「一键推送公钥」**走通道，本机常驻后端办**：`chan.call(<local>, "pubkey-push", {machine, jump, pubKeyPath})`。
 * 读本机那份 `.pub` · 校验 · 那台后端在就经它写 / 不在就一次 exec，全在后端（`src/backend/assets/pubkey.rs`）；界面只交设置页表单里那台
 * （可能还没保存）＋ 已保存的那一份 ＋ 跳板那一台（同 `remote-probe.ts`）。入参 · 回包都是后端类型生成的；回包形状由握手时的形状指纹管。
 * 本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import { hostKey, type RemoteHostConfig } from "./remote-config";
import type { PubkeyPushed } from "./generated/PubkeyPushed";
import { saidOf } from "./kit/detail";

/** 推送的结局（`outcome` · `pubPath` · `via`）。 */
export type PushResult = PubkeyPushed;

/** 期限（值归发起方，DL1）：一次拨号 ＋ 至多一次远端 CLI 往返，秒级；给足余量。 */
const PUSH_BUDGET_MS = 60_000;


/**
 * 把本机公钥推进那台的 `authorized_keys`。`saved` = 已保存的全部机器（从里面找跳板那一台）；
 * `pubKeyPath` = 用户挑的那份 `.pub`（`null` ⇒ 后端用私钥同名 `.pub`）。问不到 / 没加上 ⇒ 抛一句人话。
 */
export async function pushPublicKey(
  machine: RemoteHostConfig,
  saved: readonly RemoteHostConfig[],
  pubKeyPath: string | null,
): Promise<PushResult> {
  const jumpName = machine.jump.trim();
  const jump = jumpName ? (saved.find((h) => hostKey(h) === jumpName) ?? null) : null;
  try {
    return await callTyped(LOCAL_ORIGIN, "pubkey-push", { machine, jump, pubKeyPath }, budgetWithin(PUSH_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
}
