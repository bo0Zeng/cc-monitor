/**
 * P7a-3（#61）：**标签页集合** —— 用户手动建、手动命名的分组。
 *
 * # 为什么住 `config.json` 而不是 localStorage
 *
 * `data-section` 把两个存储分得很清：`config.json` 住 `monitorDataDir`
 * （`~/.cc-monitor/`，**NSIS 卸载默认不清**，用户元数据保留）；
 * 而 localStorage 住 **WebView2 用户数据目录**，与 `cache` / `cookies` / `IndexedDB` 同处
 * ——那段文案逐字：「想彻底清除（星标 / 颜色配置 / WebView2 cache 等）请**手动删除上面的目录**」。
 *
 * 集合名是**用户手写的真相**，不是能重算的缓存 ⇒ 它必须活过一次清缓存。
 * ⚠ localStorage 恰恰是**最顺手的错路**：tab 的那些偏好（`lastActiveSid` 等）全在那儿。
 *
 * # 🔴 组只存 `{id, 名字}`，组员关系是 tab 自己的属性
 *
 * 用户原话：「分组不应该单独存会话记录. x就是没了, 不存在还要移出分组」。
 * 从前这里每个组带一张 `members: string[]`（会话 id 名单）⇒ 关掉的 tab 的 sid 永远占着成员位、
 * resume 同一个 sid 又回到组里、成员上界会被死成员占满。现在：
 * - 组表（config `tabCollections`）每一项**只有** `id` 与 `name` 两格；
 * - 「这个 tab 在哪个组」住 tab 自己身上：内存是 `Tab.group`（`tab-model.ts`），
 *   落盘是 tab 的持久记录里那一键 `tabBar.groupOf.<sid>`（段主人 `tab-bar-state.ts`）；
 * - 组员 = `group` 等于这个 id 的那些 tab，**现算**，哪儿都不另存一张名单。
 * 旧盘上的 `members` 一概不读、不写回（`no-legacy-compat`：不迁移、不兼容）——下一次写组表时它自然消失。
 * 组的生命周期（× 连组关系一起没 · 组里最后一个 tab 关掉组消失）住 `tab-bar-prefs.ts`。
 *
 * # 不做什么
 *
 * **零自动归组**〔用 08-11 逐字「手动建, 不要自动, 纯手动」〕。
 * 我当时建议先做自动聚（tab 已带 cwd/origin，白得），用户的理由覆盖了我的：
 * **自动聚出来的东西用户改不动就是噪声**。
 *
 * **一个 tab 只属一个集合**（`U4` 正文建议「不能，保持树形，别一上来就做图」）——`Tab.group` 是单值，结构上就成立。
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
 * 只取 `id` / `name` 两格；别的字段（旧形状的 `members`）不带出去 —— 于是也写不回盘。
 */
export function sanitizeCollections(raw: unknown): TabCollection[] {
  if (!Array.isArray(raw)) return [];
  const out: TabCollection[] = [];
  const seenId = new Set<string>();
  for (const x of raw) {
    // ⚠ 这里**刻意没有** `typeof x !== "object" || Array.isArray(x)` 那道守卫：
    // 变异实测它**恒不承重** —— 数字/字符串/数组取 `.id` 都是 `undefined`，
    // 下面那两行照样把它们挡掉。留一道任何输入都区分不出的守卫，就是一条假绿的防线
    //（本仓删过一条同样形状的恒绿判据）。
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
 * `createCollection` 到上界照旧**原样返回**（数据层的性质不变，判据钉在上界常量上）；
 * 从前两个调用方（右键菜单 · 拖放）拿到原样返回就一句话都不说 —— 撞「一条都不许静默忽略」。
 * ⇒ 判定住这里（一个家），说那一句的是调用方（[`collectionRefusalText`] 出句子）。
 * 原先还有一种「组员满了」（成员上界）—— 成员上界随成员名单一起作废，那一种也没了。
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
