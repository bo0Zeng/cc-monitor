/**
 * P7a-3（#61）：**标签页集合** —— 用户手动建、手动命名的分组。
 *
 * # 为什么住 `config.json` 而不是 localStorage
 *
 * `data-section` 把两个存储分得很清：`config.json` 住 `monitorDataDir`
 * （`~/.claude/claudecode-frontend/`，**NSIS 卸载默认不清**，用户元数据保留）；
 * 而 localStorage 住 **WebView2 用户数据目录**，与 `cache` / `cookies` / `IndexedDB` 同处
 * ——那段文案逐字：「想彻底清除（星标 / 颜色配置 / WebView2 cache 等）请**手动删除上面的目录**」。
 *
 * 集合名是**用户手写的真相**，不是能重算的缓存 ⇒ 它必须活过一次清缓存。
 * ⚠ localStorage 恰恰是**最顺手的错路**：tab 的那些偏好（`lastActiveSid` 等）全在那儿。
 *
 * # 为什么成员按 `sid` 存
 *
 * 用一个每次都变的键的话，重启后集合就是空的 —— 整件白做。实测 `sid` **稳定**：
 * `main.ts` 启动时读 `lastActiveSid` 恢复上次那个 tab，而那件事只有在 sid 跨重启不变时才成立。
 *
 * # 不做什么
 *
 * **零自动归组**〔用 08-11 逐字「手动建, 不要自动, 纯手动」〕。
 * 我当时建议先做自动聚（tab 已带 cwd/origin，白得），用户的理由覆盖了我的：
 * **自动聚出来的东西用户改不动就是噪声**。
 *
 * **一个 tab 只属一个集合**（`U4` 正文建议「不能，保持树形，别一上来就做图」）。
 */
import { loadConfig, saveConfig } from "./config";

const KEY = "tabCollections";

/** 集合条数上界。有界这件事由判据守着（灌满再验最老的被挤出），不是注释里说说。 */
export const COLLECTION_CAP = 32;
/** 单个集合的成员上界。 */
export const MEMBER_CAP = 200;
/** 名字长度上界（超了截断 —— 名字是展示用的，截断不丢别的东西）。 */
export const NAME_MAX = 40;

export interface TabCollection {
  id: string;
  name: string;
  members: string[];
}

/** 新集合的 id。用时间戳 + 随机后缀：它只需在本机唯一，不进任何协议。 */
export function newCollectionId(): string {
  return `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/**
 * 盘上读回来的集合 —— **盘上可能是任何东西**（用户手改 / 旧版本 / 半截写入）。
 *
 * 逐项筛，照 `behavior.ts` 那套宽容读法：认不出就丢，不抛。
 */
export function sanitizeCollections(raw: unknown): TabCollection[] {
  if (!Array.isArray(raw)) return [];
  const out: TabCollection[] = [];
  const seenId = new Set<string>();
  const claimed = new Set<string>(); // 一个 sid 只准属一个集合
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
    const members: string[] = [];
    if (Array.isArray(o.members)) {
      for (const m of o.members) {
        if (typeof m !== "string") continue;
        const v = m.trim();
        if (!v || claimed.has(v) || members.includes(v)) continue;
        members.push(v);
        claimed.add(v);
        if (members.length >= MEMBER_CAP) break;
      }
    }
    seenId.add(id);
    out.push({ id, name, members });
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

export async function setCollections(list: readonly TabCollection[]): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg[KEY] = sanitizeCollections(list);
  await saveConfig(cfg);
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
  return [...list, { id, name: n, members: [] }];
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

/** 把 `sid` 加进某集合。**先从别的集合移出** —— 一个 tab 只属一个集合。 */
export function addMember(
  list: readonly TabCollection[],
  id: string,
  sid: string,
): TabCollection[] {
  const v = sid.trim();
  if (!v) return [...list];
  const target = list.find((c) => c.id === id);
  if (!target || target.members.length >= MEMBER_CAP) return [...list];
  return list.map((c) =>
    c.id === id
      ? { ...c, members: c.members.includes(v) ? c.members : [...c.members, v] }
      : { ...c, members: c.members.filter((m) => m !== v) },
  );
}

/** 从**所有**集合里移出 `sid`（它最多在一个里，但按集合语义写成幂等的）。 */
export function removeMember(list: readonly TabCollection[], sid: string): TabCollection[] {
  return list.map((c) => ({ ...c, members: c.members.filter((m) => m !== sid) }));
}

/** `sid` 属于哪个集合（`null` = 不属于任何一个）。 */
export function collectionOf(
  list: readonly TabCollection[],
  sid: string,
): TabCollection | null {
  return list.find((c) => c.members.includes(sid)) ?? null;
}
