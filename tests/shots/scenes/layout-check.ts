/**
 * 每张图截之前在页里量一遍 DOM 外接框（所有场景都跑，不用场景自己记得）：
 *
 * 1. 浮层贴着触发物：开着的菜单 / 浮层面板，离它最近的那个「展开着」（`aria-expanded="true"`）的触发物要紧挨着它 ——
 *    竖直方向在触发物下方或上方 4px（±1）、水平方向与触发物那一列有交叠；右键开的（没有触发物）只查在窗口里。
 *    浮层与展开着的子菜单都不许伸出窗口。
 * 2. 单行文字不压按钮：会话头标题 / 机器 / 目录 / 状态一句与右侧按钮不相交；标签页的名字与它行尾的徽标不相交。
 * 3. 折叠块标题行：标题与摘要各一行（窄了摘要省略，标题不被挤成竖排）。
 * 5. 规则编辑器：预览「这条规则」每段要么段里的号名全字可见、要么收起（不许露半截）；封顶表宽时一号一行、各格同一行，
 *    窄（窗 <640）时每号一张卡：表头那一行收起、各格竖排、每格前写着列名。
 * 4. 账号面板轮换列表一行：每段字一行不折（重置时刻那种）；状态标签（在用 · 起始 …）全字可见、不被省略
 *    （号名可以省略，只读的封顶标签可以省略）；兜底开关开着 ⇒ 看得见、有底色；关着 ⇒ 所在行没悬停、没焦点时看不见。
 *
 * 回一串「哪里不对」；空 ＝ 都对。
 */

const EPS = 1;
type R = DOMRect;

const visible = (el: Element): boolean => {
  const r = el.getBoundingClientRect();
  if (r.width === 0 || r.height === 0) return false;
  const cs = getComputedStyle(el);
  return cs.display !== "none" && cs.visibility !== "hidden";
};
const intersects = (a: R, b: R): boolean => a.left < b.right - EPS && b.left < a.right - EPS && a.top < b.bottom - EPS && b.top < a.bottom - EPS;
const name = (el: Element): string => {
  const label = el.getAttribute("aria-label") ?? el.textContent?.trim().slice(0, 16) ?? "";
  return `${el.tagName.toLowerCase()}${el.className && typeof el.className === "string" ? "." + el.className.split(" ")[0] : ""}「${label}」`;
};
const fmt = (r: R): string => `[${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}×${Math.round(r.height)}]`;

function inView(r: R): boolean {
  return r.left >= -EPS && r.top >= -EPS && r.right <= innerWidth + EPS && r.bottom <= innerHeight + EPS;
}

function floats(): string[] {
  const out: string[] = [];
  // 由 `place.ts` 摆的那几种（写了内联 left）：菜单 · 浮层面板 · ↗ 的「查找终端…」；抽屉 / 对话框不算。
  const all = [...document.querySelectorAll<HTMLElement>('body > [role="menu"], body > div[role="dialog"]:not([aria-modal="true"]), body > [data-role="front-busy"]')].filter(
    (f) => f.style.left !== "" && getComputedStyle(f).position === "fixed" && visible(f),
  );
  const anchors = [...document.querySelectorAll<HTMLElement>('[aria-expanded="true"]')].filter((a) => visible(a) && !all.some((f) => f.contains(a)));
  for (const f of all) {
    const fr = f.getBoundingClientRect();
    if (!inView(fr)) out.push(`浮层 ${name(f)} ${fmt(fr)} 伸出窗口`);
    for (const sub of f.querySelectorAll<HTMLElement>('[data-sub-open="true"] > [role="menu"]')) {
      const sr = sub.getBoundingClientRect();
      if (!inView(sr)) out.push(`子菜单 ${fmt(sr)} 伸出窗口`);
    }
    if (anchors.length === 0) continue; // 右键开的：锚在一点
    const dist = (a: HTMLElement): number => {
      const ar = a.getBoundingClientRect();
      const dx = Math.max(0, ar.left - fr.right, fr.left - ar.right);
      const dy = Math.max(0, ar.top - fr.bottom, fr.top - ar.bottom);
      return Math.hypot(dx, dy);
    };
    const a = anchors.reduce((p, c) => (dist(c) < dist(p) ? c : p));
    const ar = a.getBoundingClientRect();
    const below = Math.abs(fr.top - (ar.bottom + 4)) <= EPS;
    const above = Math.abs(fr.bottom - (ar.top + -4)) <= EPS;
    const column = fr.left <= ar.right + EPS && fr.right >= ar.left - EPS;
    if (!(below || above) || !column) out.push(`浮层 ${name(f)} ${fmt(fr)} 不贴着触发物 ${name(a)} ${fmt(ar)}`);
  }
  return out;
}

