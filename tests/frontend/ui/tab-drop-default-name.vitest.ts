/**
 * 默认组名「组 N」的编号从组名按文案键认出来，不从中文里抠：改了「组 {n}」那条文案，下一个编号照样对。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/copy-table")>();
  return {
    ...real,
    // 「组 {n}」那一条换个说法（别的照原表）
    copyText: (key: string, args: Record<string, string | number> = {}) =>
      key === "tabDrop.group.defaultName" ? `Group#${args.n}` : real.copyText(key as never, args),
  };
});

const { defaultGroupName } = await import("../../../src/frontend/ui/tab-drop");

describe("默认组名的编号跟着文案走", () => {
  it("文案换成 Group#{n} ⇒ 照样认出已有的最大编号，下一个 +1", () => {
    expect(defaultGroupName(null, null, [])).toBe("Group#1");
    expect(defaultGroupName(null, null, ["Group#1", "白天", "Group#3"])).toBe("Group#4");
    expect(defaultGroupName(null, null, ["组 7"]), "旧说法的名字不再算默认名").toBe("Group#1");
  });
});
