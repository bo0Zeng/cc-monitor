// 机器表数据层（纯数据，无 DOM）：表住本机后端（`~/.cc-monitor/machines.json`，后端 `control/machines.rs`），
// 这里只经 `machines-read` / `machines-try` / `machines-set` 读写 ＋ 反查 / 筛选纯函数。
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import type { MachineEdit } from "./generated/MachineEdit";
import type { MachineFault } from "./generated/MachineFault";
import type { MachinesReply } from "./generated/MachinesReply";
import type { PinnedDay } from "./generated/PinnedDay";

/** 一问的期限：读写一份小文件 ＋ 回程。 */
const MACHINES_BUDGET_MS = 10_000;

/**
 * 单台远端机器配置（config.json `remote.hosts[]` 的元素）。**key 必须与 Rust reader 一致**。
 * `label` 是该台的稳定身份（origin tag，多机 #30）：Tab 前缀 / 历史分组 / 选台 key。
 * 留空时后端回退用 host。port 缺省 22；keyPath / hostKeyFingerprint 可选。
 */
export interface RemoteHostConfig {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string;
  hostKeyFingerprint: string;
  /** 指纹记下的那一天（`YYYY-MM-DD`；没记过 / 不知道 ⇒ 空）。只给人看，拨号不读它。 */
  hostKeyPinnedAt?: string;
  /**
   * 备用地址（happy-eyeballs 竞发）。每项 `host` / `host:port` /
   * `[IPv6]:port` / 裸 IPv6。首选地址仍是 `host` 字段。空数组 = 仅用 host。
   */
  addresses: string[];
  /**
   * 跳板 ProxyJump：填另一台已配置主机的 `label`（空 = 直连），经那台隧道连这台
   * （数据源侧 russh direct-tcpip；拉起侧 ssh `-J`）。fail-closed：跳板缺失/断开即报错不直连。
   */
  jump: string;
  /** 连接这台：关 ⇒ 不连（盘上 `"connect": false`），开 ⇒ 去连；改了当场生效（本机后端推 `changed {machines}`，壳照它对齐）。 */
  connect: boolean;
}

