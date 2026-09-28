/**
 * 〔MIG-1 · `设计/99 §2.1 ⑯`〕「从 `~/.ssh/config` 导入」那三问**走通道，本机常驻后端出成品**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 可点的别名清单 | `ssh-config-aliases` | `{aliases}` |
 * | 一个别名的有效连接参数（`ssh -G`） | `ssh-config-resolve {alias}` | `{host, port, user, keyPath, proxyJump}` |
 * | 批量导入预览（同一台机器的多个地址聚成一组） | `ssh-config-import` | `{groups:[…]}` |
 *
 * 解读与拨号同一个家（后端 `dial/ssh_config.rs`）；monitor 零 SSH。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 * 按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」）；跨语言金样 `tests/__fixtures__/ssh-config.golden.json`。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import { copyText } from "./copy-table";

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
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Obj, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const nullableStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const isPort = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 65535;
const strs = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === "string");

function bad(): never {
  throw new Error(copyText("sshConfigReads.reply.badShape"));
}

/** `ssh-config-aliases` 的成品。严格收。 */
export function decodeAliases(v: unknown): string[] {
  if (!isObj(v) || !sameKeys(v, ["aliases"]) || !strs(v.aliases)) bad();
  return v.aliases as string[];
}

/** `ssh-config-resolve` 的成品。严格收。 */
export function decodeResolved(v: unknown): ResolvedHost {
  if (
    !isObj(v) ||
    !sameKeys(v, ["host", "port", "user", "keyPath", "proxyJump"]) ||
    typeof v.host !== "string" ||
    !isPort(v.port) ||
    typeof v.user !== "string" ||
    !nullableStr(v.keyPath) ||
    !nullableStr(v.proxyJump)
  ) {
    bad();
  }
  return { host: v.host, port: v.port, user: v.user, keyPath: v.keyPath, proxyJump: v.proxyJump };
}

function decodeMember(v: unknown): ImportMember {
  if (
    !isObj(v) ||
    !sameKeys(v, ["alias", "host", "port", "proxyJump"]) ||
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
    !sameKeys(v, ["label", "host", "port", "user", "keyPath", "addresses", "jump", "members"]) ||
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
  };
}

/** `ssh-config-import` 的成品。严格收。 */
export function decodeImport(v: unknown): ImportGroup[] {
  if (!isObj(v) || !sameKeys(v, ["groups"]) || !Array.isArray(v.groups)) bad();
  return (v.groups as unknown[]).map(decodeGroup);
}

/**
 * 期限：列别名 / 解析一个 = 读一份文件或起一次 `ssh -G`（不建连接）；批量 = 逐个 `ssh -G`，别名数通常个位。
 * 10 秒 / 30 秒盖住回程与起进程，不含握手（`<local>` 长连接早就连着）。
 */
const SSH_CONFIG_BUDGET_MS = 10_000;
const SSH_IMPORT_BUDGET_MS = 30_000;

/** 本机后端比这三问老（不认这条命令）时的那句话。 */
const OLD_BACKEND = copyText("sshConfigReads.backend.tooOld");

/** 三问共用的那一句：通道三层 → 一句人话（本机后端不在 / 太旧 / 拒了）。 */
function said(e: unknown): Error {
  return new Error(saidOf(e, OLD_BACKEND));
}

/** `~/.ssh/config` 里可点的别名。问不到 ⇒ 抛一句人话（与「真没有别名」分开）。 */
export async function listSshHostAliases(): Promise<string[]> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({});
    const budget = budgetWithin(SSH_CONFIG_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "ssh-config-aliases", body, budget);
  } catch (e) {
    throw said(e);
  }
  return decodeAliases(readJson(reply));
}

/** 一个别名的有效连接参数。 */
export async function resolveSshHost(alias: string): Promise<ResolvedHost> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({ alias });
    const budget = budgetWithin(SSH_CONFIG_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "ssh-config-resolve", body, budget);
  } catch (e) {
    throw said(e);
  }
  return decodeResolved(readJson(reply));
}

/** 批量导入预览。 */
export async function importSshHosts(): Promise<ImportGroup[]> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({});
    const budget = budgetWithin(SSH_IMPORT_BUDGET_MS);
    reply = await chan.call(LOCAL_ORIGIN, "ssh-config-import", body, budget);
  } catch (e) {
    throw said(e);
  }
  return decodeImport(readJson(reply));
}
