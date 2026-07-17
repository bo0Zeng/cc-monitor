/**
 * 历史「来源筛选/折叠」持久化偏好的纯判定逻辑（F86 · issue #45）。
 *
 * 抽成**无 import 的纯模块**便于 node 轨断言——`history.ts` 依赖 Tauri IPC / DOM，直接测不了；
 * 把最易错的「首见默认折叠 vs 用户显式偏好」判定与反污染写入单拎出来钉契约。平行 `history-cache.ts`。
 *
 * 背景：多台来源时历史浏览器原本全默认展开（滚动很长），且来源筛选（隐藏某台）纯内存、重启即丢。
 * F86 要：① 隐藏偏好持久化；② 远端来源**默认折叠**、本地默认展开，用户逐级展开且偏好持久。
 *
 * 难点：`collapsedOrigins` 原是二态 Set（在集合=折叠/不在=展开），表达不了「远端默认折叠、但这一台
 * 用户显式展开过」——因为「显式展开」与「从没表态」都表现为「不在集合里」。解法 = 三态覆盖表
 * `OriginOpenOverrides`：**只持久化用户对默认值的偏离**，缺键 = 无偏好 = 走默认规则。
 *
 * SS-4 共享面②块「筛选/折叠持久化」的判定核心。①来源缓存见 `history-cache.ts`（F76）；
 * ③动作表/右键菜单见 F85/F96——本模块不碰。
 */

/** 折叠偏好覆盖表：key = `origin ?? ""`（本地为 ""），value = 用户显式设定的 open 态。
 *  **键缺失 = 无偏好 → 走 `defaultOriginOpen`**（这就是可表达的第三态）。 */
export type OriginOpenOverrides = Record<string, boolean>;

/**
 * 某来源大区的**首见默认** open 态：本地（`undefined`）→ 展开；远端 → 折叠。
 * `buildOriginGroup` 只在多来源时调用，故此规则天然只作用于多主机场景。
 */
export function defaultOriginOpen(origin: string | undefined): boolean {
  return origin === undefined;
}

/**
 * 解析某来源大区最终 open 态：搜索强制展开 > 用户显式覆盖 > 首见默认。
 * @param override 该 origin 在覆盖表里的值；`undefined` = 无偏好。
 */
export function resolveOriginOpen(
  override: boolean | undefined,
  origin: string | undefined,
  searchActive: boolean,
): boolean {
  if (searchActive) return true;
  return override ?? defaultOriginOpen(origin);
}

/**
 * 用户 toggle 某来源后的新覆盖表（**纯函数，返回新对象、不 mutate 入参**）。
 *
 * 反污染核心：**仅当 `newOpen` 偏离该 origin 默认值时存入；回到默认则删该键**。这一条同时解决：
 *  1. 首见默认折叠的**程序化**态（远端 `newOpen=false == 默认 false`）→ 删键（无键 no-op）→ 表保持
 *     干净，免疫「初始 `details.open=false` 赋值是否触发 toggle 监听器」这个 jsdom/Chromium 时序差异。
 *  2. 用户展开远端（`true ≠ 默认 false`）→ 存 `{host:true}` → 重开保持展开，**不被默认折叠盖掉**。
 *  3. 表最小化：只存偏离默认的项，不随见到的 origin 数膨胀。
 */
export function nextOverrides(
  cur: OriginOpenOverrides,
  key: string,
  origin: string | undefined,
  newOpen: boolean,
): OriginOpenOverrides {
  const next: OriginOpenOverrides = { ...cur };
  if (newOpen === defaultOriginOpen(origin)) {
    delete next[key]; // 回到默认 → 删键，回落默认规则
  } else {
    next[key] = newOpen; // 偏离默认 → 记为用户显式偏好
  }
  return next;
}

/** 防御性解析持久化 JSON（非对象/非法项一律丢），防脏 localStorage 污染。 */
export function normalizeOverrides(raw: unknown): OriginOpenOverrides {
  const out: OriginOpenOverrides = {};
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return out;
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof v === "boolean") out[k] = v;
  }
  return out;
}

/** 防御性解析隐藏来源持久化（照 `loadExpandedForks` 的 filter 语义：只留字符串项）。 */
export function normalizeOriginKeys(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  return raw.filter((x): x is string => typeof x === "string");
}

/**
 * 两张覆盖表内容是否相同（浅比较：同键集 + 同值）。用于 toggle 后**仅当变化才存盘**，
 * 消除生产宿主（WebView2/Chromium）对默认展开大区 `open=false→true` 异步触发 toggle → 每次
 * renderList 都对同一张表重复写的写放大（`nextOverrides` 幂等收敛，内容常不变）。
 */
export function sameOverrides(a: OriginOpenOverrides, b: OriginOpenOverrides): boolean {
  const ak = Object.keys(a);
  const bk = Object.keys(b);
  if (ak.length !== bk.length) return false;
  return ak.every((k) => a[k] === b[k]);
}
