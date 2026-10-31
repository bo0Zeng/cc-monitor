/**
 * 写类工具（卡型 `diff`）展开的行级 diff：行 · 种类 · 收着画几行 · 增删数都是后端算好的（`LineDiff`：调用那一份
 * `steps[id].body.diff`，结果那一份 `results[id].patch`），这里只照行画。
 *
 * 仅 createElement + textContent，绕开 DOMPurify，三引号 / HTML / CJK / emoji 按字面渲染。
 */

import { copyText } from "../copy-table";
import type { LineDiffKind } from "../generated/LineDiffKind";
import type { LineDiffRow } from "../generated/LineDiffRow";
import type { LineDiff } from "../generated/LineDiff";

function gutterMark(kind: LineDiffKind): string {
  if (kind === "add") return "+";
  if (kind === "del") return "-";
  return "";
}

function rowEl(row: LineDiffRow): HTMLElement {
  if (row.kind === "head") {
    const label = document.createElement("div");
    label.className = "block-diff-edit-label";
    label.textContent = row.text;
    return label;
  }
  const line = document.createElement("div");
  line.className = `block-diff-line block-diff-${row.kind}`;
  const gutter = document.createElement("span");
  gutter.className = "block-diff-gutter";
  gutter.textContent = gutterMark(row.kind);
  const code = document.createElement("span");
  code.className = "block-diff-code";
  // textContent 自动转义：三引号 / HTML / 尖括号 / CJK / emoji 一律按字面安全渲染。
  code.textContent = row.text;
  line.append(gutter, code);
  return line;
}

/**
 * 行差的展开那一格。后端说没逐行比（`notice`）⇒ 顶上先摆那一句。收着先画后端给的前 `shown` 行；行比它多 ⇒ 附「显示全部改动 +N −M」，
 * 点一次补上其余。一行也没有 ⇒ `null`，调用方回退入参原文 `<pre>`。
 */
export function buildDiffBody(diff: LineDiff): HTMLElement | null {
  if (diff.rows.length === 0) return null;
  const root = document.createElement("div");
  root.className = "block-body block-args block-diff";
  if (diff.notice) {
    const notice = document.createElement("div");
    notice.className = "block-diff-edit-label";
    notice.textContent = diff.notice;
    root.appendChild(notice);
  }
  for (const row of diff.rows.slice(0, diff.shown)) root.appendChild(rowEl(row));
  if (diff.rows.length > diff.shown) {
    const btn = document.createElement("button");
    btn.className = "block-body-show-full";
    btn.textContent = copyText("diff.buildDiffBody.showAll", { addCount: diff.added, delCount: diff.removed });
    btn.addEventListener(
      "click",
      () => {
        btn.replaceWith(...diff.rows.slice(diff.shown).map(rowEl));
      },
      { once: true },
    );
    root.appendChild(btn);
  }
  return root;
}
