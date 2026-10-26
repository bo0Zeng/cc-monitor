/**
 * 会话消息里提到的格：编号原样留在正文里，后面紧跟一枚淡色标题（点了开计划页那一格）。
 *
 * 认编号、拼标题都是后端的事（帧面 `plan-refs`：认片 ＋ 交 pb 的 `dump --refs`）；这里只做三样：
 * - 记原文：建回复正文块 · 用户气泡时 [`noteProse`] 记下那一块照哪段原文、哪一种排法渲的（思考 · 工具输入输出 · 折起的粘贴块不记 ⇒ 不送）；
 * - 攒一批：卡片挂进流时 [`askRefs`] 把卡里记过原文的块按「哪台 · 哪个会话」排进队，同一趟挂进来的只问一次
 *   （补历史几百张卡也是一问；后端按段缓存，再问全中不起 pb）；
 * - 照排：回来有处的那一块照占位法重渲 —— 每处 `end` 后面先插一个占位、整段再过一遍 markdown、占位换成标题按钮；回空 ⇒ 那一块一个字不动。
 * 下标是 UTF-16（后端从 pb 的码位换好的），直接按 JS 字符串切。问没成 ⇒ 原样（不报错：带标题只是锦上添花）。
 */

import { emit } from "@tauri-apps/api/event";
import { fetchMsgRefs } from "./plan-reads";
import { renderMarkdown } from "./render";
import { PLAN_OPEN_EVENT, type PlanOpenSaid } from "./window-events";
import type { PlanMsgRef } from "./generated/PlanMsgRef";
import s from "./plan-refs.module.css";

/** 那一块怎么排原文：回复正文过 markdown，人说的话照原文（`renderPlainText`）。 */
type Render = (text: string) => string;

/** 每块正文照哪段原文、哪一种排法渲的。 */
const prose = new WeakMap<HTMLElement, { text: string; render: Render }>();

/** 记下这一块是照 `text` 用 `render` 渲的（它就成了要问的那一种块；重排时用同一种排法）。 */
export function noteProse(el: HTMLElement, text: string, render: Render = renderMarkdown): void {
  prose.set(el, { text, render });
}

/** 记过原文的块可能在的地方：回复正文块（长回复是它的第一片）· 用户气泡正文（带粘贴块时是露在外面的那几段）。 */
const PROSE_AT = ".block-text, .block-text > div, .card-user > .card-body, .card-user > .card-body > span";

interface Waiting {
  origin: string;
  sid: string;
  blocks: HTMLElement[];
}

/** 排着队、这一帧要问的（键 ＝ 哪台 ＋ 哪个会话）。 */
const queue = new Map<string, Waiting>();
let scheduled = false;
/** 在路上的几问（[`__refsSettledForTests`] 等它们）。 */
let inflight: Promise<void>[] = [];

/** 一张卡里记过原文的块排进这一帧要问的那一批（没有 ⇒ 什么都不做）。 */
export function askRefs(card: HTMLElement, origin: string, sid: string): void {
  const blocks = [...card.querySelectorAll<HTMLElement>(PROSE_AT)].filter((b) => prose.has(b));
  if (blocks.length === 0) return;
  const key = `${origin}\u0000${sid}`;
  const w = queue.get(key) ?? { origin, sid, blocks: [] };
  w.blocks.push(...blocks);
  queue.set(key, w);
  if (scheduled) return;
  scheduled = true;
  // 调度：合批 —— 这一趟同步挂进来的卡排到它之后，一台一个会话问一次
  setTimeout(() => {
    scheduled = false;
    const batch = [...queue.values()];
    queue.clear();
    for (const one of batch) {
      const p: Promise<void> = ask(one).finally(() => {
        inflight = inflight.filter((x) => x !== p);
      });
      inflight.push(p);
    }
  }, 0);
}

async function ask(w: Waiting): Promise<void> {
  const noted = w.blocks.map((b) => prose.get(b) ?? { text: "", render: renderMarkdown });
  const texts = noted.map((n) => n.text);
  let got;
  try {
    got = await fetchMsgRefs(w.origin, w.sid, texts);
  } catch (e) {
    console.debug("[plan-refs] 没问成：", e);
    return;
  }
  const { workspace, slice } = got;
  if (workspace === null || slice === null) return;
  w.blocks.forEach((b, i) => {
    const refs = got.refs[i] ?? [];
    if (refs.length > 0) fill(b, noted[i], refs, { origin: w.origin, workspace, slice });
  });
}

/** 判据用：等排着的与在路上的都落地。 */
export async function __refsSettledForTests(): Promise<void> {
  for (let k = 0; k < 3 && (scheduled || inflight.length > 0); k++) {
    // 调度：一次性 —— 让排到帧末的那一问先起，再等在路上的落地（至多三轮）
    await new Promise((r) => setTimeout(r, 20));
    await Promise.all(inflight);
  }
}

const MARK_OPEN = String.fromCharCode(0xe000);
const MARK_CLOSE = String.fromCharCode(0xe001);
const MARK_SPLIT = new RegExp(`${MARK_OPEN}(\\d+)${MARK_CLOSE}`);

/** 重渲那一块：每处 `end` 后面插占位，照记下的排法再排一遍，占位换成标题按钮。越界 · 叠着的不算（照原文）。 */
function fill(
  el: HTMLElement,
  { text, render }: { text: string; render: Render },
  refs: readonly PlanMsgRef[],
  at: Omit<PlanOpenSaid, "id">,
): void {
  const kept: PlanMsgRef[] = [];
  let md = "";
  let from = 0;
  for (const r of [...refs].sort((a, b) => a.start - b.start)) {
    if (r.start < from || r.end > text.length || r.start >= r.end || r.ids.length === 0) continue;
    md += `${text.slice(from, r.end)}${MARK_OPEN}${kept.length}${MARK_CLOSE}`;
    kept.push(r);
    from = r.end;
  }
  if (kept.length === 0) return;
  md += text.slice(from);
  el.innerHTML = render(md);
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  const hits: Text[] = [];
  while (walker.nextNode()) if ((walker.currentNode.nodeValue ?? "").includes(MARK_OPEN)) hits.push(walker.currentNode as Text);
  for (const t of hits) {
    const frag = document.createDocumentFragment();
    (t.nodeValue ?? "").split(MARK_SPLIT).forEach((p, j) => {
      if (j % 2 === 0) {
        if (p) frag.appendChild(document.createTextNode(p));
        return;
      }
      const r = kept[Number(p)];
      if (r) frag.appendChild(tail(r, at));
    });
    t.replaceWith(frag);
  }
}

function tail(r: PlanMsgRef, at: Omit<PlanOpenSaid, "id">): HTMLElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = s.prTail;
  // 截断了悬停看全由 kit/tooltip 给（单行件），这里不另写 title。
  b.textContent = r.label;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    const said: PlanOpenSaid = { ...at, id: r.ids[0] };
    void emit(PLAN_OPEN_EVENT, said).catch((x: unknown) => console.warn("[plan-refs] 交不给主窗口：", x));
  });
  return b;
}
