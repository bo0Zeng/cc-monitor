// F43：指纹重置按钮显隐纯逻辑。remote-section 的 DOM 主体重(拉整卡),这里只钉住
// 「有固化指纹才显示重置按钮」这条判定,防未来误改成空指纹也显示(重置无意义)。
import { describe, it, expect } from "vitest";
import { shouldShowResetFingerprint, parseAddressLines, describeStage } from "./remote-section";

describe("F43 shouldShowResetFingerprint", () => {
  it("已固化非空指纹 → 显示", () => {
    expect(shouldShowResetFingerprint("SHA256:abc")).toBe(true);
  });
  it("空 / 纯空白 → 不显示(重置无意义)", () => {
    expect(shouldShowResetFingerprint("")).toBe(false);
    expect(shouldShowResetFingerprint("   ")).toBe(false);
    expect(shouldShowResetFingerprint("\n\t")).toBe(false);
  });
});

describe("F45 parseAddressLines", () => {
  it("按行 trim + 去空行", () => {
    expect(parseAddressLines("10.0.0.2\n  pi:2222 \n\n[::1]:22\n   ")).toEqual([
      "10.0.0.2",
      "pi:2222",
      "[::1]:22",
    ]);
  });
  it("空文本 → 空数组", () => {
    expect(parseAddressLines("")).toEqual([]);
    expect(parseAddressLines("   \n  ")).toEqual([]);
  });
});

describe("F46 describeStage", () => {
  it("各阶段 kind 有图标+文案", () => {
    expect(describeStage({ kind: "dialing", endpoint: "h:22" }).text).toContain("拨号 h:22");
    expect(describeStage({ kind: "won", endpoint: "h:22" }).icon).toBe("✓");
    expect(describeStage({ kind: "failed", endpoint: "h:22", reason: "x" }).text).toContain("失败");
    expect(describeStage({ kind: "auth", ok: false, detail: "被拒" }).text).toContain("被拒");
    expect(describeStage({ kind: "auth", ok: true, detail: null }).text).toContain("鉴权通过");
    expect(describeStage({ kind: "established" }).text).toContain("就绪");
    expect(describeStage({ kind: "hostKey", endpoint: "h:22", fingerprint: "SHA256:x" }).text).toContain("SHA256:x");
  });
});
