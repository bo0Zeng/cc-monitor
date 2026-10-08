/**
 * `config.json` 的 `tabBar` 段 —— tab 栏里那些用户手写的真相（拖出来的顺序、固定、分组都重算不出来，要活过重启）。
 *
 * 三个键：`order`（顺序）· `pinned`（固定的 tab）· `groupOf`（每个 tab 自己的组 id，`tabBar.groupOf.<sid> = <组 id>`）。
 * 每个键只动自己：`writeSegKey` 是 order / pinned 唯一的写口（`tab-bar-state.vitest.ts` 各有一格盯着），
 * groupOf 按 tab 一条路径写（[`groupOfEdit`] 是唯一的造法）—— 盘上没有「一个组的成员名单」。
 * 不并进 `tabCollections`：那一段已经稳了，动它只添迁移风险。
 */
import { loadConfig, patchConfig, removeAt, setAt, type ConfigEdit } from "./config";
import type { Origin } from "./ipc/origin";

const KEY = "tabBar";

/**
 * 段里两个键唯一的写口 —— 只交 `tabBar.<field>` 这一条路径（按键补丁；读出整段再写回去，两个键会互盖）。
 * 一个函数而不是两处各写：「不写没别人的键」是同一条不变量的两侧。
 * 不做 try/catch：写失败要让调用方知道（调用方的形状是
 *   「先改内存再落盘，落盘失败只记日志」——日志由调用方打，不是这里吞掉）。
 */
async function writeSegKey(field: "order" | "pinned", value: unknown): Promise<void> {
  await patchConfig([setAt([KEY, field], value)]);
}

/**
 * 段里两个键唯一的读口。段不是对象（数组 / 字符串 / 缺失）⇒ 回 `undefined`，
 * 由各自的 `sanitize*` 把它变成空表。
 */
async function readSegKey(field: "order" | "pinned" | "groupOf"): Promise<unknown> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const seg = cfg[KEY];
  if (!seg || typeof seg !== "object" || Array.isArray(seg)) return undefined;
  return (seg as Record<string, unknown>)[field];
}

/**
 * 顺序的上界。
 *
 * 不是「最多能有几个 tab」，是落盘的顺序表最长记到哪（`COLLECTION_CAP` 的 8 倍）：「现有 tab 数」写的时候不知道 ⇒ 给一个结构性上界，
 * 按现有 tab 过滤发生在应用那一刻。两道一起才够：只靠过滤，撑爆的配置照样写进盘；只靠上界，已删会话的 sid 一直挂着。
 */
export const ORDER_CAP = 256;

/**
 * 清洗落盘来的顺序。形状照 `sanitizeCollections`（`tab-collections.ts:54`）。
 *
 * @param raw   盘上读到的东西（什么都可能）
 * @param alive 今天真的存在的 sid。**给 `null` 表示「这一趟不按存活过滤」** ——
 *              写盘那一侧用它：写的时候不该把「此刻恰好没建出来的 tab」摘掉。
 *
 * ⚠ 去重是**必须的**：同一个 sid 出现两次，`orderedIds` 就会让那个 tab 渲染两遍。
 */
export function sanitizeOrder(raw: unknown, alive: ReadonlySet<string> | null): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  const seen = new Set<string>();
  for (const x of raw) {
    if (typeof x !== "string") continue;
    const v = x.trim();
    if (!v || seen.has(v)) continue;
    if (alive !== null && !alive.has(v)) continue;
    seen.add(v);
    out.push(v);
    if (out.length >= ORDER_CAP) break;
  }
  return out;
}

/**
 * 读顺序。
 *
 * 读失败不阻断启动：坏掉的配置最坏也就是顺序退回默认。
 *
 * `alive` 传 `null`（生产只走这一档）：读在启动那一拍，tab 随后陆续建出来 —— 那一刻「已删的会话」与「还没到的会话」长得一样，
 *   拿当时的存活集过滤会把整张顺序摘掉。过滤在应用那一刻做（`TabManager.applySavedOrder` 按此刻在的 sid 筛）。
 */
