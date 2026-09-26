/**
 * `config.json` 的 `tabBar` 段 —— **tab 栏里那些「用户手写的真相」**。
 *
 * # 它为什么存在（`设计/30 §C.2` 逐字：**这是设计漏洞，不是未做的功能**）
 *
 * `tab-collections.ts` 立集合落盘时写的理由是：
 * 「**用户手写的真相，不是能重算的缓存**」。
 *
 * 而**拖动排序完全符合那条判据** —— 它是纯手工输入，重算不出来。
 * 在这之前 `orderedIds` 的 8 个写入点**零持久化** ⇒ 同一个栏里两种寿命：
 * **你手动建的分组活过重启，你手动拖的顺序活不过。**
 * ⇒ 同一条理由必须给它同样的待遇，否则那条理由就是**选择性适用**的。
 *
 * # 为什么新开一个段，不合进 `tabCollections`（`§4` 逐字）
 *
 * `tabCollections` 已经有落盘、清洗、上界和一份判据齐全的 vitest。
 * 动它等于给一个已经稳的东西加迁移风险，**换不到任何东西**。新的东西住新地方。
 *
 * # 射程
 *
 * 本文件装 `§4` 那张表里的**两个键**：`order`（`§C`，2026-09-19 上午）与
 * `pinned`（`§B`，固定 tab，2026-09-19 下午）。按 `§4` 它们住**同一个段**，
 * 不另开文件 —— 「一个事实一个住址」在这里的形是「一段配置一个模块」。
 * ⚠ 因此本文件的读写**刻意只动自己那个键**，不整段覆盖：
 *   否则两条路会互相把对方的键写没。
 *   🔴 这条不靠自觉：`writeSegKey` 是**两个键唯一的写口**（下面那个函数），
 *   而 `tests/tab-bar-state.vitest.ts` 对 `order`/`pinned` **各有一格**专盯它。
 *
 * 〔GRP1 · `设计/99 §1` V140〕第三个键 `groupOf`：**每个 tab 自己的组 id**（`tabBar.groupOf.<sid> = <组 id>`）。
 * V140「组员关系是 tab 自己的属性（tab 上带组 id，随 tab 的持久记录一起存）」⇒ 与固定 / 顺序同段；
 * 它的写**按 tab 一条路径**（[`groupOfEdit`] 是这条路径唯一的造法），不整张重写 —— 于是盘上没有「一个组的成员名单」这种东西。
 */
import { loadConfig, patchConfig, removeAt, setAt, type ConfigEdit } from "./config";
import type { Origin } from "./ipc/origin";

const KEY = "tabBar";

/**
 * 🔴 **段里两个键唯一的写口** —— 只交 `tabBar.<field>` 这一条路径（〔CFG1〕按键补丁；从前是
 * 「读出整段、只覆盖 `field`、写回去」，同一拍 order 与 pinned 各读各写仍会互盖，分组那一键更是整份被盖）。
 *
 * 为什么抽成一个函数而不是两处各写一遍：`§B` 与 `§C` 是**同一条不变量的两侧**
 * （「不写没别人的键」），两处各抄一份的话，将来只改一侧就是一次静默的互相清空。
 * ⚠ 这里**刻意不做 try/catch**：写失败要让调用方知道（调用方的形状是
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
 * 🔴 **它不是「最多能有几个 tab」** —— 是「落盘的顺序表最长记到哪」。
 * 取 `COLLECTION_CAP` 的量级（32）的 8 倍：`§4` 那张表逐字「上界 = 现有 tab 数」，
 * 而「现有 tab 数」在**读的时候**才知道，写的时候没有 ⇒ 这里给一个**结构性上界**，
 * 真正按「现有 tab」过滤发生在 `sanitizeOrder(raw, alive)` 的 `alive` 那一参上。
 * ⚠ 两道一起才够：只有 `alive` 过滤 ⇒ 一份被撑爆的配置照样能写进盘；
 *   只有这个上界 ⇒ 已删会话的 sid 会一直挂着。
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
 * ⚠ 读失败**不阻断启动**（照 `loadCollections` 的 try/catch 形状，`§4` 逐字）——
 *   一份坏掉的配置不该让 tab 栏起不来，最坏也就是顺序退回默认。
 *
 * 🔴 **`alive` 收 `null`（2026-09-21 修步 17·C 那个 no-op 时加的）** —— 理由不是图省事：
 *   **「按存活过滤」这件事不能在读的这一刻做。** 读发生在启动那一拍，而 tab 是随后由
 *   `session_added` / 首行**陆续**建出来的（`main.ts` 那个启动窗口期给到 30s）。
 *   ⇒ 在那一刻，「已经被删掉的会话」与「还没到的会话」**长得一模一样**，
 *   拿当时的存活集去过滤 = 把整张顺序当成死 sid 摘掉（那正是原来那个 bug 的机制）。
 *   ⇒ 过滤挪到**应用**那一刻做（`TabManager.applySavedOrder` 按 `orderedIds` 里
 *   此刻真的在的 sid 筛），读这一侧只负责如实把盘上那份意图交出来。
 *   传 `Set` 那一路仍然守着（`sanitizeOrder` 的 `alive` 两档各有判据），
 *   但**生产今天只走 `null` 这一档** —— 如实记在这里，别当它还有别的消费者。
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
 * 一条固定记录。**字段表逐条照 `设计/30 §B.5`**，一个不多一个不少 ——
 * 那张表不是清单，是由「重启后要把一个**没有活进程**的 tab 恢复出来」反推出来的。
 *
 * 🔴 **落盘的是条目，不是内容**（`§3.5.7` 逐字）：内容由 resume 拿
 * （`99 §2.5 P3` 已裁定「已结束的会话点进去**不能**看内容，只能 resume」）
 * ⇒ 不需要回答「存全量还是存尾部」那类容量问题。
 */
