/**
 * 设置窗里写好的说明可以分行（`\n`）：落进的那一格照分行画（`white-space: pre-line`）。
 * 门禁里没有排版引擎：这里钉住那一句落在哪一格、那一格的样式让换行成立；真引擎里的样子看截图台架
 * （settings-data-chore-open · settings-machine-config）。
 */
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";

vi.mock("@tauri-apps/api/core", () => ({ invoke: () => new Promise(() => {}) }));

import { choreRow } from "../../../../src/frontend/ui/settings/chore-row";
import { buildConfigPage } from "../../../../src/frontend/ui/settings/config-page";
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

/** `root` 里正文恰好是 `text` 的那一格的 white-space（按它身上各类在 settings.css 里的声明）。 */
function whiteSpaceOf(root: HTMLElement, text: string): string | undefined {
  expect(text).toContain("\n");
  const cell = [...root.querySelectorAll<HTMLElement>("*")].find((e) => e.textContent === text && e.children.length === 0);
  expect(cell).toBeDefined();
  return [...cell!.classList].map((k) => declsOf(`.${k}`).get("white-space")).find((v) => v !== undefined);
}

describe("设置窗 · 分行的说明照分行画", () => {
  it("★ 待办点开那一句「为什么」（接中转那一件）", () => {
    const why = copyText("beChore.relay.why");
    const c = { id: "relay", kind: "optional", state: "todo", name: "relay", loc: "", said: "todo", why, steps: [], diff: null, copy: null, whole: null, wholeCovers: [], file: null, go: null, howUrl: null, action: "copySnippet" } as unknown as Chore;
    const row = choreRow(c, { open: true, copied: false, onToggle: vi.fn(), onCopied: vi.fn(), onGo: vi.fn(), onOpenFile: null, onDecline: vi.fn(), onRecheck: vi.fn() });
    expect(whiteSpaceOf(row, why)).toBe("pre-line");
  });

  it("★ 机器页「别名与配置文件」页头那一句", () => {
    const page = buildConfigPage({ platform: "posix", origin: () => "devbox", machine: () => "devbox" });
    expect(whiteSpaceOf(page, copyText("cfgPage.head.intro"))).toBe("pre-line");
  });
});
