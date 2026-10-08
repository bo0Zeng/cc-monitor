import { describe, expect, it } from "vitest";

import { copyText } from "../../src/frontend/ui/copy-table";
import { copyPattern } from "./copy-pattern";

describe("copyPattern：按文案键认一句话（占位处任意值）", () => {
  it("认得出取文口插出来的句子，值是 ASCII 还是汉字都认（接缝空格可有可无）", () => {
    expect(copyText("peerVersion.said.old", { machine: "gpu-01" })).toMatch(copyPattern("peerVersion.said.old"));
    expect(copyText("peerVersion.said.old", { machine: copyText("control.machine.local") })).toMatch(copyPattern("peerVersion.said.old"));
    expect(`前缀 ${copyText("peerVersion.said.old", { machine: "gpu-01" })}`).toMatch(copyPattern("peerVersion.said.old"));
  });

  it("给了值的占位必须是那个值；whole ⇒ 整串就是这一句", () => {
    const s = copyText("peerVersion.said.old", { machine: "gpu-01" });
    expect(s).toMatch(copyPattern("peerVersion.said.old", { machine: "gpu-01" }, { whole: true }));
    expect(s).not.toMatch(copyPattern("peerVersion.said.old", { machine: "devbox" }));
    expect(`前缀 ${s}`).not.toMatch(copyPattern("peerVersion.said.old", { machine: "gpu-01" }, { whole: true }));
  });

  it("缺了固定字的一截不认；没有的键、没有的占位名直接抛", () => {
    const s = copyText("peerVersion.said.old", { machine: "gpu-01" });
    expect(s.slice(0, -2)).not.toMatch(copyPattern("peerVersion.said.old", {}, { whole: true }));
    // @ts-expect-error 故意给一个表里没有的键
    expect(() => copyPattern("no.such.key")).toThrow();
    expect(() => copyPattern("peerVersion.said.old", { nope: "x" })).toThrow();
  });
});
