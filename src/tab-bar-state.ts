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
 * 本文件今天只装 `order`（`§C`）。`pinned`（`§B`，固定 tab）**还没做**，
 * 按 `§4` 它落在**同一个段**里 —— 那一刀来时往这里加一个键，不另开文件。
 * ⚠ 因此本文件的读写**刻意只动自己那个键**，不整段覆盖：
 *   否则 `B` 落地后，两条路会互相把对方的键写没。
 */
import { loadConfig, saveConfig } from "./config";

const KEY = "tabBar";

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
 */
export async function getTabOrder(alive: ReadonlySet<string>): Promise<string[]> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const seg = cfg[KEY];
    if (!seg || typeof seg !== "object" || Array.isArray(seg)) return [];
    return sanitizeOrder((seg as Record<string, unknown>).order, alive);
  } catch (e) {
    console.warn("getTabOrder failed:", e);
    return [];
  }
}

/**
 * 写顺序。**只动 `tabBar.order` 这一个键**，段里别的键（将来的 `pinned`）原样留着。
 *
 * ⚠ 调用方的形状是「**先改内存再落盘**」（`§C.3` 逐字）：UI 立刻响应，
 *   落盘失败只记日志 —— 顺序丢一次远好过拖动卡一下。
 */
export async function setTabOrder(order: readonly string[]): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const prev = cfg[KEY];
  const seg: Record<string, unknown> =
    prev && typeof prev === "object" && !Array.isArray(prev)
      ? { ...(prev as Record<string, unknown>) }
      : {};
  seg.order = sanitizeOrder(order, null);
  cfg[KEY] = seg;
  await saveConfig(cfg);
}
