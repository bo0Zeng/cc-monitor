/**
 * 「要你动手」里的一件（`机器配置-v2.md` §3）：收着时一行（状态点 · 名字 · 类 / 态的小标签 · 下一行 位置小标签 ＋ 一句现状 · 主按钮 · 展开钮），
 * 点开依次 为什么 · 怎么做（编号步骤）· diff（上下各一行原文，删红加绿，钥匙遮住）· 按钮（复制这几行 · 复制改好的整份文件 · 打开 · 不用了）·
 * 一句「存盘后这里自己认出」＋［再查一遍］。
 * 每一格都照那台后端的成品画（类 · 态 · 字 · 行号 · diff · 要复制的原文），界面只多记一个「已复制」。
 */
import { button } from "../kit/button";
import { foldCaret } from "../kit/fold";
import { tag } from "../kit/badge";
import { copyText } from "../copy-table";
import type { Chore, ChoreKind } from "./data-reads";

export interface ChoreRowHost {
  open: boolean;
  copied: boolean;
  onToggle: (open: boolean) => void;
  /** 复制了 `ids` 那几件的东西（整份文件可能盖住两件）。 */
  onCopied: (text: string, ids: string[]) => void;
  /** 主按钮里不是复制的那几样（去定… · 安装方法 · 先装 cc-bus）。 */
  onGo: (c: Chore) => void;
  onOpenFile: ((path: string) => void) | null;
  onDecline: (c: Chore, decline: boolean) => void;
  onRecheck: () => void;
}

const KIND_LABEL: Record<ChoreKind, () => string> = {
  must: () => copyText("dataPage.kind.must"),
  install: () => copyText("dataPage.install.kind"),
  decide: () => copyText("dataPage.kind.decide"),
  installOptional: () => copyText("dataPage.install.kindOptional"),
  optional: () => copyText("dataPage.kind.optional"),
};

/** 状态点的色：要做 / 过期 红 · 要你定 / 要装 琥珀 · 可选 空心 · 已做 绿。 */
function tone(c: Chore): string {
  if (c.state === "done") return "ok";
  if (c.state === "expired" || c.kind === "must") return "bad";
  if (c.kind === "decide" || c.kind === "install") return "warn";
  return "muted";
}

/** 遮钥匙用的圆点（U+2022）。 */
const MASK_DOT = String.fromCharCode(0x2022);

/** 显示时把钥匙遮住（复制的照旧是真内容）。 */
export function masked(text: string, mask: string | null): string {
  return mask ? text.split(mask).join(MASK_DOT.repeat(8)) : text;
}

function el<K extends keyof HTMLElementTagNameMap>(tagName: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tagName);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function stateTag(c: Chore, copied: boolean): HTMLElement {
  const t =
    c.state === "expired"
      ? tag(copyText("dataPage.state.expired"))
      : c.state === "blocked"
        ? tag(copyText("dataPage.state.blocked"))
        : copied && c.state === "todo"
          ? tag(copyText("dataPage.state.copied"))
          : tag(KIND_LABEL[c.kind]());
  t.dataset.kind = c.state === "expired" ? "must" : c.kind;
  return t;
}

function mainButton(c: Chore, host: ChoreRowHost): HTMLButtonElement | null {
  if (c.state === "done" || c.state === "declined") return null;
  const copy = (label: string, primary: boolean): HTMLButtonElement | null =>
    c.copy === null ? null : button({ label, size: "compact", kind: primary ? "primary" : "secondary", onClick: () => host.onCopied(c.copy ?? "", [c.id]) });
  switch (c.action) {
    case "copyCommand":
      return copy(copyText("dataPage.chore.copyCommand"), true);
    case "copySnippet":
      return c.state === "blocked" ? null : copy(copyText("dataPage.chore.copySnippet"), c.kind === "must");
    case "locate":
      return button({ label: copyText("dataPage.chore.locate"), size: "compact", onClick: () => host.onToggle(!host.open) });
    case "decide":
      return button({ label: copyText("dataPage.chore.decide"), size: "compact", onClick: () => host.onGo(c) });
    case "how":
      return c.howUrl ? button({ label: copyText("dataPage.install.how"), size: "compact", onClick: () => host.onGo(c) }) : null;
    case "installFirst":
      return button({ label: copyText("dataPage.chore.installFirst"), size: "compact", onClick: () => host.onGo(c) });
  }
}

