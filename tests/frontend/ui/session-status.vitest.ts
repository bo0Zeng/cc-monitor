// 会话那颗点照后端那份整份脸画：金样里每种点都落到 `kit/status-dot` 的那一颗（样式表里有它那一档），灯那一档按点取。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { lightOf } from "../../../src/frontend/ui/session-status";
import { statusDot } from "../../../src/frontend/ui/kit/status-dot";
import type { SessionDot } from "../../../src/frontend/ui/generated/SessionDot";
import { allFaces } from "../../test-support/session-faces";
import { REPO_ROOT } from "../../test-support/repo-root";
import { productionRsFiles, productionTsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

const CSS = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/kit/status-dot.module.css"), "utf8");

describe("会话点照脸画", () => {
  it("★ 金样里每种点 ⇒ 状态点那一颗（data-state ＝ 线上名、样式表里有那一档、读屏名 ＝ 短名）", () => {
    const faces = allFaces();
    // 金样罩住线上每一种点（这张表漏一种编不过）。
    const every = { running: 1, asks: 1, "needs-you": 1, background: 1, idle: 1, exited: 1, ended: 1, gone: 1, unknown: 1 } satisfies Record<SessionDot, 1>;
    expect([...new Set(faces.map(([, f]) => f.dot))].sort()).toEqual(Object.keys(every).sort());
    for (const [key, f] of faces) {
      const d = statusDot(f.dot, f.name, "compact");
      expect(d.dataset.state, key).toBe(f.dot);
      expect(d.getAttribute("aria-label"), key).toBe(f.name);
      expect(CSS.includes(`.dot[data-state="${f.dot}"]`), `${key}：样式表里没有 ${f.dot} 那一档`).toBe(true);
    }
  });

  it("灯：在等人的两颗（待答请求 · 需手动）都呼吸、空闲暗着，别的不叠", () => {
    const got = Object.fromEntries(allFaces().map(([k, f]) => [k, lightOf(f.dot)]));
    expect(got).toMatchObject({ "live-asks": "waiting", "live-needs-you-tmux": "waiting", "live-idle": "idle", "live-working-plain": "", ended: "", reconnectable: "", unclear: "" });
  });
});

/**
 * 「这颗点在等人」（待答请求 · 需手动两颗）只判一处：Rust 是 `SessionDot::waits_for_human`（cmd-wire，哪几颗住 `SessionDot::WAITING`），前端照它现生成的 `WAITING_DOTS` 查、不另判。
 * 量法：「待答请求」那颗点是后来加的、谁点名它谁就在分它：生产源码剥注释后，点名它的每一处（Rust 标识符 `Asks` · TS 字面量 `"asks"`）
 * 按文件计数，两向等于下表。别处再判一份在等人（同行 · 折行 · 分臂 · 查表都一样）⇒ 红，改调那一处；收掉一处 ⇒ 红，删那一行。
 * 量不到的：用补集写（「不是 running / idle / …」）、不点名它的那种；Kotlin · CSS 不在人群里（画法另管）。
 */
const ASKS_DOT_PLACES: Record<string, number> = {
  "src/common/cmd-wire/src/frame.rs": 2, // 枚举那一值 ＋ `SessionDot::WAITING`（Rust 那一处；界面那一份由它现生成，不点名）
  "src/backend/wire/mod.rs": 1, // 造它的那一处：状态一句说界面答得了 ⇒ 点换成待答请求
  "src/common/face-core/src/groups.rs": 2, // 组头「待答请求 n」那一颗：几颗点的先后里那一值 ＋ 写字那一臂（与「需手动 n」各数各的）
  "src/frontend/ui/needs-book.ts": 1, // `columnFace`：列 ⇒ 点只对一次（栏顶那颗 · 浮层会话行都读它；列在代码里叫 answerable，不进这张表）
};

describe("在等人只判一处", () => {
  it("★ 点名「待答请求」那颗点的处数 ＝ 登记表", () => {
    const got: Record<string, number> = {};
    const count = (file: string, code: string, re: RegExp): void => {
      const n = code.match(re)?.length ?? 0;
      if (n > 0) got[file] = n;
    };
    for (const f of productionRsFiles("src")) count(f.file, stripComments(f.text, "rust"), /\bAsks\b/g);
    for (const f of productionTsFiles("src")) count(f.file, stripComments(f.text, "ts"), /(["'`])asks\1/g);
    expect(got).toEqual(ASKS_DOT_PLACES);
  });
});
