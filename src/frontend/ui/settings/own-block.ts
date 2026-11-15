/**
 * 「cc-monitor 放了什么 → cc-monitor 的文件」那一块：照那台后端 `data-report` 的 `own` 排（它自己家里的每一样；本机那一台连同界面自己那几样），
 * 本机远端同一套行（名字 · 是什么 · 删了会怎样 · 大小 · 打开）；打开那一格：本机多一颗系统打开，「在文件窗口里打开」本机远端一样。
 * 每一格照后端那一份画，界面不判、不另拼句子。只给位置、不给删。
 */
import { copyText } from "../copy-table";
import { dataClassCell } from "./data-section";
import type { OwnItem } from "./data-reads";

/**
 * 那台家里那几样排成一块。`actions` 给那一样打开那一格的按钮（拿后端写好的绝对路径 `abs`）；`title` 有 ⇒ 块头带标题。
 */
export function ownBlock(
  items: readonly OwnItem[],
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
    label.textContent = o.label;
    const desc = document.createElement("span");
    desc.className = "settings-data-item-desc";
    desc.textContent = o.what;
    const meta = document.createElement("span");
    meta.className = "settings-data-item-meta";
    // 不在盘上那一句是那台后端写好的（`absent_said`，在盘上 ⇒ null），照抄。
    meta.textContent = o.absent_said ?? o.size_text ?? copyText("data.item.created");
    const acts = document.createElement("span");
    acts.className = "settings-data-item-actions";
    acts.append(...actions(o, o.abs));
    const path = document.createElement("div");
    path.className = "settings-data-item-path";
    path.textContent = o.path;
    path.title = o.abs;
    li.append(label, desc, dataClassCell(o.class), meta, acts, path);
    list.appendChild(li);
  }
  block.appendChild(list);
  return block;
}

/** 那台够不够得着：连着 ⇒ 点了怎么开；断开（画的是上次的）⇒ 本机后端给的那台没问到的那一句（`chores-merge` 的 `said`）。 */
export type Reach = { open: (abs: string) => void } | { offline: string };

/**
 * 那一样的［在文件窗口里打开］，本机远端同一颗：断开 ⇒ 置灰、提示是那台没问到的那一句；不在盘上 ⇒ 置灰、提示是后端写的 `absent_said`；
 * 别的 ⇒ 点了照 `reach.open` 开。界面不另取字。
 */
function revealButton(o: OwnItem, abs: string, reach: Reach): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "settings-data-item-open";
  b.dataset.reveal = "";
  b.textContent = copyText("data.item.reveal");
  const why = "offline" in reach ? reach.offline : o.exists ? null : (o.absent_said ?? "");
  b.disabled = why !== null;
  if (why !== null) b.title = why;
  else if ("open" in reach) b.addEventListener("click", () => reach.open(abs));
  return b;
}

/** 打开那一格，本机远端同一份：［在文件窗口里打开］；`system` 有（本机那台）⇒ 前面多一颗系统打开。 */
export function openCell(reach: Reach, system?: (o: OwnItem, abs: string) => HTMLElement): (o: OwnItem, abs: string) => HTMLElement[] {
  return (o, abs) => [...(system ? [system(o, abs)] : []), revealButton(o, abs, reach)];
}
