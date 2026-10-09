/**
 * 要求：推送公钥是本机后端帧命令 `pubkey-push {machine}`，与 `remote-probe` 同形（「monitor 零 SSH」）。
 *
 * F50「一键推送公钥」**走通道，本机常驻后端办**：`chan.call(<local>, "pubkey-push", {machine, saved, jump, pubKeyPath})`。
 * 读本机那份 `.pub` · 校验 · 那台后端在就经它写 / 不在就一次 exec，全在后端（`src/backend/assets/pubkey.rs`）；界面只交设置页表单里那台
 * （可能还没保存）＋ 已保存的那一份 ＋ 跳板那一台（同 `remote-probe.ts`），按形状严格收。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import { hostKey, type RemoteHostConfig } from "./remote-config";
import { exactKeys, isObj } from "./ipc/decode";

/** 推送的结局。 */
export interface PushResult {
  /** `added` = 新加的 · `already` = 本就有整行相等的一行（没写）。 */
  outcome: "added" | "already";
  /** 实际推的是哪一份 `.pub`（给人看）。 */
  pubPath: string;
  /** 走了哪条：那台后端的文件管理面 · 那一次 exec。 */
  via: "backend" | "exec";
}

/** 期限（值归发起方，DL1）：一次拨号 ＋ 至多一次远端 CLI 往返，秒级；给足余量。 */
const PUSH_BUDGET_MS = 60_000;


function bad(): never {
  throw new ReplyUnreadable("pubkeyPush reply shape");
}

/** 应答体。严格收：恰好三格、值在闭集里。 */
export function decodePush(v: unknown): PushResult {
  if (!isObj(v) || !exactKeys(v, ["outcome", "pubPath", "via"])) bad();
  const o = v;
  if (o.outcome !== "added" && o.outcome !== "already") bad();
  if (o.via !== "backend" && o.via !== "exec") bad();
  if (typeof o.pubPath !== "string") bad();
  return { outcome: o.outcome, pubPath: o.pubPath, via: o.via };
}

/**
 * 把本机公钥推进那台的 `authorized_keys`。`saved` = 已保存的全部机器（从里面找「已保存的那一份」与跳板那一台）；
 * `pubKeyPath` = 用户挑的那份 `.pub`（`null` ⇒ 后端用私钥同名 `.pub`）。问不到 / 没加上 ⇒ 抛一句人话。
 */
export async function pushPublicKey(
  machine: RemoteHostConfig,
  saved: readonly RemoteHostConfig[],
  pubKeyPath: string | null,
): Promise<PushResult> {
  const mine = saved.find((h) => hostKey(h) === hostKey(machine)) ?? null;
  const jumpName = machine.jump.trim();
  const jump = jumpName ? (saved.find((h) => hostKey(h) === jumpName) ?? null) : null;
  const body = jsonBody({ machine, saved: mine, jump, pubKeyPath });
  const budget = budgetWithin(PUSH_BUDGET_MS);
  try {
    return decodePush(readJson(await chan.call(LOCAL_ORIGIN, "pubkey-push", body, budget)));
  } catch (e) {
    throw new Error(saidFrom(e, LOCAL_ORIGIN));
  }
}
