/**
 * 标签页集合 —— 用户手动建、手动命名的分组。
 *
 * 住 `config.json`（`~/.cc-monitor/`，卸载默认不清）而不是 localStorage（WebView2 数据目录，清缓存就没了）：集合名是用户手写的真相，要活过清缓存。
 *
 * 组只存 `{id, 名字}`，组员关系是 tab 自己的属性：内存是 `Tab.group`（`tab-model.ts`），落盘是 `tabBar.groupOf.<sid>`（`tab-bar-state.ts`）；
 * 组员 = `group` 等于这个 id 的那些 tab，现算 —— 关掉的 tab 不会占着成员位。组的生命周期住 `tab-bar-prefs.ts`。
 *
 * 不自动归组（自动聚出来的东西用户改不动就是噪声）；一个 tab 只属一个集合（`Tab.group` 是单值）。
 */
import { loadConfig, setAt, type ConfigEdit } from "./config";
import { copyText } from "./copy-table";

const KEY = "tabCollections";

/** 集合条数上界。有界这件事由判据守着（灌满再验最老的被挤出），不是注释里说说。 */
export const COLLECTION_CAP = 32;
/** 名字长度上界（超了截断 —— 名字是展示用的，截断不丢别的东西）。 */
export const NAME_MAX = 40;

/** 一个组**只有**这两格 —— 没有成员名单（组员是「`Tab.group` 等于 `id` 的那些 tab」）。 */
export interface TabCollection {
  id: string;
  name: string;
}

/** 新集合的 id。用时间戳 + 随机后缀：它只需在本机唯一，不进任何协议。 */
export function newCollectionId(): string {
  return `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/**
 * 盘上读回来的集合 —— **盘上可能是任何东西**（用户手改 / 旧版本 / 半截写入）。
 *
 * 逐项筛，照 `behavior.ts` 那套宽容读法：认不出就丢，不抛。
 * 只取 `id` / `name` 两格；别的字段不带出去 —— 于是也写不回盘。
 */
export function sanitizeCollections(raw: unknown): TabCollection[] {
  if (!Array.isArray(raw)) return [];
  const out: TabCollection[] = [];
  const seenId = new Set<string>();
  for (const x of raw) {
    // 不加 `typeof x !== "object"` 守卫：数字 / 字符串 / 数组取 `.id` 都是 `undefined`，下面两行照样挡掉。
    if (!x) continue;
    const o = x as Record<string, unknown>;
    const id = typeof o.id === "string" ? o.id.trim() : "";
    const name = typeof o.name === "string" ? o.name.trim().slice(0, NAME_MAX) : "";
    if (!id || !name || seenId.has(id)) continue;
    seenId.add(id);
    out.push({ id, name });
    if (out.length >= COLLECTION_CAP) break;
  }
  return out;
}

export async function getCollections(): Promise<TabCollection[]> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    return sanitizeCollections(cfg[KEY]);
  } catch (e) {
    console.warn("getCollections failed:", e);
    return [];
  }
}

/**
 * 组表那一条补丁（整张组表换成 `list`，落盘前过一遍清洗）。
 *
 * 不自己 `patchConfig`：组的一次改动常常要与几个 tab 的组 id 键**同一批**落盘
 * （建组 ＝ 组表 ＋ 两个 `tabBar.groupOf.<sid>`），由 `tab-bar-prefs.ts` 把它们装进一次写。
 */
export function collectionsEdit(list: readonly TabCollection[]): ConfigEdit {
  return setAt([KEY], sanitizeCollections(list));
}

// ===== 纯操作（判据直接打这里）=====

/** 建一个集合。名字空 ⇒ 原样返回（空名不是一个集合）。到上界 ⇒ 原样返回。 */
export function createCollection(
  list: readonly TabCollection[],
  name: string,
  id: string = newCollectionId(),
): TabCollection[] {
  const n = name.trim().slice(0, NAME_MAX);
  if (!n || list.length >= COLLECTION_CAP) return [...list];
  return [...list, { id, name: n }];
}

/** 改名。空名 ⇒ 原样返回（不许把一个集合改成没名字）。 */
export function renameCollection(
  list: readonly TabCollection[],
  id: string,
  name: string,
): TabCollection[] {
  const n = name.trim().slice(0, NAME_MAX);
  if (!n) return [...list];
  return list.map((c) => (c.id === id ? { ...c, name: n } : c));
}

/**
 * 删集合。
 *
 * ★ **只解散分组，不碰任何会话** —— 集合是个视图，不是容器。
 * 调用方那边 `orderedIds` 一个都不许少，由判据钉住。
 */
export function deleteCollection(list: readonly TabCollection[], id: string): TabCollection[] {
  return list.filter((c) => c.id !== id);
}

/**
 * 组数到上界时**为什么没做** —— 纯判定，给调用方出声用。
 *
 * `createCollection` 到上界原样返回；调用方（右键菜单 · 拖放）据这里出声（[`collectionRefusalText`] 出句子），不静默。
 */
export type CollectionRefusal = { kind: "collections-full" };

/** 想新建一个集合：到上界 ⇒ 拒绝原因；没到 ⇒ `null`。 */
export function createRefusal(list: readonly TabCollection[]): CollectionRefusal | null {
  return list.length >= COLLECTION_CAP ? { kind: "collections-full" } : null;
}

/** 拒绝原因 → 对用户说的那一句（文案表 `tabCollections.full.*`）。今天只有一种原因，参数留着给类型说话。 */
export function collectionRefusalText(_r: CollectionRefusal): { title: string; body: string } {
  return {
    title: copyText("tabCollections.full.collectionsTitle"),
    body: copyText("tabCollections.full.collectionsBody", { cap: String(COLLECTION_CAP) }),
  };
}
