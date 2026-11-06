/**
 * `config.json` 的 `tabBar` 段 —— 这扇窗自己的看法（拖出来的顺序重算不出来，要活过重启）。组与固定是后端的数据（`group-book.ts`），不在这里。
 *
 * 三个键：`order`（顺序）· `collapsed`（收着的组 id）· `hiddenOpen`（栏底「已隐藏」那一行展开着没有，缺 ＝ 收着）。
 * 每个键只动自己：`writeSegKey` 是它们唯一的写口（`tab-bar-state.vitest.ts` 各有一格盯着）。
 */
import { loadConfig, patchConfig, setAt } from "./config";

const KEY = "tabBar";

/**
 * 段里两个键唯一的写口 —— 只交 `tabBar.<field>` 这一条路径（按键补丁；读出整段再写回去，两个键会互盖）。
 * 一个函数而不是两处各写：「不写没别人的键」是同一条不变量的两侧。
 * 不做 try/catch：写失败要让调用方知道（调用方的形状是
 *   「先改内存再落盘，落盘失败只记日志」——日志由调用方打，不是这里吞掉）。
 */
async function writeSegKey(field: "order" | "collapsed" | "hiddenOpen", value: unknown): Promise<void> {
  await patchConfig([setAt([KEY, field], value)]);
}

/**
 * 段里两个键唯一的读口。段不是对象（数组 / 字符串 / 缺失）⇒ 回 `undefined`，
 * 由各自的 `sanitize*` 把它变成空表。
 */
async function readSegKey(field: "order" | "collapsed" | "hiddenOpen"): Promise<unknown> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const seg = cfg[KEY];
  if (!seg || typeof seg !== "object" || Array.isArray(seg)) return undefined;
  return (seg as Record<string, unknown>)[field];
}

/**
 * 顺序的上界。
 *
 * 不是「最多能有几个 tab」，是落盘的顺序表最长记到哪：「现有 tab 数」写的时候不知道 ⇒ 给一个结构性上界，
 * 按现有 tab 过滤发生在应用那一刻。两道一起才够：只靠过滤，撑爆的配置照样写进盘；只靠上界，已删会话的 sid 一直挂着。
 */
export const ORDER_CAP = 256;

/**
 * 清洗落盘来的顺序：认不出就丢，不抛。
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
 * `alive` 传 `null`（生产只走这一档）：读在启动那一刻，tab 随后陆续建出来 —— 那一刻「已删的会话」与「还没到的会话」长得一样，
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
 * 写顺序。**只动 `tabBar.order` 这一个键**，段里别的键原样留着。
 *
 * ⚠ 调用方的形状是「**先改内存再落盘**」（`§C.3` 逐字）：UI 立刻响应，
 *   落盘失败只记日志 —— 顺序丢一次远好过拖动卡一下。
 */
export async function setTabOrder(order: readonly string[]): Promise<void> {
  await writeSegKey("order", sanitizeOrder(order, null));
}

// ===== 收着的组（`tabBar.collapsed`）=====

/** 收着的组最多记几个（组 id；多出来的丢，最坏是那一组展开着）。 */
export const COLLAPSED_CAP = 64;

/** 清洗盘上的收着的组：只认非空字符串，去重，有上界。认不出就丢，不抛。 */
export function sanitizeCollapsed(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  for (const x of raw) {
    const v = typeof x === "string" ? x.trim() : "";
    if (!v || out.includes(v)) continue;
    out.push(v);
    if (out.length >= COLLAPSED_CAP) break;
  }
  return out;
}

/** 读收着的组。读失败不阻断启动：最坏也就是组都展开着。 */
export async function getCollapsed(): Promise<string[]> {
  try {
    return sanitizeCollapsed(await readSegKey("collapsed"));
  } catch (e) {
    console.warn("getCollapsed failed:", e);
    return [];
  }
}

/** 写收着的组。**只动 `tabBar.collapsed` 这一个键**。 */
export async function setCollapsed(ids: readonly string[]): Promise<void> {
  await writeSegKey("collapsed", sanitizeCollapsed(ids));
}

// ===== 栏底「已隐藏」那一行收 / 展（`tabBar.hiddenOpen`）=====

/** 读「已隐藏」那一行展开着没有：只认 `true`，别的（缺 · 认不出 · 读失败）⇒ 收着。 */
export async function getHiddenOpen(): Promise<boolean> {
  try {
    return (await readSegKey("hiddenOpen")) === true;
  } catch (e) {
    console.warn("getHiddenOpen failed:", e);
    return false;
  }
}

/** 写「已隐藏」那一行收 / 展。**只动 `tabBar.hiddenOpen` 这一个键**。 */
export async function setHiddenOpen(on: boolean): Promise<void> {
  await writeSegKey("hiddenOpen", on);
}