/** 一件。 */
export function choreRow(c: Chore, host: ChoreRowHost): HTMLElement {
  const row = el("div", "data-item chore-item");
  row.dataset.chore = c.id;
  row.dataset.state = c.state;
  const top = el("div", "chore-top");
  const dot = el("span", "data-dot");
  dot.dataset.tone = tone(c);
  const body = el("div", "data-item-body");
  const title = el("div", "data-item-title", c.name);
  title.appendChild(stateTag(c, host.copied));
  const sub = el("div", "data-item-sub");
  if (c.loc) sub.appendChild(el("code", "data-loc", c.loc));
  sub.appendChild(el("span", "", c.said));
  body.append(title, sub);
  top.append(dot, body);
  const main = mainButton(c, host);
  if (main) top.appendChild(main);
  if (c.state === "declined") top.appendChild(button({ label: copyText("dataPage.chore.undecline"), size: "compact", onClick: () => host.onDecline(c, false) }));
  const hasDetail = c.why !== "" || c.steps.length > 0 || c.diff.length > 0;
  if (hasDetail && c.state !== "declined") {
    const caret = button({ label: host.open ? copyText("dataPage.chore.collapse") : copyText("dataPage.chore.expand"), kind: "icon", size: "compact", onClick: () => host.onToggle(!host.open) });
    caret.prepend(foldCaret());
    caret.setAttribute("aria-expanded", String(host.open));
    top.appendChild(caret);
  }
  row.appendChild(top);
  if (host.open && hasDetail) row.appendChild(detail(c, host));
  return row;
}

function detail(c: Chore, host: ChoreRowHost): HTMLElement {
  const box = el("div", "chore-detail");
  if (c.why) box.appendChild(el("div", "chore-why", c.why));
  if (c.steps.length) {
    const ol = el("ol", "chore-steps");
    for (const s of c.steps) ol.appendChild(el("li", "", masked(s, c.mask)));
    box.appendChild(ol);
  }
  if (c.diff.length) {
    const pre = el("div", "chore-diff");
    pre.setAttribute("role", "group");
    pre.setAttribute("aria-label", copyText("dataPage.chore.diff"));
    for (const d of c.diff) {
      const line = el("div", "chore-diff-line");
      line.dataset.op = d.op;
      line.append(el("span", "chore-diff-n", d.n === null ? "+" : String(d.n)), el("span", "chore-diff-text", masked(d.text, c.mask)));
      pre.appendChild(line);
    }
    box.appendChild(pre);
  }
  if (c.mask) box.appendChild(el("div", "settings-hint", copyText("dataPage.chore.maskHint")));
  const acts = el("div", "chore-acts");
  if (c.state !== "done" && c.state !== "blocked") {
    if (c.copy !== null && c.action !== "copyCommand") acts.appendChild(button({ label: c.action === "locate" || c.action === "decide" ? copyText("dataPage.chore.copyPosition") : copyText("dataPage.chore.copyLines"), size: "compact", kind: "primary", onClick: () => host.onCopied(c.copy ?? "", [c.id]) }));
    if (c.whole !== null) {
      const label = c.wholeCovers.length > 1 ? copyText("dataPage.chore.copyWholeN", { n: c.wholeCovers.length }) : copyText("dataPage.chore.copyWhole");
      acts.appendChild(button({ label, size: "compact", onClick: () => host.onCopied(c.whole ?? "", c.wholeCovers) }));
    }
  }
  if (c.file && host.onOpenFile) {
    const file = c.file;
    const open = host.onOpenFile;
    acts.appendChild(button({ label: copyText("dataPage.chore.openFile"), size: "compact", kind: "ghost", onClick: () => open(file) }));
  }
  if (c.kind === "optional" && c.state === "todo") acts.appendChild(button({ label: copyText("dataPage.chore.decline"), size: "compact", kind: "ghost", onClick: () => host.onDecline(c, true) }));
  if (acts.childElementCount) box.appendChild(acts);
  if (c.state !== "done") {
    const foot = el("div", "chore-foot");
    foot.append(el("span", "settings-hint", copyText("dataPage.chore.autoDetect")), button({ label: copyText("dataPage.chore.recheck"), size: "compact", kind: "ghost", onClick: () => host.onRecheck() }));
    box.appendChild(foot);
  }
  return box;
}