export async function getTabOrder(alive: ReadonlySet<string> | null): Promise<string[]> {
  try {
    return sanitizeOrder(await readSegKey("order"), alive);
  } catch (e) {
    console.warn("getTabOrder failed:", e);
    return [];
  }
}

/**
 * 写顺序。**只动 `tabBar.order` 这一个键**，段里别的键（`pinned`）原样留着。
 *
 * ⚠ 调用方的形状是「**先改内存再落盘**」（`§C.3` 逐字）：UI 立刻响应，
 *   落盘失败只记日志 —— 顺序丢一次远好过拖动卡一下。
 */
export async function setTabOrder(order: readonly string[]): Promise<void> {
  await writeSegKey("order", sanitizeOrder(order, null));
}

// ===== `§B` 固定（pinned）=====

/**
 * 一条固定记录：字段是「重启后把一个没有活进程的 tab 恢复出来」要的那几样，一个不多。
 * 落盘的是条目不是内容（已结束的会话点进去只能 resume，内容由 resume 拿）。
 */
export interface PinnedTab {
  /** resume 的主键。 */
  sid: string;
  /** = `Tab.parentPath`。空串 = 这条从没收到过带路径的行（降级，不是丢）。 */
  jsonlPath: string;
  /** = `Tab.projectDir`（后端给的项目目录；复活时先用它，那台再宣告 / 会话事实到了就对齐）。 */
  cwd: string | null;
  /**
   * 哪台机器（决定复活后走哪条读命令 / resume 往哪台机去）。本机 = `LOCAL_ORIGIN`；`null` 由 [`sanitizePinned`] 按坏行丢。
   */
  origin: Origin;
  /**
   * 非有不可：缺了 resume 会静默落到默认号。`null` = 没记到，不是「默认号」。
   */
  account: string | null;
  /**
   * 列表排序与「说不清」那一态要用。`null` = 说不清：只有活着的 tab 落盘时有读数（「此刻还活着」），
   *   已经灰了的 tab 什么时候最后活动过前端不知道 ⇒ 沿用盘上那份，否则 `null`。
   */
  lastActiveAt: number | null;
  /** 后台会话（复活骨架时喂给 `createSkeletonTab`）。 */
  background: boolean;
  /** bgName（同上）。 */
  name: string | null;
  /** 存一份，骨架期就能显示正确标题（不等读完）。 */
  title: string;
}

/**
 * 固定条数上界 —— `§4` 那张表逐字「**上界 32 条**（照 `COLLECTION_CAP` 的量级）」。
 *
 * ⚠ 与 `ORDER_CAP` 不同，这里**没有第二道** `alive` 过滤：`pinned` 的全部意义
 * 就是「这条今天不在，明天也要把它造出来」—— 按存活过滤等于把这个功能过滤掉。
 */
export const PINNED_CAP = 32;

/** 盘上的字符串：不是字符串或空白 ⇒ 空串。 */
function pinStr(v: unknown): string {
  return typeof v === "string" ? v.trim() : "";
}
/** 同上，但空串收敛成 `null`（可空字段用）。 */
function pinStrOrNull(v: unknown): string | null {
  const s = pinStr(v);
  return s === "" ? null : s;
}

/**
 * 清洗落盘来的固定表。形状照 `sanitizeCollections`（认不出就丢，不抛）。
 *
 * 逐条筛的四条：
 * 1. `sid` 是主键 —— 空的 / 重复的整条丢（重复会让同一个 tab 被复活两遍）。
 * 1b. `origin` 不是非空字符串 ⇒ 整条丢：本机也有名字（`"<local>"`），
 *    「没写哪台机」复活不出来 —— 猜成本机就是治的那件事。
 * 2. `jsonlPath` 为空**保留**（`§4` 逐字「保留但标记降级」）—— 丢掉它等于
 *    「用户固定过的东西第二天自己没了」，那比降级坏。降级的判词住 `isDegradedPin`。
 * 3. 文件已被删的那一条要自动摘除（`§4`）—— **今天做不到**，理由写在
 *    `isDegradedPin` 的头注里（前端没有任何文件存在性探针）。
 */