function noOverlap(group: string, texts: Element[], buttons: Element[]): string[] {
  const out: string[] = [];
  for (const t of texts.filter(visible)) {
    const tr = t.getBoundingClientRect();
    for (const b of buttons.filter(visible)) {
      if (intersects(tr, b.getBoundingClientRect())) out.push(`${group}：${name(t)} ${fmt(tr)} 压住 ${name(b)} ${fmt(b.getBoundingClientRect())}`);
    }
  }
  return out;
}

function singleLines(): string[] {
  const out: string[] = [];
  const head = document.getElementById("session-head");
  if (head && visible(head)) {
    const kids = [...head.children];
    const sp = kids.findIndex((k) => getComputedStyle(k).flexGrow === "1");
    const texts = kids.slice(0, sp < 0 ? kids.length : sp).filter((k) => k.tagName === "SPAN" && k.childElementCount === 0);
    const buttons = [...head.querySelectorAll("button")];
    out.push(...noOverlap("会话头", texts, buttons));
    const hr = head.getBoundingClientRect();
    for (const b of buttons.filter(visible)) if (b.getBoundingClientRect().right > hr.right + EPS) out.push(`会话头：${name(b)} 被挤出右边`);
  }
  for (const row of document.querySelectorAll<HTMLElement>("#tab-bar .tab")) {
    if (!visible(row)) continue;
    const texts = [...row.querySelectorAll(".tab-title")];
    const badges = [...row.querySelectorAll(".tab-trail > *, .tab-machine")];
    out.push(...noOverlap("标签页", texts, badges));
    const rr = row.getBoundingClientRect();
    for (const b of badges.filter(visible)) if (b.getBoundingClientRect().right > rr.right + EPS) out.push(`标签页：${name(b)} 被挤出行尾`);
  }
  return out;
}

function acctRows(): string[] {
  const out: string[] = [];
  for (const row of document.querySelectorAll<HTMLElement>("[data-acct-row]")) {
    if (!visible(row)) continue;
    const who = row.dataset.acctRow;
    for (const leaf of row.querySelectorAll<HTMLElement>("*")) {
      if (leaf.childElementCount > 0 || !leaf.textContent?.trim() || !visible(leaf)) continue;
      const n = lineCount(leaf);
      if (n > 1) out.push(`轮换列表 ${who}：${name(leaf)} 折成了 ${n} 行`);
    }
    for (const tag of row.querySelectorAll<HTMLElement>("[data-acct-tag]:not([data-cap])")) {
      if (visible(tag) && tag.scrollWidth > tag.clientWidth + EPS) out.push(`轮换列表 ${who}：标签 ${name(tag)} 被截（${tag.scrollWidth} > ${tag.clientWidth}）`);
    }
    for (const fb of row.querySelectorAll<HTMLElement>("[data-rot-fallback]")) {
      const cs = getComputedStyle(fb);
      const on = fb.getAttribute("aria-pressed") === "true";
      const shown = Number(cs.opacity) > 0.5 && fb.getBoundingClientRect().width > 0;
      const lit = fb.parentElement?.matches(":hover, :focus-within") ?? false;
      if (on && (!shown || cs.backgroundColor === "rgba(0, 0, 0, 0)")) out.push(`轮换列表 ${who}：兜底开着却不是实心标`);
      if (!on && !lit && Number(cs.opacity) > 0 && fb.getBoundingClientRect().width > 0) out.push(`轮换列表 ${who}：兜底关着、行没悬停没焦点却看得见`);
      if (!on && lit && !shown) out.push(`轮换列表 ${who}：兜底关着、行悬停 / 有焦点却没出来`);
    }
  }
  return out;
}

