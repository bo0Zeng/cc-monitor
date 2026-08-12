// P6c（#69 b/c）：resume 命令预设的**纯**那半 —— 并入规则与盘上脏值的宽容读法。
//
// 面板那半（点一下填回去 / 远端预设走越层诊断）住 `settings/panel-block-isolation.vitest.ts`；
// 这里只钉不需要 DOM 的部分，好让失败信息直接指向规则本身。
import { describe, it, expect, vi, beforeEach } from "vitest";

const store = { cfg: {} as Record<string, unknown> };
vi.mock("./config", () => ({
  loadConfig: vi.fn(async () => store.cfg),
  saveConfig: vi.fn(async (c: Record<string, unknown>) => {
    store.cfg = c;
  }),
}));

import { withResumePreset, getBehavior, RESUME_PRESET_CAP } from "./behavior";

describe("P6c withResumePreset：并入规则", () => {
  it("★ P6c-Y2：trim、空不收、重复上浮、上界从尾部丢", () => {
    // 空 = 「用默认」，不是一条命令 ⇒ 不该占一个预设位。
    expect(withResumePreset(["a"], "")).toEqual(["a"]);
    expect(withResumePreset(["a"], "   ")).toEqual(["a"]);
    // trim 之后才比，否则 `"ccm "` 与 `"ccm"` 会各占一格。
    expect(withResumePreset([], "  ccm  ")).toEqual(["ccm"]);
    // 重复**上浮到最前**（刚用过的最可能再用），不产生第二份。
    expect(withResumePreset(["a", "b", "c"], "c")).toEqual(["c", "a", "b"]);
    expect(withResumePreset(["a", "b"], "b").filter((x) => x === "b")).toHaveLength(1);
    // 不改原数组（调用方拿它渲染中）。
    const orig = ["a", "b"];
    withResumePreset(orig, "z");
    expect(orig).toEqual(["a", "b"]);
  });

  it("★ P6c-Y2：**上界真的裁**（灌满再验最老的被挤出）", () => {
    // ⚠ 这条不是形式主义：本工作区 `P0c` 的 `M-D2` 就是「缓存有上界」当时没有任何东西
    // 守着 —— 去掉裁剪的变异**第一次跑照样绿**。有界必须被灌满验过才算数。
    let list: string[] = [];
    for (let i = 0; i < RESUME_PRESET_CAP + 5; i++) list = withResumePreset(list, `cmd${i}`);
    expect(list).toHaveLength(RESUME_PRESET_CAP);
    // 最新的在最前，最老的那几条已经不在。
    expect(list[0]).toBe(`cmd${RESUME_PRESET_CAP + 4}`);
    expect(list).not.toContain("cmd0");
    expect(list).not.toContain("cmd4");
  });
});

describe("P6c 盘上读回来：脏值不许进", () => {
  beforeEach(() => {
    store.cfg = {};
  });

  it("★ P6c-Y2：不是数组 / 混着非字符串 / 有重复 / 超上界 —— 逐种都收拾干净", async () => {
    // 盘上可能是**任何东西**：用户手改、旧版本、半截写入。
    store.cfg = { resumeCommandLocalPresets: "ccm" }; // 不是数组
    expect((await getBehavior()).resumeCommandLocalPresets).toEqual([]);

    store.cfg = {
      resumeCommandLocalPresets: ["ccm", 7, null, { a: 1 }, "  ", "cct", "ccm"],
    };
    // 非字符串丢掉、空白丢掉、重复丢掉，顺序保留。
    expect((await getBehavior()).resumeCommandLocalPresets).toEqual(["ccm", "cct"]);

    store.cfg = {
      resumeCommandRemotePresets: Array.from({ length: 100 }, (_, i) => `c${i}`),
    };
    expect((await getBehavior()).resumeCommandRemotePresets).toHaveLength(RESUME_PRESET_CAP);
  });

  it("缺字段 ⇒ 空列表，不是 undefined（调用方直接遍历它）", async () => {
    const b = await getBehavior();
    expect(b.resumeCommandLocalPresets).toEqual([]);
    expect(b.resumeCommandRemotePresets).toEqual([]);
  });
});
