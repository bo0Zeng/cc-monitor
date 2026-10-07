// 拒绝码 ⇒ 一句话按表（`refusalsByTable`）：认得的码说表里那一句、认不出有原话 ⇒「其它码」、没原话 ⇒「没给原因」。
import { describe, it, expect } from "vitest";
import { refusalsByTable } from "../../../src/frontend/ui/control-said";

describe("refusalsByTable", () => {
  const r = refusalsByTable(
    { not_installed: (d) => `没装:${d}`, bad_id: (d) => `坏名:${d}` },
    { other: (d) => `其它:${d}`, none: () => "没给原因" },
  );
  it("认得的码：说表里那一句，带上原话", () => {
    expect(r.byCode("not_installed", "x")).toBe("没装:x");
    expect(r.byCode("bad_id", "")).toBe("坏名:");
  });
  it("认不出的码：有原话 ⇒ 其它码那一句；原话是空白 ⇒ 没给原因", () => {
    expect(r.byCode("new_code", "z")).toBe("其它:z");
    expect(r.byCode("new_code", "  ")).toBe("没给原因");
    expect(r.byCode("toString", "q"), "原型上的名字不算认得的码").toBe("其它:q");
    expect(r.noReason()).toBe("没给原因");
  });
});
