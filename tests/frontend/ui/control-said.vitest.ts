// 被拒那一句由那台后端写好（`asSaid`）：有那一句 ⇒ 原样上屏；空白 / 读不出 ⇒「没给原因」那一句。界面不按码另写句子。
import { describe, it, expect } from "vitest";
import { asSaid } from "../../../src/frontend/ui/control-said";

describe("asSaid", () => {
  const r = asSaid(() => "none-said");
  it("那台写了那一句 ⇒ 原样上屏（码不看）", () => {
    expect(r.byCode("not_installed", "s-1")).toBe("s-1");
    expect(r.byCode("new_code", "s-2")).toBe("s-2");
  });
  it("那一句是空白 ⇒ 没给原因那一句", () => {
    expect(r.byCode("new_code", "  ")).toBe("none-said");
    expect(r.noReason()).toBe("none-said");
  });
});
