/**
 * kit 路径显示形：家目录那一截写成 `~`，别的一字不动。
 */
import { describe, it, expect } from "vitest";
import { homeShort } from "../../../../src/frontend/ui/kit/path";

describe("homeShort", () => {
  it("家目录下的路径 ⇒ ~/…；就是家目录 ⇒ ~", () => {
    expect(homeShort("/home/u/.cc-monitor/accounts/a", "/home/u")).toBe("~/.cc-monitor/accounts/a");
    expect(homeShort("/home/u", "/home/u/")).toBe("~");
    expect(homeShort("C:\\Users\\u\\.cc-monitor", "C:\\Users\\u")).toBe("~\\.cc-monitor");
  });
  it("只在路径分隔处切：/home/user2 不是 /home/u 下的", () => {
    expect(homeShort("/home/user2/x", "/home/u")).toBe("/home/user2/x");
  });
  it("家目录没答 / 空 ⇒ 原样", () => {
    expect(homeShort("/home/u/x", null)).toBe("/home/u/x");
    expect(homeShort("/home/u/x", "")).toBe("/home/u/x");
    expect(homeShort("/x", "/")).toBe("/x");
  });
});
