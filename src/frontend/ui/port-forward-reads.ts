/**
 * 端口转发（F58，`-L`）那三问**走通道，本机常驻后端记账**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 起一条 | `forward-start {origin, localPort, remoteHost, remotePort}` | `{id}` |
 * | 停一条 | `forward-stop {id}` | `{id}` |
 * | 列 | `forward-list` | `{forwards:[{id, origin, localPort, remoteHost, remotePort, state, connCount}]}` |
 *
 * 转发账与开链路同一个家（后端 `dial/forwards.rs`）；monitor 零转发账。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 * 按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidFrom, unreadableFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import type { RemoteHostConfig } from "./remote-config";

/** 一条转发的状态（列表展示）。 */
export interface ForwardStatus {
  id: string;
  origin: string;
  localPort: number;
  remoteHost: string;
  remotePort: number;
  /** `running`（链路还在）/ `error`（链路自己收工了）。 */
  state: "running" | "error";
  /** 累计接进的连接数。 */
  connCount: number;
}

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

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Obj, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const isPort = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 65535;
const isCount = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0;

function bad(): never {
  throw unreadableFrom(LOCAL_ORIGIN, "portForwardReads reply shape");
}

function decodeRow(v: unknown): ForwardStatus {
  if (
    !isObj(v) ||
    !sameKeys(v, ["id", "origin", "localPort", "remoteHost", "remotePort", "state", "connCount"]) ||
    typeof v.id !== "string" ||
    typeof v.origin !== "string" ||
    !isPort(v.localPort) ||
    typeof v.remoteHost !== "string" ||
    !isPort(v.remotePort) ||
    (v.state !== "running" && v.state !== "error") ||
    !isCount(v.connCount)
  ) {
    bad();
  }
  return {
    id: v.id,
    origin: v.origin,
    localPort: v.localPort,
    remoteHost: v.remoteHost,
    remotePort: v.remotePort,
    state: v.state,
    connCount: v.connCount,
  };
}

/** `forward-list` 的成品。严格收。 */
export function decodeForwards(v: unknown): ForwardStatus[] {
  if (!isObj(v) || !sameKeys(v, ["forwards"]) || !Array.isArray(v.forwards)) bad();
  return (v.forwards as unknown[]).map(decodeRow);
}

/** `forward-start` / `forward-stop` 的成品：`{id}`。严格收。 */
export function decodeId(v: unknown): string {
  if (!isObj(v) || !sameKeys(v, ["id"]) || typeof v.id !== "string") bad();
  return v.id as string;
}

/**
 * 期限：起 = 查表 ＋ 池里那条 SSH 上开一条链路（没连着就要握手、可能过跳板）⇒ 30 秒；停 / 列 = 本机后端一把锁 ⇒ 10 秒。
 */
const FORWARD_START_BUDGET_MS = 30_000;
const FORWARD_LEDGER_BUDGET_MS = 10_000;


function said(e: unknown): Error {
  return new Error(saidFrom(e, LOCAL_ORIGIN));
}

/** 起一条转发，回它的号。口绑不上 / 连不上 ⇒ 抛那句原话。 */
export async function startForward(spec: ForwardSpec, via: ForwardMachine | null = null): Promise<string> {
  let reply: Uint8Array;
  try {
    const body = jsonBody(via ? { ...spec, machine: via.machine, jump: via.jump } : { ...spec });
    const budget = budgetWithin(FORWARD_START_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "forward-start", body, budget);
  } catch (e) {
    throw said(e);
  }
  return decodeId(readJson(reply));
}

/** 停一条转发。 */
export async function stopForward(id: string): Promise<void> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({ id });
    const budget = budgetWithin(FORWARD_LEDGER_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "forward-stop", body, budget);
  } catch (e) {
    throw said(e);
  }
  decodeId(readJson(reply));
}

/** 列当前所有转发。 */
export async function listForwards(): Promise<ForwardStatus[]> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({});
    const budget = budgetWithin(FORWARD_LEDGER_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "forward-list", body, budget);
  } catch (e) {
    throw said(e);
  }
  return decodeForwards(readJson(reply));
}
