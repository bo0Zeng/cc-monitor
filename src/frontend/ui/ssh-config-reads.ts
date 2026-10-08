/**
 * 「从 `~/.ssh/config` 导入」那一问**走通道，本机常驻后端出成品**：批量导入预览（同一台机器的多个地址聚成一组），
 * 帧命令 `ssh-config-import`，成品 `{groups:[…]}`。
 *
 * 解读与拨号同一个家（后端 `dial/ssh_config.rs`）；monitor 零 SSH。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 * 按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」）；跨语言金样 `tests/__fixtures__/ssh-config.golden.json`。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidFrom, unreadableFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import { exactKeys, isObj } from "./ipc/decode";

/** 一个别名解析出的有效连接参数。 */
export interface ResolvedHost {
  host: string;
  port: number;
  user: string;
  /** 第一个**存在**的 IdentityFile（`~` 已展开）；都不存在 ⇒ `null`。 */
  keyPath: string | null;
  /** `ssh -G` 的 `proxyjump`（`none` ⇒ `null`）。 */
  proxyJump: string | null;
}

/** 预览组里的一个来源别名（「拆分」时据此还原成独立机器）。 */
export interface ImportMember {
  alias: string;
  host: string;
  port: number;
  proxyJump: string | null;
}

/** 批量导入预览的一组：聚合后的一台建议机器。 */
export interface ImportGroup {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string | null;
  /** 组内除 `host` 外的其余地址（端口与组首不同则 `host:port`）。 */
  addresses: string[];
  /** 组内首个非空 proxyjump（别名）。 */
  jump: string | null;
  members: ImportMember[];
  /** 已在机器列表里（后端按地址 ＋ 用户 ＋ 端口认，列表由这一问交过去）。 */
  inList: boolean;
}

const nullableStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const isPort = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 65535;
const strs = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === "string");

function bad(): never {
  throw unreadableFrom(LOCAL_ORIGIN, "sshConfigReads reply shape");
}

function decodeMember(v: unknown): ImportMember {
  if (
    !isObj(v) ||
    !exactKeys(v, ["alias", "host", "port", "proxyJump"]) ||
    typeof v.alias !== "string" ||
    typeof v.host !== "string" ||
    !isPort(v.port) ||
    !nullableStr(v.proxyJump)
  ) {
    bad();
  }
  return { alias: v.alias, host: v.host, port: v.port, proxyJump: v.proxyJump };
}

function decodeGroup(v: unknown): ImportGroup {
  if (
    !isObj(v) ||
    !exactKeys(v, ["label", "host", "port", "user", "keyPath", "addresses", "jump", "members", "inList"]) ||
    typeof v.inList !== "boolean" ||
    typeof v.label !== "string" ||
    typeof v.host !== "string" ||
    !isPort(v.port) ||
    typeof v.user !== "string" ||
    !nullableStr(v.keyPath) ||
    !strs(v.addresses) ||
    !nullableStr(v.jump) ||
    !Array.isArray(v.members)
  ) {
    bad();
  }
  return {
    label: v.label,
    host: v.host,
    port: v.port,
    user: v.user,
    keyPath: v.keyPath,
    addresses: v.addresses,
    jump: v.jump,
    members: v.members.map(decodeMember),
    inList: v.inList as boolean,
  };
}

/** `ssh-config-import` 的成品。严格收。 */
export function decodeImport(v: unknown): ImportGroup[] {
  if (!isObj(v) || !exactKeys(v, ["groups"]) || !Array.isArray(v.groups)) bad();
  return (v.groups as unknown[]).map(decodeGroup);
}

/** 期限：批量 = 逐个 `ssh -G`（不建连接），别名数通常个位；30 秒盖住回程与起进程，不含握手（`<local>` 长连接早就连着）。 */
const SSH_IMPORT_BUDGET_MS = 30_000;

/** 那一问的那一句：通道三层 → 一句人话（本机后端不在 / 太旧 / 拒了）。 */
function said(e: unknown): Error {
  return new Error(saidFrom(e, LOCAL_ORIGIN));
}

/** 批量导入预览。`known` ＝ 机器列表里已有的那几台（后端据它标「已在列表里」）。 */
export async function importSshHosts(known: { host: string; user: string; port: number }[]): Promise<ImportGroup[]> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({ known });
    const budget = budgetWithin(SSH_IMPORT_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "ssh-config-import", body, budget);
  } catch (e) {
    throw said(e);
  }
  return decodeImport(readJson(reply));
}