function ruleEditor(): string[] {
  const out: string[] = [];
  for (const n of document.querySelectorAll<HTMLElement>('[data-ed-lane="rule"] [data-ed-seg-name]')) {
    if (!visible(n)) continue;
    const seg = n.parentElement!;
    if (n.scrollWidth > n.clientWidth + EPS) out.push(`预览：段 ${seg.dataset.edSeg} 的号名被截（${n.scrollWidth} > ${n.clientWidth}）`);
  }
  for (const seg of document.querySelectorAll<HTMLElement>('[data-ed-lane="rule"] [data-ed-seg]')) {
    if (!visible(seg)) continue;
    const n = seg.querySelector<HTMLElement>("[data-ed-seg-name]");
    if (!n || (!visible(n) && !seg.getAttribute("aria-label")?.includes(n.textContent ?? "\u0000"))) out.push(`预览：段 ${seg.dataset.edSeg} 读不出号名`);
  }
  const narrow = innerWidth < 640;
  for (const row of document.querySelectorAll<HTMLElement>("[data-ed-cap-row]")) {
    if (!visible(row)) continue;
    const who = row.dataset.edCapRow;
    const cells = [...row.querySelectorAll<HTMLElement>("[data-ed-cap], [data-ed-stint-cell]")].filter(visible);
    const tops = new Set(cells.map((c) => Math.round(c.getBoundingClientRect().top)));
    const lefts = new Set(cells.map((c) => Math.round(c.getBoundingClientRect().left)));
    const labels = [...row.querySelectorAll<HTMLElement>("[data-ed-cap-label]")].filter(visible);
    if (narrow) {
      if (tops.size !== cells.length || lefts.size !== 1) out.push(`封顶表 ${who}：窄窗里各格没有竖排（${cells.length} 格 · ${tops.size} 行 · ${lefts.size} 列）`);
      if (labels.length !== cells.length) out.push(`封顶表 ${who}：窄窗里 ${cells.length} 格只有 ${labels.length} 个列名`);
    } else if (tops.size > 1) out.push(`封顶表 ${who}：各格折成了 ${tops.size} 行`);
  }
  if (narrow)
    for (const head of document.querySelectorAll<HTMLElement>('[data-ed-caps] [role="row"]:not([data-ed-cap-row])'))
      if (visible(head)) out.push("封顶表：窄窗里表头那一行还在（应收进每张卡）");
  return out;
}

/** 一个元素里的字占了几行（数不同的行顶；省略号会把一行拆成几个框，不算折行）。 */
function lineCount(el: Element): number {
  const rg = document.createRange();
  rg.selectNodeContents(el);
  return new Set([...rg.getClientRects()].filter((r) => r.width > 0).map((r) => Math.round(r.top))).size;
}

function foldHeads(): string[] {
  const out: string[] = [];
  for (const head of document.querySelectorAll<HTMLElement>("[data-fold-head]")) {
    if (!visible(head)) continue;
    for (const part of [...head.children].filter((c) => c.tagName === "SPAN" && visible(c))) {
      const n = lineCount(part);
      if (n > 1) out.push(`折叠块：${name(part)} 折成了 ${n} 行`);
    }
  }
  return out;
}

export function layoutProblems(): string[] {
  const out = [...floats(), ...singleLines(), ...foldHeads(), ...acctRows(), ...ruleEditor()];
  // 也写进页里的控制台（截图工具每张存一份），整趟被打断时还查得到。
  for (const p of out) console.warn(`[layout] ${p}`);
  return out;
}
