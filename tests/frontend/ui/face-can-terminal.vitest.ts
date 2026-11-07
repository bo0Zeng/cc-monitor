/**
 * 「终端」那一颗只读后端脸上的 `can.terminal`（会话头 · 右键 · 命令面板 · 快捷键 · 报错卡同一道，`face-can.ts::terminalWay`）：
 * 那台把两种看法都写好了，界面只挑 —— 本机标签页挑 `near`、远端挑 `far`；还没收到脸 ⇒ 没有（灰着、不说为什么）。
 * 界面不再自己看壳拉不拉得了窗（`terminalFrontAvailable`）、不再按「本机走 ↗、别处接进去」自己拼两条路。
 *
 * 脸取金样（`session-faces.ts`），不手写一份。
 */
import { describe, expect, it } from "vitest";

import { terminalAlive, terminalWay, terminalWhy } from "../../../src/frontend/ui/face-can";
import { goldenFace } from "../../test-support/session-faces";
import { productionTsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

describe("「终端」只读脸上的 can.terminal", () => {
  it("本机标签页挑 near、远端挑 far：照那台写好的那一格原样交出", () => {
    for (const key of ["live-needs-you-tmux", "reconnectable", "live-working-plain", "ended"] as const) {
      const f = goldenFace(key);
      expect(terminalWay(f, false), `${key} 本机`).toEqual(f.can.terminal.near);
      expect(terminalWay(f, true), `${key} 远端`).toEqual(f.can.terminal.far);
    }
    expect(terminalWay(null, false)).toBeNull();
  });

  it("灰着那一句：做得了 ⇒ null；做不了 ⇒ 那台写好的那一句；还没收到脸 ⇒ 空串", () => {
    const bare = goldenFace("live-working-plain");
    expect(bare.can.terminal.far.go, "量具自检：不在宿主里的会话别处没有终端可接").toBe("no");
    expect(terminalWhy(bare, true)).toBe(bare.can.terminal.far.go === "no" ? bare.can.terminal.far.why : "x");
    expect(terminalWhy(goldenFace("live-needs-you-tmux"), true)).toBeNull();
    const unseen = goldenFace("unseen");
    expect(terminalWhy(unseen, false)).toBe(unseen.can.terminal.near.go === "no" ? unseen.can.terminal.near.why : "x");
    expect(terminalWhy(null, false)).toBe("");
  });

  it("进程或它住的终端还在（自动跟随 · 已隐藏那一行）只看那台写好的一生", () => {
    expect(terminalAlive(goldenFace("live-idle"))).toBe(true);
    expect(terminalAlive(goldenFace("reconnectable"))).toBe(true);
    for (const key of ["ended", "gone", "unclear", "unseen"] as const) expect(terminalAlive(goldenFace(key)), key).toBe(false);
    expect(terminalAlive(null)).toBe(false);
  });

  it("界面不自判：不问壳拉不拉得了窗，can.terminal 只在 face-can.ts 里读", () => {
    const hits: string[] = [];
    for (const { file, text } of productionTsFiles("src/frontend/ui")) {
      const code = stripComments(text, "ts");
      for (const needle of ["terminalFrontAvailable", "can.term.", "can.attach", 'dot === "exited"', "isAttachable"]) {
        if (code.includes(needle)) hits.push(`${file}: ${needle}`);
      }
      if (file !== "src/frontend/ui/face-can.ts" && code.includes("can.terminal")) hits.push(`${file}: can.terminal`);
    }
    expect(hits).toEqual([]);
  });
});