export interface PinnedTab {
  /** resume 的主键。 */
  sid: string;
  /** = `Tab.parentPath`。空串 = 这条从没收到过带路径的行（`§B.6` 第一格：降级，不是丢）。 */
  jsonlPath: string;
  cwd: string | null;
  /**
   * 哪台机器（决定复活后走哪条读命令 / resume 往哪台机去）。本机 = `LOCAL_ORIGIN`。
   * 〔C4a · `设计/05 §8` 步 2〕上一版这里是 `null` = 本机；盘上的旧 `null` **不兼容**，
   * 由 [`sanitizePinned`] 按坏行丢（与 Rust `Origin` 反序列化那道闸同形）。
   */
  origin: Origin;
  /**
   * 🔴 **非有不可**（`§3.5.7`）：缺了 resume 会静默落到默认号，
   * 撞 `accounts.ts` 那条「绝不下沉到当前号」的纪律。
   * ⚠ `null` 在这里是**诚实的「没记到」**，不是「默认号」—— 两者不是一回事。
   */
  account: string | null;
  /**
   * 列表排序与「说不清」那一态要用（`§B.5` 逐字）。
   * ⚠ `null` = **说不清**（`01 §6.9`：判不了就说判不了）。今天只有 live tab 落盘时
   *   才有一个诚实的读数（「此刻它还活着」）；已经灰了的 tab 什么时候最后活动过，
   *   前端**没有这个数**（`Tab` 上零时间戳字段，现打），所以原样沿用盘上那份、否则 `null`。
   */
  lastActiveAt: number | null;
  /** bg 任务 vs 交互（复活骨架时喂给 `createSkeletonTab`）。 */
  kind: string | null;
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
 * 1b. 〔C4a〕`origin` 不是非空字符串 ⇒ 整条丢：本机也有名字（`"<local>"`），
 *    「没写哪台机」复活不出来 —— 猜成本机就是 `设计/05 §8` 步 2 治的那件事。
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
      kind: pinStrOrNull(o.kind),
      name: pinStrOrNull(o.name),
      // 标题缺了也要有个认得出的东西 —— 照全仓那条「session_id 前 8 位」的兜底。
      title: pinStr(o.title) || sid.slice(0, 8),
    });
    if (out.length >= PINNED_CAP) break;
  }
  return out;
}

/**
 * 这条固定记录是不是**降级**的（`§B.6` 第一格 / `§4` 那张表的「标记降级」）。
 *
 * 🔴 **`§4` 还要求「文件不存在的自动摘除」，那一条今天做不到，如实记在这里**：
 * 前端一侧**没有任何文件存在性探针** —— `src/ipc/commands.ts` 里零个 `exists`
 * 类命令，`package.json` 也没有 `@tauri-apps/plugin-fs`（两处现打）。
 * 唯一能碰到那个文件的是 `stream_read_session_jsonl`，而 `99 §2.5 P3` 已裁定
 * **复活不读内容** ⇒ 拿它当存在性探针就是绕过那条裁定。
 * ⇒ 要做这一条得先加一个后端命令，**那不在本刀的写区**。判不了就写判不了。
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

// ===== 〔GRP1 · `设计/99 §1` V140〕tab 的组 id（`tabBar.groupOf.<sid>`）=====

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
