/**
 * 「在终端里打开」点不点得了只读后端脸上的 `can.attach`（会话头 · 报错卡 · 右键菜单同一道，`face-can.ts::canAttach`）：
 * 那台说能 ⇒ `from` 说在哪看才给（`everywhere` 托管终端哪台都给 · `elsewhere` tmux 会话只在别的机器上给，本机走 ↗）；那台说不能 · 还没收到脸 ⇒ 不给。
 * 界面不再自己看点（`dot === "exited"`）、不记 pidfile 的 `attachable`、不读 `can.attach` 以外的格判这一样。
 *
 * 脸取金样（`session-faces.ts`），不手写一份。
 */
import { describe, expect, it } from "vitest";

import { canAttach } from "../../../src/frontend/ui/face-can";
import { goldenFace } from "../../test-support/session-faces";
import { productionTsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

describe("「在终端里打开」只读脸上的 can.attach", () => {
  it("那台说能 ⇒ 照 from 定在哪看才给；说不能 · 没有脸 ⇒ 不给", () => {
    const exited = goldenFace("reconnectable");
    expect(exited.can.attach).toEqual({ can: "yes", from: "elsewhere" });
    expect(canAttach(exited, true)).toBe(true);
    expect(canAttach(exited, false)).toBe(false);

    const managed = goldenFace("live-idle");
    managed.can.attach = { can: "yes", from: "everywhere" };
    expect(canAttach(managed, true)).toBe(true);
    expect(canAttach(managed, false)).toBe(true);

    for (const key of ["ended", "gone", "unclear", "unseen"] as const) {
      const f = goldenFace(key);
      expect(f.can.attach.can, key).toBe("no");
      expect(canAttach(f, true), key).toBe(false);
    }
    expect(canAttach(null, true)).toBe(false);
  });

  it("界面不自判：不看点 · 不记 attachable，can.attach 只在 face-can.ts 里读", () => {
    const hits: string[] = [];
    for (const { file, text } of productionTsFiles("src/frontend/ui")) {
      const code = stripComments(text, "ts");
      for (const needle of ['dot === "exited"', "isAttachable", "notAttachableSids", "attachable:"]) {
        if (code.includes(needle)) hits.push(`${file}: ${needle}`);
      }
      if (file !== "src/frontend/ui/face-can.ts" && code.includes("can.attach")) hits.push(`${file}: can.attach`);
    }
    expect(hits).toEqual([]);
  });
});
