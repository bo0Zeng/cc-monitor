/**
 * 终端那三样各读脸上各自那一格（`face-can.ts`）：拉前 `can.front` · 在终端里打开 `can.attach` · 看它的终端 `can.view`。
 * 不再并成一颗「终端」让界面挑；本机会话与远端会话同一道。看的这一端的壳拉不拉得了窗界面不判（点了壳照实回，那一句核心写）。
 * 还没收到脸 ⇒ 灰着、不说为什么（空串）。
 *
 * 脸取金样（`session-faces.ts`），不手写一份。
 */
import { describe, expect, it } from "vitest";

import { attachBlocked, frontBlocked, terminalAlive, viewBlocked } from "../../../src/frontend/ui/face-can";
import { goldenFace } from "../../test-support/session-faces";
import { productionTsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

describe("终端三样各读各的那一格", () => {
  it("tmux 里一条活会话：拉前 · 在终端里打开 · 看它的终端三样都点得了", () => {
    const f = goldenFace("live-needs-you-tmux");
    expect([frontBlocked(f), attachBlocked(f), viewBlocked(f)]).toEqual([null, null, null]);
  });

  it("不在任何宿主里：只拉得了前；在终端里打开 · 看它的终端灰着、写那台那一句", () => {
    const bare = goldenFace("live-working-plain");
    expect(frontBlocked(bare)).toBeNull();
    expect(bare.can.attach.can, "量具自检：不在宿主里的会话接不进去").toBe("no");
    expect(attachBlocked(bare)).toBe(bare.can.attach.can === "no" ? bare.can.attach.why : "x");
    expect(viewBlocked(bare)).toBe(bare.can.view.can === "no" ? bare.can.view.why : "x");
  });

  it("已结束 · 看不见：三样各写那一句；还没收到脸 ⇒ 空串", () => {
    const ended = goldenFace("ended");
    expect(frontBlocked(ended)).toBe(ended.can.front.can === "no" ? ended.can.front.why : "x");
    expect(viewBlocked(ended), "已结束：看它的终端先恢复再开").toBeNull();
    const unseen = goldenFace("unseen");
    for (const why of [frontBlocked(unseen), attachBlocked(unseen), viewBlocked(unseen)]) expect(why).toBe(unseen.line);
    expect([frontBlocked(null), attachBlocked(null), viewBlocked(null)]).toEqual(["", "", ""]);
  });

  it("进程或它住的终端还在（自动跟随 · 已隐藏那一行）只看那台写好的一生", () => {
    expect(terminalAlive(goldenFace("live-idle"))).toBe(true);
    expect(terminalAlive(goldenFace("reconnectable"))).toBe(true);
    for (const key of ["ended", "gone", "unclear", "unseen"] as const) expect(terminalAlive(goldenFace(key)), key).toBe(false);
    expect(terminalAlive(null)).toBe(false);
  });

  it("界面不自判：那三格只在 face-can.ts 里读，并成一颗的旧读法不在", () => {
    const hits: string[] = [];
    for (const { file, text } of productionTsFiles("src/frontend/ui")) {
      const code = stripComments(text, "ts");
      for (const needle of ["terminalWay", "terminalWhy", "can.terminal", "TermWay", 'dot === "exited"']) {
        if (code.includes(needle)) hits.push(`${file}: ${needle}`);
      }
      if (file !== "src/frontend/ui/face-can.ts" && /can\.(front|attach|view)\b/.test(code)) hits.push(`${file}: can.front/attach/view`);
    }
    expect(hits).toEqual([]);
  });
});
