// 机器表数据层（纯数据，无 DOM）：表住本机后端（`~/.cc-monitor/machines.json`，后端读法 `dial/machines.rs` · 写口 `control/machines.rs`），
// 这里只经 `machines-read` / `machines-try` / `machines-set` 读写 ＋ 反查 / 筛选纯函数。
// 一台在界面里是后端读回的那一行（`MachineRow`：后端算好的名字 ＋ 那一台原样）；名字只认后端那一格，界面不再自己算。
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL } from "./ipc/origin";
import type { Host } from "./generated/Host";
import type { MachineEdit } from "./generated/MachineEdit";
import type { MachineFault } from "./generated/MachineFault";
import type { MachineRow } from "./generated/MachineRow";
import type { MachinesReply } from "./generated/MachinesReply";

/** 一问的期限：读写一份小文件 ＋ 回程。 */
const MACHINES_BUDGET_MS = 10_000;

/** 多行文本 ↔ 地址数组（trim ＋ 去空行）。界面用 textarea，线上用数组。 */
export function parseAddressLines(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

/**
 * 端口框里的字 → `Host.port`（添加框 · 机器卡同读这一处）：空 ⇒ 不给（缺省只住拨号核心）；整数 ⇒ 那个数；
 * 别的 ⇒ 0（交后端判「端口不在 1–65535」，不在这里另判一遍）。
 */
export function portOf(text: string): number | undefined {
  const t = text.trim();
  if (t === "") return undefined;
  return /^\d+$/.test(t) ? Number(t) : 0;
}

/** 一台带上端口框那一格：空 ⇒ 不带 `port` 键。 */
export function withPort(h: Host, port: number | undefined): Host {
  const { port: _old, ...rest } = h;
  return port === undefined ? rest : { ...rest, port };
}

/** 机器表：后端读回的那几行（照表里的先后）。 */
export interface RemoteConfig {
  hosts: MachineRow[];
  /**
   * 机器表那份文件在、却读不懂时那一句（本机后端写好的）；读得懂 / 没有那份 ⇒ 不给。读不懂时 `hosts` 恒空（不猜那是哪台），
   * 设置页把这一句说在机器列表顶上，壳那一侧一台都不连。
   */
  unrecognized?: string;
  /** 每台记下的主机指纹（名字 → 指纹；库里没有的那台不出现）。只读，拨号核心那份库现读。 */
  hostKeys?: Record<string, string[]>;
}

/**
 * 能打开文件窗口的远端主机：`host` 与 `user` 都非空。顶栏文件入口据此决定 0 台提示 / 1 台直开 / 多台选单。
 * 不看「连不连」：远端数据源没启用也能浏览某台的文件。
 */
export function sftpEligibleHosts(cfg: RemoteConfig): MachineRow[] {
  return cfg.hosts.filter((r) => r.host.host.trim() !== "" && r.host.user.trim() !== "");
}

/** 读本机后端的机器表 → RemoteConfig。读不懂 ⇒ 空表 ＋ 那一句；问不到 ⇒ 空表（记一句日志）。永不抛。 */
export async function readRemoteConfig(): Promise<RemoteConfig> {
  try {
    return remoteConfigOf(await callTyped(LOCAL, "machines-read", {}, budgetWithin(MACHINES_BUDGET_MS)));
  } catch (e) {
    console.warn("readRemoteConfig failed:", e);
    return { hosts: [] };
  }
}

/** 那一答 → `RemoteConfig`（纯函数）。 */
export function remoteConfigOf(r: MachinesReply): RemoteConfig {
  if (r.state === "unreadable") return { hosts: [], unrecognized: r.reason ?? "" };
  return { hosts: r.hosts, hostKeys: r.hostKeys };
}

/** 在那几行里按名字（会话来源 · 寻址用的那个）反查（纯函数）。找不到 → null。 */
export function findHostByOrigin(rows: MachineRow[], origin: string): MachineRow | null {
  return rows.find((r) => r.name === origin) ?? null;
}

/** 按名字反查那一行。找不到（主机被删 / 改名）→ null。 */
export async function resolveRemoteConfigByOrigin(origin: string): Promise<MachineRow | null> {
  return findHostByOrigin((await readRemoteConfig()).hosts, origin);
}

/** 一次局部修改：没被提到的机器不碰（补丁里没有它）。 */
export interface RemoteHostsPatch {
  /**
   * 要写入的机器。`key` = 这条记录**在盘上当前的名字**（后端读回的那一格）；`null` = 新增（追加到末尾）。
   * key 在盘上找不到（被别处删了 / 改了）⇒ 整批拒、说出来（不当新增）。
   */
  upsert?: { key: string | null; value: Host; was?: Host }[];
  /** 要删除的机器，按名字。 */
  remove?: string[];
}

/**
 * 界面写机器表的唯一入口。补丁全按键认那一台（[`remoteHostsEdits`]），不读、不整张写 ——
 * 认那一台在本机后端写口锁内现读现判（`machines-set`）。回写完读回的那一张（名字是后端算好的）；一条都没动 ⇒ `null`。拒 / 问不到 ⇒ 抛。
 */
export async function patchRemoteConfig(patch: RemoteHostsPatch): Promise<RemoteConfig | null> {
  const edits = remoteHostsEdits(patch);
  if (edits.length === 0) return null;
  return remoteConfigOf((await callTyped(LOCAL, "machines-set", { edits }, budgetWithin(MACHINES_BUDGET_MS))).machines);
}

/** 忘掉这台记下的主机指纹（拨号核心那份库里删这台的行；下回连上照首见记）。拒 / 问不到 ⇒ 抛。 */
export async function forgetHostKeys(origin: string): Promise<void> {
  await callTyped(LOCAL, "host-keys-forget", { machine: origin }, budgetWithin(MACHINES_BUDGET_MS));
}

/**
 * 这一次局部修改若现在落盘，机器表那一道过不过（试算，盘上不动；与写口同一份规则，`machines-try`）。过 ⇒ `null`。
 */
export async function tryRemoteConfig(patch: RemoteHostsPatch): Promise<MachineFault | null> {
  const edits = remoteHostsEdits(patch);
  if (edits.length === 0) return null;
  return (await callTyped(LOCAL, "machines-try", { edits }, budgetWithin(MACHINES_BUDGET_MS))).fault;
}

/**
 * 一次局部修改 ⇒ 交给唯一写口的补丁：**全部按键认元素，没有整段写**。**纯函数。**
 *
 * - 删（`remove`）⇒ 每台一条 `removein`：盘上认不出 / 认出多台 ⇒ 整批拒；
 * - 改（`key` ＋ 加载时那份 `was`）⇒ 动过的格各一条 `setin`（[`hostCellEdits`]）；
 * - 增（`key = null`）⇒ 一条 `insertin`：写口照它的名字（`Host::name`）判盘上有没有同名的，有 ⇒ 整批拒；
 * - `key` 在、`was` 缺（加载时那个名字不止一台）⇒ 按那个名字删了再插 —— 认出多台 ⇒ `removein` 整批拒，不猜是哪台。
 *
 * 先删后改再增（写口锁内按序应用）⇒「删掉 A、同时新增一台也叫 A」是替换；别的窗口改的格不会被盖掉。
 */
export function remoteHostsEdits(patch: RemoteHostsPatch): MachineEdit[] {
  const edits: MachineEdit[] = [];
  for (const k of patch.remove ?? []) edits.push({ op: "removein", where: hostWhere(k) });
  const inserts: MachineEdit[] = [];
  for (const u of patch.upsert ?? []) {
    if (u.key !== null && u.was !== undefined) {
      edits.push(...hostCellEdits(u.key, u.was, u.value));
      continue;
    }
    if (u.key !== null) edits.push({ op: "removein", where: hostWhere(u.key) });
    // 新加那一台：认重名的两格原样交给写口（`label` · `host`），名字由写口照 `Host::name` 算、再判一遍。
    inserts.push({ op: "insertin", where: insertWhere(u.value), value: u.value });
  }
  return [...edits, ...inserts];
}

/** 表里一台机器的认法：后端读回的那个名字（写口 `ElemKey` 的口径：`label` 非空取它、否则 `host`，同 `Host::name`）。 */
function hostWhere(key: string): { fields: string[]; equals: string }[] {
  return [{ fields: ["label", "host"], equals: key }];
}

/** 新加那一台交给写口认重名的那一格：表单里填的 `label`，没填交 `host`（写口照 `Host::name` 认，同一条规则只在后端）。 */
function insertWhere(h: Host): { fields: string[]; equals: string }[] {
  return h.label !== "" ? [{ fields: ["label"], equals: h.label }] : [{ fields: ["host"], equals: h.host }];
}

/** 一台的各格（照 `Host` 的形状；`label` · `host` 认它的那两格另排）。 */
const HOST_CELLS = ["port", "user", "keyPath", "addresses", "jump", "connect"] as const satisfies readonly (keyof Host)[];

/** 上面那张漏掉的格（应为 `never`；加了格而这里没跟上 ⇒ `tsc` 当场红、点名是哪一格）。 */
type MissingCell = Exclude<keyof Host, (typeof HOST_CELLS)[number] | "label" | "host">;
const _noMissingCell: MissingCell extends never ? true : MissingCell = true;
void _noMissingCell;

/**
 * 一台已在盘上的机器（加载时名字 = `key`、加载时的样子 `was`）改成 `now`：只出动过的那几格，每格一条 `setin`。**纯函数。**
 *
 * 认它的键就是名字，改 `label` / `host` 会换掉它 ⇒ 顺序：两者动了任一 ⇒ 先按旧名字把 `label` 写成表单值；
 * 其余格随后按新名字认（`label` 空 ⇒ 仍是旧 `host`），`host` 排最后。
 */
export function hostCellEdits(key: string, was: Host, now: Host): MachineEdit[] {
  const moved = (f: keyof Host) => JSON.stringify(was[f]) !== JSON.stringify(now[f]);
  const cell = (k: string, field: keyof Host): MachineEdit => ({
    op: "setin",
    where: hostWhere(k),
    field,
    // 这一格这回没有（端口框清空）⇒ `null`：写口拿掉那一格、回缺省。
    value: now[field] ?? null,
  });
  const out: MachineEdit[] = [];
  let k = key;
  if (moved("label") || moved("host")) {
    out.push(cell(key, "label"));
    k = now.label !== "" ? now.label : was.host;
  }
  for (const f of HOST_CELLS) if (moved(f)) out.push(cell(k, f));
  if (moved("host")) out.push(cell(k, "host"));
  return out;
}