/** 多行文本 ↔ 地址数组（trim ＋ 去空行）。界面用 textarea，config / IPC 用数组。 */
export function parseAddressLines(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

/** 机器表：每台自己的「连接这台」。 */
export interface RemoteConfig {
  hosts: RemoteHostConfig[];
  /**
   * 机器表那份文件在、却读不懂时那一句（本机后端写好的）；读得懂 / 没有那份 ⇒ 不给。读不懂时 `hosts` 恒空（不猜那是哪台），
   * 设置页把这一句说在机器列表顶上，壳那一侧一台都不连。
   */
  unrecognized?: string;
}

/**
 * 能打开文件窗口的远端主机：`host` 与 `user` 都非空（`file-window.ts::openFileWindow` 的前置）。顶栏文件入口据此决定 0 台提示 / 1 台直开 / 多台选单。
 * 不看 `enabled`：远端数据源没启用也能浏览某台的文件。
 */
export function sftpEligibleHosts(cfg: RemoteConfig): RemoteHostConfig[] {
  return cfg.hosts.filter((h) => h.host.trim() !== "" && h.user.trim() !== "");
}

export const HOST_DEFAULTS: RemoteHostConfig = {
  label: "",
  host: "",
  port: 22,
  user: "",
  keyPath: "",
  hostKeyFingerprint: "",
  hostKeyPinnedAt: "",
  addresses: [],
  jump: "",
  connect: true,
};

/** 容忍 config 里 addresses 为数组 / 换行文本 / 缺失。 */
function coerceAddresses(v: unknown): string[] {
  if (Array.isArray(v)) {
    return v
      .filter((x): x is string => typeof x === "string")
      .map((x) => x.trim())
      .filter(Boolean);
  }
  if (typeof v === "string") return parseAddressLines(v);
  return [];
}

function coerceHost(obj: Record<string, unknown>): RemoteHostConfig {
  const str = (k: string, d: string) => (typeof obj[k] === "string" ? (obj[k] as string) : d);
  const host = str("host", HOST_DEFAULTS.host);
  return {
    // label 缺省回退 host（与 Rust origin_label 一致：空 label → host）
    label: str("label", "") || host,
    host,
    port:
      typeof obj.port === "number" && Number.isFinite(obj.port)
        ? (obj.port as number)
        : HOST_DEFAULTS.port,
    user: str("user", HOST_DEFAULTS.user),
    keyPath: str("keyPath", HOST_DEFAULTS.keyPath),
    hostKeyFingerprint: str("hostKeyFingerprint", HOST_DEFAULTS.hostKeyFingerprint),
    hostKeyPinnedAt: str("hostKeyPinnedAt", ""),
    addresses: coerceAddresses(obj.addresses),
    jump: str("jump", HOST_DEFAULTS.jump),
    connect: obj.connect !== false,
  };
}

/** 读本机后端的机器表 → RemoteConfig。读不懂 ⇒ 空表 ＋ 那一句；问不到 ⇒ 空表（记一句日志）。永不抛。 */
export async function readRemoteConfig(): Promise<RemoteConfig> {
  try {
    return remoteConfigOf(await callTyped(LOCAL_ORIGIN, "machines-read", {}, budgetWithin(MACHINES_BUDGET_MS)));
  } catch (e) {
    console.warn("readRemoteConfig failed:", e);
    return { hosts: [] };
  }
}

/** 那一答 → `RemoteConfig`（纯函数）。 */
export function remoteConfigOf(r: MachinesReply): RemoteConfig {
  if (r.state === "unreadable") return { hosts: [], unrecognized: r.reason ?? "" };
  return { hosts: r.hosts.map((h) => coerceHost(h as unknown as Record<string, unknown>)) };
}

/**
 * 在主机列表里按 origin 反查（纯函数）。origin = `label` 非空则 label 否则 host，与后端 `origin_label()` 同口径
 * （纯空白 label 除外：这里 trim、后端不 trim，那时反查落空，调用方出声）。找不到 → null。
 */
export function findHostByOrigin(
  hosts: RemoteHostConfig[],
  origin: string,
): RemoteHostConfig | null {
  return hosts.find((h) => hostKey(h) === origin) ?? null;
}

/** 按 origin（会话来源）反查完整 RemoteHostConfig。找不到（主机被删 / 改名）→ null。 */
export async function resolveRemoteConfigByOrigin(
  origin: string,
): Promise<RemoteHostConfig | null> {
  return findHostByOrigin((await readRemoteConfig()).hosts, origin);
}

/**
 * 落盘时要序列化的字段清单（单一来源）。`RemoteHostConfig` 加了字段而这里没跟上 ⇒ 每次保存都静默丢掉它；
 * 下面的 `MissingField` 检查让 `tsc` 当场红，并点名缺的是哪个字段。
 */
const REMOTE_HOST_FIELDS = [
  "label",
  "host",
  "port",
  "user",
  "keyPath",
  "hostKeyFingerprint",
  "addresses",
  "jump",
  "connect",
] as const satisfies readonly (keyof RemoteHostConfig)[];

/**
 * 上面清单**漏掉**的字段（应为 `never`）。`hostKeyPinnedAt` 不由界面写：后端写口跟着指纹盖上（`control/machines.rs::stamp_pinned_day`），界面只读。
 */
type MissingField = Exclude<
  keyof RemoteHostConfig,
  (typeof REMOTE_HOST_FIELDS)[number] | "hostKeyPinnedAt"
>;
// 漏了字段这一行就红，且 TS 的错误信息会把缺的字段名打出来。
const _noMissingField: MissingField extends never ? true : MissingField = true;
void _noMissingField;

/** 按 [`REMOTE_HOST_FIELDS`] 挑字段落盘（不逐字段手抄，杜绝静默丢失）。 */
function serializeHost(h: RemoteHostConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const k of REMOTE_HOST_FIELDS) out[k] = h[k];
  return out;
}

/** 一台机器在盘上的定位键 = 它的 origin（与 [`findHostByOrigin`] 同口径）。 */
export function hostKey(h: RemoteHostConfig): string {
  return h.label.trim() || h.host;
}

/** 一次局部修改：没被提到的机器不碰（补丁里没有它）。 */
export interface RemoteHostsPatch {
  /**
   * 要写入的机器。`key` = 这条记录**在盘上当前的 origin**；`null` = 新增（追加到末尾）。
   * key 在盘上找不到（被别处删了 / 改了）⇒ 整批拒、说出来（不当新增）。
   */
  upsert?: { key: string | null; value: RemoteHostConfig; was?: RemoteHostConfig }[];
  /** 要删除的机器，按 origin。 */
  remove?: string[];
}

