/**
 * 〔MIG-1 续 · `设计/99 §2.1 ⑬` · 主会话裁「后端持有全部 SSH」〕「测试连接」**走通道，本机常驻后端拨一次、回结局**：
 * `chan.call(<local>, "remote-probe", {machine, saved, jump})` ⇒ 结局（原 monitor Tauri 命令 `test_remote_connection` 的回包形状 ＋ `stages`）。
 *
 * 交过去的是设置页表单里那台（**可能还没保存**）的配置 ＋ 已保存的那一份（表单空着指纹时，同一个 host 才继承）＋ 跳板那一台；
 * 拨号请求在后端组（`src/backend/dial/machine.rs`）。阶段行随结局一并回来（不再边拨边推）。
 * 按形状严格收；本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import { copyText } from "./copy-table";
import type { ConnectStage } from "./generated/ConnectStage";
import { hostKey, type RemoteHostConfig } from "./remote-config";

/** 测试连接的结局。 */
export interface ConnTestResult {
  /** SSH 连接 + 鉴权是否成功。 */
  sshOk: boolean;
  /** 握手时观察到的 server host key 指纹（`SHA256:...`）。失败时恒 `null`（免得把失配的 key 固化）。 */
  fingerprint: string | null;
  /** 竞速胜出的地址（`host:port`）。 */
  endpoint: string | null;
  /** 那台后端回了 hello 没有。 */
  backendOk: boolean;
  /** hello 的人读摘要（`v=.. build=.. home=.. … control=..`）。 */
  backendHello: string | null;
  /** 人读的总体状态 / 失败原因。 */
  message: string;
  /** 拨号阶段行（与 `ConnectStage` 同形），按发生顺序。 */
  stages: ConnectStage[];
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Obj, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const nullableStr = (v: unknown): v is string | null => v === null || typeof v === "string";

function bad(): never {
  throw new Error(copyText("remoteProbe.reply.badShape"));
}

/** `remote-probe` 的成品。严格收（阶段行只查是对象、带 `kind`；各形的字段由画它的那一处读）。 */
export function decodeProbe(v: unknown): ConnTestResult {
  if (
    !isObj(v) ||
    !sameKeys(v, ["sshOk", "fingerprint", "endpoint", "backendOk", "backendHello", "message", "stages"]) ||
    typeof v.sshOk !== "boolean" ||
    !nullableStr(v.fingerprint) ||
    !nullableStr(v.endpoint) ||
    typeof v.backendOk !== "boolean" ||
    !nullableStr(v.backendHello) ||
    typeof v.message !== "string" ||
    !Array.isArray(v.stages) ||
    !v.stages.every((s) => isObj(s) && typeof s.kind === "string")
  ) {
    bad();
  }
  return {
    sshOk: v.sshOk,
    fingerprint: v.fingerprint,
    endpoint: v.endpoint,
    backendOk: v.backendOk,
    backendHello: v.backendHello,
    message: v.message,
    stages: v.stages as ConnectStage[],
  };
}

/**
 * 期限：握手（可能过跳板）＋ 那台后端的首行 ＋ 一次往返。后端零定时器 —— 那台一声不吭时由探活连接的空闲上限（30 s）拆掉，
 * 最坏两段各吃满一次 ⇒ 给 75 s，盖住那两段 ＋ 回程。
 */
const PROBE_BUDGET_MS = 75_000;

/** 本机后端比这一问老（不认这条命令）时的那句话。 */
const OLD_BACKEND = copyText("remoteProbe.backend.tooOld");

/**
 * 测一台机器。`saved` = 已保存的全部机器（从里面找「已保存的那一份」与跳板那一台）。问不到 ⇒ 抛一句人话。
 */
export async function probeMachine(machine: RemoteHostConfig, saved: readonly RemoteHostConfig[]): Promise<ConnTestResult> {
  const mine = saved.find((h) => hostKey(h) === hostKey(machine)) ?? null;
  const jumpName = machine.jump.trim();
  const jump = jumpName ? (saved.find((h) => hostKey(h) === jumpName) ?? null) : null;
  let reply: Uint8Array;
  try {
    const body = jsonBody({ machine, saved: mine, jump });
    const budget = budgetWithin(PROBE_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "remote-probe", body, budget);
  } catch (e) {
    throw new Error(saidOf(e, OLD_BACKEND));
  }
  return decodeProbe(readJson(reply));
}
