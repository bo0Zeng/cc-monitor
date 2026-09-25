/**
 * 〔U2 · 拆 `tabs.ts` ④〕**拖动排序 / 拖动成组的落点算术** —— 全是纯函数，判据直接打在它们身上。
 *
 * 零 DOM、零 IPC：量尺寸（`getBoundingClientRect`）与挂监听是 `tab-bar-drag.ts` 的事，
 * 这里只回答「这几个矩形 ＋ 这个指针位置 ⇒ 落在哪」「这次落点对集合表意味着什么」。
 * 原住 `tabs.ts`，逐字搬出；`tabs.ts` 原样 re-export，既有 import 面（`tabs.vitest.ts`）零改动。
 */
import {
  addMember,
  collectionOf,
  createCollection,
  removeMember,
  type TabCollection,
} from "./tab-collections";

/**
 * P7a-2：把被拖的那个 `sid` 挪到 `beforeSid` 之前。
 * `beforeSid === null` = 挪到末尾。**纯函数** —— 判据直接打在落位上，不必先造一次真拖拽。
 *
 * ⚠ 落点就是它自己 ⇒ **原样返回**（拖到自己身上不是一次重排，把它算成「挪到末尾」是错的）。
 * 〔BG1 · V125「删掉树」〕原先挪的是「一块」（被拖的交互 tab 连同它的 bg 子串）；
 *   树删了 ⇒ 块的概念没了，一次只拖一个。
 */
export function moveTab(
  order: readonly string[],
  sid: string,
  beforeSid: string | null,
): string[] {
  if (beforeSid === sid) return [...order];
  const rest = order.filter((x) => x !== sid);
  if (beforeSid === null) return [...rest, sid];
  const at = rest.indexOf(beforeSid);
  if (at < 0) return [...rest, sid];
  return [...rest.slice(0, at), sid, ...rest.slice(at)];
}

// ===== 〔步 17·D · `设计/30 §D`〕Edge 式拖动合并成组 =====

/**
 * 落点语义。**从 1 种扩到 3 种**（`§D.3` 逐字）—— 在这之前只有 `before`（与 `null`＝末尾）。
 *
 * ⚠ `onto` 与 `before` 的区别不只是「插哪儿」：`onto` 会**建组 / 入组**，
 *   而落点所在的容器还顺带决定「拖出组」（`§D.7`）。两件事在 `applyDropToCollections` 里合一。
 */
export type DropTarget =
  | { kind: "before"; sid: string } // 插到它前面（今天的行为）
  | { kind: "onto"; sid: string } // 🆕 与它成组
  | { kind: "end" }; // 落到末尾（今天的 `null`）

/**
 * `onto` 的触发：**停留**，不是三等分（`§D.4` 已推荐，理由三条）。
 *
 * 🔴 **为什么不三等分**：竖栏里 tab 高约 28px，切成上/中/下三档 = 每档 9px。
 * 9px 的判定区在实际拖动里误触率极高 —— 用户想插到两个 tab 之间，结果建了个组。
 * 而插入排序是**快动作**、成组是**慢动作**，两个手势在**时间**上天然分开，
 * 不用去抢那 9px 的空间。
 */
export const DWELL_MS = 250;
/** 停留期间允许的抖动。超过就不算「压住」，退回 `before`/`end`（`§D.4` 逐字）。 */
export const DWELL_MOVE_PX = 4;

/** 一个 tab 按钮在纵轴上占的那一段。判据直接喂这个，不必先造一次真 DOM 布局。 */
export interface TabRect {
  sid: string;
  top: number;
  height: number;
}

/**
 * 指针**正压在**哪个 tab 上（`null` = 没压在任何一个上）。停留计时器靠它决定「还在不在同一个」。
 *
 * ⚠ 被拖的那一个（`dragged`）要排除：压在自己身上不是一次合并。
 */
export function tabUnderY(
  rects: readonly TabRect[],
  clientY: number,
  dragged: string,
): string | null {
  for (const r of rects) {
    if (r.sid === dragged) continue;
    if (clientY >= r.top && clientY < r.top + r.height) return r.sid;
  }
  return null;
}

/**
 * 算落点。三种语义的**唯一判定处**。
 *
 * @param dwellSid 停留已经攒满的那个 sid（`null` = 还没攒满）。攒满这件事由计时器判，
 *                 不在这里判 —— 但「攒满之后指针有没有还在那个矩形里」在这里**再判一次**：
 *                 计时器与指针是两个来源，只信计时器的话，指针早已划走还会合并成组。
 *
 * 🔴 **必须按视觉序（`top` 升序）扫，不能按 `orderedIds` 扫。**
 *   `§D.2` 拆掉「组里的 tab 不参与」那道过滤之后，`orderedIds` 的次序与屏幕上的次序
 *   **不再一致**（组容器整块排在散 tab 前面，见 `refreshTabBar`）⇒ 按 `orderedIds` 扫会
 *   在第一个「中线在指针下方」的元素上停住，而那个元素可能在屏幕上离指针很远。
 */
export function pickDropTarget(
  rects: readonly TabRect[],
  clientY: number,
  dragged: string,
  dwellSid: string | null,
): DropTarget {
  const sorted = rects.filter((r) => r.sid !== dragged).sort((a, b) => a.top - b.top);
  if (dwellSid !== null && dwellSid !== dragged) {
    const r = sorted.find((x) => x.sid === dwellSid);
    if (r && clientY >= r.top && clientY < r.top + r.height) {
      return { kind: "onto", sid: dwellSid };
    }
  }
  for (const r of sorted) {
    if (clientY < r.top + r.height / 2) return { kind: "before", sid: r.sid };
  }
  return { kind: "end" };
}

