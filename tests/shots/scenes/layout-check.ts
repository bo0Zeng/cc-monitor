/**
 * 每张图截之前在页里量一遍 DOM 外接框（所有场景都跑，不用场景自己记得）：
 *
 * 1. 浮层贴着触发物：开着的菜单 / 浮层面板，离它最近的那个「展开着」（`aria-expanded="true"`）的触发物要紧挨着它 ——
 *    竖直方向在触发物下方或上方 4px（±1）、水平方向与触发物那一列有交叠；右键开的（没有触发物）只查在窗口里。
 *    浮层与展开着的子菜单都不许伸出窗口。
 * 2. 单行文字不压按钮：会话头标题 / 机器 / 目录 / 状态一句与右侧按钮不相交；标签页的名字与它行尾的徽标不相交。
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

export function layoutProblems(): string[] {
  const out = [...floats(), ...singleLines()];
  // 也写进页里的控制台（截图工具每张存一份），整趟被打断时还查得到。
  for (const p of out) console.warn(`[layout] ${p}`);
  return out;
}
