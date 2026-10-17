// 会话那颗点照后端那份整份脸画：金样里每种点都落到 `kit/status-dot` 的那一颗（样式表里有它那一档），灯那一档按点取。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { lightOf } from "../../../src/frontend/ui/session-status";
import { statusDot } from "../../../src/frontend/ui/kit/status-dot";
import type { SessionDot } from "../../../src/frontend/ui/generated/SessionDot";
import { allFaces } from "../../test-support/session-faces";
import { REPO_ROOT } from "../../test-support/repo-root";

const CSS = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/kit/status-dot.module.css"), "utf8");

describe("会话点照脸画", () => {
  it("★ 金样里每种点 ⇒ 状态点那一颗（data-state ＝ 线上名、样式表里有那一档、读屏名 ＝ 短名）", () => {
    const faces = allFaces();
    // 金样罩住线上每一种点（这张表漏一种编不过）。
    const every = { running: 1, "needs-you": 1, background: 1, idle: 1, exited: 1, ended: 1, gone: 1, unknown: 1 } satisfies Record<SessionDot, 1>;
    expect([...new Set(faces.map(([, f]) => f.dot))].sort()).toEqual(Object.keys(every).sort());
    for (const [key, f] of faces) {
      const d = statusDot(f.dot, f.name, "compact");
      expect(d.dataset.state, key).toBe(f.dot);
      expect(d.getAttribute("aria-label"), key).toBe(f.name);
      expect(CSS.includes(`.dot[data-state="${f.dot}"]`), `${key}：样式表里没有 ${f.dot} 那一档`).toBe(true);
    }
  });

  it("灯：需手动呼吸、空闲暗着，别的不叠", () => {
    const got = Object.fromEntries(allFaces().map(([k, f]) => [k, lightOf(f.dot)]));
    expect(got).toMatchObject({ "live-needs-you-tmux": "waiting", "live-idle": "idle", "live-working-plain": "", ended: "", reconnectable: "", unclear: "" });
  });
});