/** 存完那一答里界面要的：写口替那几台盖上的「指纹记于哪一天」。 */
export interface RemotePatched {
  pinned: PinnedDay[];
}

/**
 * 界面写机器表的唯一入口。补丁全按键认那一台（[`remoteHostsEdits`]），不读、不整张写 ——
 * 认那一台在本机后端写口锁内现读现判（`machines-set`）。拒 / 问不到 ⇒ 抛。
 */
export async function patchRemoteConfig(patch: RemoteHostsPatch): Promise<RemotePatched> {
  const edits = remoteHostsEdits(patch);
  if (edits.length === 0) return { pinned: [] };
  const r = await callTyped(LOCAL_ORIGIN, "machines-set", { edits }, budgetWithin(MACHINES_BUDGET_MS));
  return { pinned: r.pinned };
}

/**
 * 这一次局部修改若现在落盘，机器表那一道过不过（试算，盘上不动；与写口同一份规则，`machines-try`）。过 ⇒ `null`。
 */
export async function tryRemoteConfig(patch: RemoteHostsPatch): Promise<MachineFault | null> {
  const edits = remoteHostsEdits(patch);
  if (edits.length === 0) return null;
  return (await callTyped(LOCAL_ORIGIN, "machines-try", { edits }, budgetWithin(MACHINES_BUDGET_MS))).fault;
}

/**
 * 一次局部修改 ⇒ 交给唯一写口的补丁：**全部按键认元素，没有整段写 `remote`**。**纯函数。**
 *
 * - 删（`remove`）⇒ 每台一条 `removein`：盘上认不出 / 认出多台 ⇒ 整批拒；
 * - 改（`key` ＋ 加载时那份 `was`）⇒ 动过的格各一条 `setin`（[`hostCellEdits`]）；
 * - 增（`key = null`）⇒ 一条 `insertin`：那个 origin 盘上已有 ⇒ 整批拒；
 * - `key` 在、`was` 缺（加载时那个 origin 不止一台）⇒ 按那个 origin 删了再插 —— 认出多台 ⇒ `removein` 整批拒，不猜是哪台。
 *
 * 先删后改再增（写口锁内按序应用）⇒「删掉 A、同时新增一台也叫 A」是替换；刚自动固化的指纹、别的窗口改的格都不会被盖掉。
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
    inserts.push({ op: "insertin", where: hostWhere(hostKey(u.value)), value: serializeHost(u.value) });
  }
  return [...edits, ...inserts];
}

/** 表里一台机器的认法：`label` 非空取它、否则 `host`（后端写口 `ElemKey` 的口径，同 `control/machines.rs::Host::name`）。 */
function hostWhere(key: string): { fields: string[]; equals: string }[] {
  return [{ fields: ["label", "host"], equals: key }];
}

/**
 * 一台已在盘上的机器（加载时 origin = `key`、加载时的样子 `was`）改成 `now`：只出动过的那几格，每格一条 `setin`。**纯函数。**
 *
 * 认它的键就是 origin，改 `label` / `host` 会换掉它 ⇒ 顺序：两者动了任一 ⇒ 先按旧 origin 把 `label` 写成表单值（盘上空 `label`
 * 读进来时被补成了 `host`，照写才与整台写那一形同一个 origin）；其余格随后按新 origin 认（`label` 空 ⇒ 仍是旧 `host`），`host` 排最后。
 */
export function hostCellEdits(key: string, was: RemoteHostConfig, now: RemoteHostConfig): MachineEdit[] {
  const a = serializeHost(was);
  const b = serializeHost(now);
  const moved = (f: (typeof REMOTE_HOST_FIELDS)[number]) => JSON.stringify(a[f]) !== JSON.stringify(b[f]);
  const cell = (k: string, field: string): MachineEdit => ({
    op: "setin",
    where: hostWhere(k),
    field,
    value: b[field],
  });
  const out: MachineEdit[] = [];
  let k = key;
  if (moved("label") || moved("host")) {
    out.push(cell(key, "label"));
    k = now.label !== "" ? now.label : was.host;
  }
  for (const f of REMOTE_HOST_FIELDS) {
    if (f !== "label" && f !== "host" && moved(f)) out.push(cell(k, f));
  }
  if (moved("host")) out.push(cell(k, "host"));
  return out;
}
