/**
 * 端口转发（F58，`-L`）那三问**走通道，本机常驻后端记账**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 起一条 | `forward-start {origin, localPort, remoteHost, remotePort, machine?, jump?}` | `{id}` |
 * | 停一条 | `forward-stop {id}` | `{id}` |
 * | 列 | `forward-list` | `{forwards:[{id, origin, localPort, remoteHost, remotePort, state, connCount}]}` |
 *
 * 转发账与开链路同一个家（后端 `dial/forwards.rs`）；monitor 零转发账。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 * 入参 · 回包都是后端类型生成的（`generated/`）；回包形状由握手时的形状指纹管，这里不逐格判。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import type { RemoteHostConfig } from "./remote-config";
import type { ForwardRow } from "./generated/ForwardRow";
import { saidOf } from "./kit/detail";

/** 一条转发的状态（列表展示）。 */
export type ForwardStatus = ForwardRow;

/** 起一条转发的规格。 */
export interface ForwardSpec {
  origin: string;
  localPort: number;
  remoteHost: string;
  remotePort: number;
}

/**
 * 〔流没起的远端不许拒〕那台的配置（＋ 跳板那一台）：那台的流没握手过时，本机后端按它自己组请求去拨。
 * 流握手过的那台用后端手里那一份，这两格不看。
 */
export interface ForwardMachine {
  machine: RemoteHostConfig;
  jump: RemoteHostConfig | null;
}

/**
 * 期限：起 = 查表 ＋ 池里那条 SSH 上开一条链路（没连着就要握手、可能过跳板）⇒ 30 秒；停 / 列 = 本机后端一把锁 ⇒ 10 秒。
 */
const FORWARD_START_BUDGET_MS = 30_000;
const FORWARD_LEDGER_BUDGET_MS = 10_000;

function said(e: unknown): Error {
  return new Error(saidOf(e));
}

/** 起一条转发，回它的号。口绑不上 / 断开 ⇒ 抛那句原话。 */
export async function startForward(spec: ForwardSpec, via: ForwardMachine | null = null): Promise<string> {
  try {
    const req = via ? { ...spec, machine: via.machine, jump: via.jump } : { ...spec };
    const { id } = await callTyped(LOCAL_ORIGIN, "forward-start", req, budgetWithin(FORWARD_START_BUDGET_MS));
    return id;
  } catch (e) {
    throw said(e);
  }
}

/** 停一条转发。 */
export async function stopForward(id: string): Promise<void> {
  try {
    await callTyped(LOCAL_ORIGIN, "forward-stop", { id }, budgetWithin(FORWARD_LEDGER_BUDGET_MS));
  } catch (e) {
    throw said(e);
  }
}

/** 列当前所有转发。 */
export async function listForwards(): Promise<ForwardStatus[]> {
  try {
    const { forwards } = await callTyped(LOCAL_ORIGIN, "forward-list", {}, budgetWithin(FORWARD_LEDGER_BUDGET_MS));
    return forwards;
  } catch (e) {
    throw said(e);
  }
}
