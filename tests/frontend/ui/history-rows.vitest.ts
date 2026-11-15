/**
 * `src/frontend/ui/views/history-rows.ts` 的判据：行尾的恢复那一项只照那台判好的 `can.resume` 画，不另判。
 *
 * 判据：
 * 1. **认不出是哪一家的那一行**（线上不带 `agent`，`can.resume` 是 `no`、`can.resumeWhy` 是那台写好的那一句）：恢复那一颗灰着，照摆那一句，
 *    点了不交给页面去恢复。能恢复的那一行照常点得动。
 * 2. **在等人的那一行**：脸上的点是 `asks`（待答请求）或 `needs-you`（需手动）都上「在等」那一色的徽章，字照脸上的名字。
 */
import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { rowBadges, sessionRow, type RowHooks } from "../../../src/frontend/ui/views/history-rows";
import type { HistoryRow } from "../../../src/frontend/ui/history-list-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";

const golden = JSON.parse(
  readFileSync(join(__dirname, "../../__fixtures__", "history-list.golden.json"), "utf8"),
) as { order: string[]; up: Record<string, HistoryRow> };

/** 金样里一行没在跑、能恢复的（替身家那一形不点名哪一家）。 */
function resumable(): HistoryRow {
  const r = Object.values(golden.up).find((x) => x.can.resume === "yes");
  if (!r) throw new Error("金样里没有能恢复的行");
  return structuredClone(r);
}

function hooks(): RowHooks & { resumed: HistoryRow[] } {
  const resumed: HistoryRow[] = [];
  return {
    resumed,
    select: () => {},
    resume: (r) => resumed.push(r),
    menu: () => {},
    openWindow: () => {},
    toggleForks: () => {},
  };
}

const opts = { compact: false, forksOpen: false, child: false, orphan: false, forksText: null };

/** 行尾那颗恢复（第一颗按钮）。 */
function resumeButton(el: HTMLElement): HTMLButtonElement {
  const b = el.querySelector("button");
  if (!b) throw new Error("行尾没有按钮");
  return b;
}

describe("历史页的行：恢复那一项照 can.resume", () => {
  it("认不出是哪一家的那一行：恢复灰着，说那一句，点了不恢复", () => {
    const r = resumable();
    delete (r as { agent?: string }).agent;
    r.can = { ...r.can, resume: "no", resumeWhy: copyText("rsAgents.family.unknown"), accounts: false, fork: false };
    r.agentTag = copyText("rsAgents.family.unknown");
    const h = hooks();
    const b = resumeButton(sessionRow(r, h, opts));
    expect(b.getAttribute("aria-disabled")).toBe("true");
    expect(b.title || b.dataset.kitTitle).toBe(copyText("rsAgents.family.unknown"));
    b.click();
    expect(h.resumed).toEqual([]);
  });

  it("能恢复的那一行照常点得动", () => {
    const h = hooks();
    const b = resumeButton(sessionRow(resumable(), h, opts));
    expect(b.getAttribute("aria-disabled")).toBeNull();
    b.click();
    expect(h.resumed).toHaveLength(1);
  });
});

describe("历史页的行：在等人那一枚徽章", () => {
  it("`asks` 与 `needs-you` 都上「在等」那一色的徽章，字照脸上的名字", () => {
    const live = Object.values(golden.up).find((x) => x.face.life.is === "live");
    if (!live) throw new Error("金样里没有在跑的行");
    for (const [dot, name] of [["asks", copyText("needs.asks.title")], ["needs-you", copyText("beSession.needs.unknown")]] as const) {
      const r = structuredClone(live);
      r.face = { ...r.face, dot: dot as HistoryRow["face"]["dot"], name };
      const need = rowBadges(r).filter((b) => b.dataset.emph === "need");
      expect(need.map((b) => b.textContent), dot).toEqual([name]);
    }
  });
});
