/**
 * 〔拆 `tabs.ts` ④〕**拖动排序 / 拖动成组的落点算术** —— 全是纯函数，判据直接打在它们身上。
 *
 * 零 DOM、零 IPC：量尺寸（`getBoundingClientRect`）与挂监听是 `tab-bar-drag.ts` 的事，
 * 这里只回答「这几个矩形 ＋ 这个指针位置 ⇒ 落在哪」「这次落点对被拖那个 tab 的组意味着什么」。
 * 原住 `tabs.ts`，逐字搬出；`tabs.ts` 原样 re-export，既有 import 面（`tabs.vitest.ts`）零改动。
 */
import { copyText } from "./copy-table";

/**
 * 把被拖的那个 `sid` 挪到 `beforeSid` 之前。
 * `beforeSid === null` = 挪到末尾。**纯函数** —— 判据直接打在落位上，不必先造一次真拖拽。
 *
 * 落点就是它自己 ⇒ 原样返回（拖到自己身上不是重排）。一次只拖一个。
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

// ===== Edge 式拖动合并成组 =====

/**
 * 落点语义。**从 1 种扩到 3 种**（`§D.3` 逐字）—— 在这之前只有 `before`（与 `null`＝末尾）。
 *
 * ⚠ `onto` 与 `before` 的区别不只是「插哪儿」：`onto` 会**建组 / 入组**，
 *   而落点所在的容器还顺带决定「拖出组」（`§D.7`）。两件事在 [`groupMoveForDrop`] 里合一。
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
 * 按视觉序（`top` 升序）扫，不按 `orderedIds`：组容器整块排在散 tab 前面（见 `refreshTabBar`），两个次序不一致，
 *   按 `orderedIds` 扫会停在一个屏幕上离指针很远的元素上。
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
 * 🔴 **这条路上不能弹 `window.prompt`**（原生阻塞弹窗、风格不一致，
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
    const n = defaultNameNumber(name);
    if (n !== null) max = Math.max(max, n);
  }
  return copyText("tabDrop.group.defaultName", { n: max + 1 });
}

/** 一个组名是不是默认名、编号几：照「组 {n}」那一条文案现取模板认（改了文案照样认得），不在代码里写那个字。 */
function defaultNameNumber(name: string): number | null {
  const mark = "\u0000";
  const [pre, post = ""] = copyText("tabDrop.group.defaultName", { n: mark }).split(mark);
  const esc = (x: string) => x.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const m = new RegExp(`^${esc(pre.trim())}\\s*(\\d+)\\s*${esc(post.trim())}$`).exec(name.trim());
  return m ? Number(m[1]) : null;
}

/**
 * 一次落点对**被拖那个 tab 的组**意味着什么。
 * 组员关系是 tab 自己的属性（`Tab.group`）⇒ 只回「这个 tab 怎么动」。
 */
export type GroupMove =
  | { kind: "stay" } // 归属不变（零写盘：拖动是高频动作，没改归属就不许每拖一下写一次 `config.json`）
  | { kind: "join"; gid: string } // 进一个已有的组
  | { kind: "found"; with: string } // 与落点那个散 tab 现建一个组（两个都进去）
  | { kind: "leave" }; // 移出所在的组，回到散 tab

/**
 * 一次落点对组的全部后果：归属跟着落点宿主走（落点宿主 ≠ 当前所属组的容器 ⇒ 移出）。
 * | 落点 | 宿主 | 后果 |
 * |---|---|---|
 * | `onto X` | X 所在的组；X 还没组 ⇒ 现建一个 | **入组** / **现建** |
 * | `before X` | X 所在的组（X 是散 tab ⇒ 无宿主）| 入组 / **拖出组** |
 * | `end` | 无宿主（末尾就是散 tab 区）| **拖出组** |
 * 宿主就是它现在那个组 ⇒ `stay`（`before` 同组的另一个 · 散 tab 拖到散 tab 之间 · 压在自己身上）。
 *
 * 归属只跟着被拖的那一个走。现建到上界（32 个组）不归这里：`TabBarPrefs.foundGroup` 回拒绝原因，调用方出声。
 *
 * @param groupOf 此刻某个 tab 的组 id（`null` = 散 tab）—— 读 `Tab.group`，由调用方给（本文件零 DOM、零 store）。
 */
export function groupMoveForDrop(
  groupOf: (sid: string) => string | null,
  sid: string,
  target: DropTarget,
): GroupMove {
  if (target.kind === "onto" && target.sid === sid) return { kind: "stay" }; // 压在自己身上不是一次合并
  const cur = groupOf(sid);
  let host: string | null = null;
  if (target.kind === "onto") {
    host = groupOf(target.sid);
    if (host === null) return { kind: "found", with: target.sid };
  } else if (target.kind === "before") {
    host = groupOf(target.sid);
  }
  if (host === cur) return { kind: "stay" };
  return host === null ? { kind: "leave" } : { kind: "join", gid: host };
}