export function sanitizePinned(raw: unknown): PinnedTab[] {
  if (!Array.isArray(raw)) return [];
  const out: PinnedTab[] = [];
  const seen = new Set<string>();
  for (const x of raw) {
    if (!x) continue;
    const o = x as Record<string, unknown>;
    const sid = pinStr(o.sid);
    if (!sid || seen.has(sid)) continue;
    const origin = pinStr(o.origin);
    if (!origin) continue;
    const ts = o.lastActiveAt;
    seen.add(sid);
    out.push({
      sid,
      jsonlPath: pinStr(o.jsonlPath),
      cwd: pinStrOrNull(o.cwd),
      origin,
      account: pinStrOrNull(o.account),
      lastActiveAt: typeof ts === "number" && Number.isFinite(ts) ? ts : null,
      background: o.background === true,
      name: pinStrOrNull(o.name),
      // 标题缺了也要有个认得出的东西 —— 照全仓那条「session_id 前 8 位」的兜底。
      title: pinStr(o.title) || sid.slice(0, 8),
    });
    if (out.length >= PINNED_CAP) break;
  }
  return out;
}

/**
 * 这条固定记录是不是降级的。
 * 文件已不在的不自动摘：前端没有文件存在性探针，复活又不读内容 ⇒ 判不了。
 */
export function isDegradedPin(p: PinnedTab): boolean {
  return p.jsonlPath === "";
}

/**
 * 读固定表。
 *
 * ⚠ 读失败**不阻断启动**（`§4` 逐字，照 `loadCollections` 的 try/catch 形状）：
 *   一份坏掉的配置最坏也就是这次少几个灰 tab，不该让 tab 栏起不来。
 */
export async function getPinned(): Promise<PinnedTab[]> {
  try {
    return sanitizePinned(await readSegKey("pinned"));
  } catch (e) {
    console.warn("getPinned failed:", e);
    return [];
  }
}

/** 写固定表。**只动 `tabBar.pinned` 这一个键**，段里别的键（`order`）原样留着。 */
export async function setPinned(list: readonly PinnedTab[]): Promise<void> {
  await writeSegKey("pinned", sanitizePinned(list));
}

// ===== tab 的组 id（`tabBar.groupOf.<sid>`）=====

/**
 * 清洗盘上的 `tabBar.groupOf`：`{ <sid>: <组 id> }`。认不出就丢，不抛（照 `sanitizePinned`）。
 *
 * - 段不是对象（数组 / 字符串 / 缺失）⇒ 空表。
 * - 键原样保留（不 `trim`）：它就是写盘那条路径的最后一段，改了它，之后的 `removeAt` 就摘不到原来那条。
 * - 值必须是非空字符串（一个组 id）；数组 / 对象 / 空串整条丢 —— **这里只认「一个 tab 一个组」**。
 * ⚠ 「指向的组还在不在」不在这里判（那要组表，住 `tab-bar-prefs.ts::loadCollections`）。
 */
export function sanitizeGroupOf(raw: unknown): Map<string, string> {
  const out = new Map<string, string>();
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return out;
  for (const [sid, v] of Object.entries(raw as Record<string, unknown>)) {
    const gid = typeof v === "string" ? v.trim() : "";
    if (!sid || !gid) continue;
    out.set(sid, gid);
  }
  return out;
}

/** 读 `tabBar.groupOf`。读失败**不阻断启动**（`§4` 逐字）：最坏也就是这次 tab 都不带组。 */
export async function getGroupOf(): Promise<Map<string, string>> {
  try {
    return sanitizeGroupOf(await readSegKey("groupOf"));
  } catch (e) {
    console.warn("getGroupOf failed:", e);
    return new Map();
  }
}

/**
 * 🔴 **`tabBar.groupOf.<sid>` 这条路径唯一的造法**：`gid` 非空 ⇒ `set`；`null` ⇒ `remove`（这个 tab 不在任何组里）。
 *
 * 不自己写盘：一次分组改动常常要把组表与几个 tab 的这一键**装进同一次** `patchConfig`（`tab-bar-prefs.ts`）。
 */
export function groupOfEdit(sid: string, gid: string | null): ConfigEdit {
  return gid === null ? removeAt([KEY, "groupOf", sid]) : setAt([KEY, "groupOf", sid], gid);
}
