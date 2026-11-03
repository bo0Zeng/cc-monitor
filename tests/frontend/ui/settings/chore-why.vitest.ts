/**
 * 「待办」里一件点开之后的「为什么」那一句：后端写好的说明可以分行（`\n`），那一格照分行画。
 * 门禁里没有排版引擎：这里钉住那一句落在哪一格、那一格的样式让换行成立；真引擎里的样子看截图台架（settings-data-chore-open）。
 */
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";
import { choreRow } from "../../../../src/frontend/ui/settings/chore-row";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { Chore } from "../../../../src/frontend/ui/settings/data-reads";

const CSS = resolve(__dirname, "../../../../src/frontend/ui/styles/settings.css");

/** settings.css 里选择器恰好是 `sel` 的规则里各声明的最后取值。 */
function declsOf(sel: string): Map<string, string> {
  const out = new Map<string, string>();
  postcss.parse(readFileSync(CSS, "utf8")).walkRules((r: Rule) => {
    if (!r.selectors.map((x) => x.trim()).includes(sel)) return;
    r.walkDecls((d) => void out.set(d.prop, d.value));
  });
  return out;
}

describe("待办 · 为什么那一句", () => {
  it("★ 后端那句带 \\n（接中转那一件）⇒ 落进的那一格保留换行（white-space: pre-line）", () => {
    const why = copyText("beChore.relay.why");
    expect(why).toContain("\n");
    const c = { id: "relay", kind: "optional", state: "todo", name: "名字", loc: "", said: "现状", why, steps: [], diff: null, copy: null, whole: null, wholeCovers: [], file: null, go: null, howUrl: null, action: "copySnippet" } as unknown as Chore;
    const row = choreRow(c, { open: true, copied: false, onToggle: vi.fn(), onCopied: vi.fn(), onGo: vi.fn(), onOpenFile: null, onDecline: vi.fn(), onRecheck: vi.fn() });
    const cell = [...row.querySelectorAll<HTMLElement>("*")].find((e) => e.textContent === why && e.children.length === 0)!;
    expect(cell).toBeDefined();
    const ws = [...cell.classList].map((k) => declsOf(`.${k}`).get("white-space")).find((v) => v !== undefined);
    expect(ws).toBe("pre-line");
  });
});
