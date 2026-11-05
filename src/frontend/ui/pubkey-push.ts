/**
 * 要求：推送公钥是本机后端帧命令 `pubkey-push {machine}`，与 `remote-probe` 同形（「monitor 零 SSH」）。
 *
 * F50「一键推送公钥」**走通道，本机常驻后端办**：`chan.call(<local>, "pubkey-push", {machine, pubKeyPath})`。
 * 读本机那份 `.pub` · 校验 · 那台后端在就经它写 / 不在就一次 exec，全在后端（`src/backend/assets/pubkey.rs`）；界面只交设置页表单里那台
 * （可能还没保存；跳板照本机后端的机器表按名字找，同 `remote-probe.ts`）。入参 · 回包都是后端类型生成的；回包形状由握手时的形状指纹管。
 * 本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Host } from "./generated/Host";
import { LOCAL } from "./ipc/origin";
import type { PubkeyPushed } from "./generated/PubkeyPushed";
import { saidOf } from "./kit/detail";

/** 推送的结局（`said` 后端写好的那一句，界面照摆 · `via`）。 */
export type PushResult = PubkeyPushed;

/** 期限（值归发起方，DL1）：一次拨号 ＋ 至多一次远端 CLI 往返，秒级；给足余量。 */
const PUSH_BUDGET_MS = 60_000;


/**
 * 把本机公钥推进那台的 `authorized_keys`（跳板照本机后端的机器表找）；
 * `pubKeyPath` = 用户挑的那份 `.pub`（`null` ⇒ 后端用私钥同名 `.pub`）。问不到 / 没加上 ⇒ 抛一句人话。
 */
export async function pushPublicKey(
  machine: Host,
  pubKeyPath: string | null,
): Promise<PushResult> {
  try {
    return await callTyped(LOCAL, "pubkey-push", { machine, pubKeyPath }, budgetWithin(PUSH_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
}
