/**
 * 「cc-monitor 放了什么 → cc-monitor 的文件」那台后端家里那一块：照那台后端 `data-report` 的 `own`（它自己家里的每一样）排，
 * 本机远端同一套行（名字 · 是什么 · 删了会怎样 · 大小 · 打开）；只有打开那一格各走各的（本机 ＝ 系统打开 · 在文件夹中显示，远端 ＝ 文件窗口）。
 * 每一格照后端那一份画，界面不判、不另拼句子。只给位置、不给删。
 */
import { copyText } from "../copy-table";
import { sizeText } from "../format";
import { dataClassCell } from "./data-section";
import type { OwnItem } from "./data-reads";

/** `~/.cc-monitor/x` ⇒ 显示名（去掉家那一段；目录带尾 `/`）。 */
function labelOf(o: OwnItem): string {
  const tail = o.path.replace(/^~\/\.cc-monitor\/?/, "");
  return o.dir ? `${tail}/` : tail;
}

/**
 * 那台家里那几样排成一块。`actions` 给那一样打开那一格的按钮（拿家目录已展开的绝对路径）；`title` 有 ⇒ 块头带标题。
 */
export function ownBlock(
  items: readonly OwnItem[],
  home: string,
  actions: (o: OwnItem, abs: string) => HTMLElement[],
  title?: string,
): HTMLElement {
  const block = document.createElement("div");
  block.className = "settings-data-block";
  const head = document.createElement("div");
  head.className = "settings-data-block-head";
  if (title) {
    const t = document.createElement("span");
    t.className = "settings-data-block-title";
    t.textContent = title;
    head.appendChild(t);
  }
  const sub = document.createElement("span");
  sub.className = "settings-data-block-subtitle";
  sub.textContent = "~/.cc-monitor";
  head.appendChild(sub);
  block.appendChild(head);
  const list = document.createElement("ul");
  list.className = "settings-data-list";
  for (const o of items) {
    const li = document.createElement("li");
    li.className = `settings-data-item ${o.exists ? "exists" : "absent"}`;
    li.dataset.kind = o.dir ? "dir" : "file";
    li.dataset.class = o.class;
    li.dataset.own = o.id;
    const label = document.createElement("span");
    label.className = "settings-data-item-label";
    label.textContent = labelOf(o);
    const desc = document.createElement("span");
    desc.className = "settings-data-item-desc";
    desc.textContent = o.what;
    const meta = document.createElement("span");
    meta.className = "settings-data-item-meta";
    meta.textContent = !o.exists ? copyText("data.item.notCreated") : o.size !== null ? sizeText(o.size) : copyText("data.item.created");
    const abs = o.path.replace(/^~/, home);
    const acts = document.createElement("span");
    acts.className = "settings-data-item-actions";
    acts.append(...actions(o, abs));
    const path = document.createElement("div");
    path.className = "settings-data-item-path";
    path.textContent = o.path;
    path.title = abs;
    li.append(label, desc, dataClassCell(o.class), meta, acts, path);
    list.appendChild(li);
  }
  block.appendChild(list);
  return block;
}

/** 远端那台：［在文件窗口打开］；`open` 为 `null` ＝ 那台断开（画的是上次的），置灰。 */
export function remoteOpen(open: ((abs: string) => void) | null): (o: OwnItem, abs: string) => HTMLElement[] {
  return (o, abs) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "settings-data-item-open";
    b.textContent = copyText("dataPage.own.openRemote");
    b.disabled = !o.exists || open === null;
    if (open === null) b.title = copyText("acctPage.offline.hover");
    if (o.exists && open !== null) b.addEventListener("click", () => open(abs));
    return [b];
  };
}