/**
 * 两个 cwd 的**共同前缀的目录名**（`§D.6` 默认名规则第 1 条）——
 * 最常见情况：同项目的两个会话。算不出来回 `null`。
 *
 * ⚠ 两种分隔符都认：这个 app 的客户端常在 Windows 上，而会话可能来自 Linux 远端。
 * ⚠ 盘符（`C:`）不算目录名 —— 「C:」当组名是噪声，退回 `组 N` 更诚实。
 */
export function commonDirName(a: string | null, b: string | null): string | null {
  if (!a || !b) return null;
  const seg = (p: string): string[] => p.split(/[/\\]+/).filter((s) => s !== "");
  const sa = seg(a);
  const sb = seg(b);
  const n = Math.min(sa.length, sb.length);
  let i = 0;
  while (i < n && sa[i] === sb[i]) i++;
  if (i === 0) return null;
  const last = sa[i - 1];
  if (/^[A-Za-z]:$/.test(last)) return null;
  return last;
}

/**
 * 拖动合并出来的那个组**叫什么**（`§D.6`）。
 *
 * 🔴 **这条路上不能弹 `window.prompt`**（`真相源/05` I5：原生阻塞弹窗、风格不一致，
 * 且仓里另一套 dialog 插件正在被 ACL 拒）⇒ 必须能算出一个默认名。
 *
 * 优先级：① 两个 cwd 的共同前缀目录名 ② `组 N`（取当前最大编号 +1）。
 * 事后点组头改名（那条路已有，`groupElFor`）。
 */
export function defaultGroupName(
  cwdA: string | null,
  cwdB: string | null,
  existingNames: readonly string[],
): string {
  const dir = commonDirName(cwdA, cwdB);
  if (dir) return dir;
  let max = 0;
  for (const name of existingNames) {
    const m = /^组\s*(\d+)$/.exec(name.trim());
    if (m) max = Math.max(max, Number(m[1]));
  }
  return `组 ${max + 1}`;
}

/**
 * 一次落点对**集合**的全部后果。`§D.7` 那条判据的唯一住址：
 * 「**落点宿主 ≠ 该 tab 当前所属组的容器 ⇒ 视为移出**」。
 *
 * 这里把它写成对称的一句话：**归属跟着落点宿主走**。
 * | 落点 | 宿主 | 后果 |
 * |---|---|---|
 * | `onto X` | X 所在的组；X 还没组 ⇒ 现建一个 | **入组** |
 * | `before X` | X 所在的组（X 是散 tab ⇒ 无宿主）| 入组 / **拖出组** |
 * | `end` | 无宿主（末尾就是散 tab 区）| **拖出组** |
 *
 * 〔BG1 · V125「删掉树」〕归属只跟着被拖的那一个走：原先这里收的是「一块」（交互 tab 连同它的
 *   bg 子串整块入组 / 出组）—— 那是 bg 树借集合开的第二条分类路，与 `设计/30 §1` 不变量 3
 *   「集合归属是唯一分类维」相违，随树一起删。
 * ⚠ 建组失败（到 32 个集合的上界 / 名字空）⇒ **原样返回，什么都不做** ——
 *   不许把 tab 塞进一个不存在的集合（`newCollectionId` 那条路已有同样的守卫）。
 */
export function applyDropToCollections(
  collections: readonly TabCollection[],
  sid: string,
  target: DropTarget,
  newName: string,
  newId: string,
): TabCollection[] {
  let next: TabCollection[] = [...collections];
  let hostId: string | null = null;
  if (target.kind === "onto") {
    if (target.sid === sid) return next; // 压在自己身上不是一次合并
    const existing = collectionOf(next, target.sid);
    if (existing) {
      hostId = existing.id;
    } else {
      // 🔴 **这里刻意没有「建组失败就提前 return」那道守卫** —— 死值验刀 24 实测它恒不承重：
      //   到 32 个集合的上界时 `createCollection` 原样返回，随后 `addMember` 找不到
      //   `newId` 这个集合、也原样返回（`tab-collections.ts` 里那两条各自的守卫），
      //   于是加不加那一行，输出一个字节都不差。
      //   照 `sanitizeCollections` 的逐字先例：任何输入都区分不出的守卫是一条假绿的防线。
      // ⇒ 「到上界就什么都不做」这条性质的住址是 `COLLECTION_CAP`，判据也钉在那儿
      //   （死值验刀 24 改的是那一行，当场红）。
      next = addMember(createCollection(next, newName, newId), newId, target.sid);
      hostId = newId;
    }
  } else if (target.kind === "before") {
    hostId = collectionOf(next, target.sid)?.id ?? null;
  }
  return hostId ? addMember(next, hostId, sid) : removeMember(next, sid);
}

/**
 * 两份集合表是不是同一件事（顺序、id、名字、成员全比）。
 *
 * 只为一件事存在：**拖动是高频动作**，落点没改变归属时不该每拖一下就写一次
 * `config.json`。写盘本身没坏处，但那会把「用户改了分组」这条信号淹掉。
 */
export function collectionsEqual(
  a: readonly TabCollection[],
  b: readonly TabCollection[],
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const x = a[i];
    const y = b[i];
    if (x.id !== y.id || x.name !== y.name) return false;
    if (x.members.length !== y.members.length) return false;
    for (let j = 0; j < x.members.length; j++) {
      if (x.members[j] !== y.members[j]) return false;
    }
  }
  return true;
}
