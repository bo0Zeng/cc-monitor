// account-ux U1：账号色 slot 纯函数测试。
import { describe, it, expect } from "vitest";
import { accountColorSlot, ACCOUNT_COLOR_SLOTS } from "./account-color";

describe("accountColorSlot（FNV-1a % 8）", () => {
  it("确定性：同名恒同 slot", () => {
    expect(accountColorSlot("zhang")).toBe(accountColorSlot("zhang"));
    expect(accountColorSlot("bob")).toBe(accountColorSlot("bob"));
  });
  it("始终落在 [0, ACCOUNT_COLOR_SLOTS)", () => {
    for (const n of ["z", "b", "wei", "工作号", "a-very-long-account-name", "x1", "X1"]) {
      const s = accountColorSlot(n);
      expect(Number.isInteger(s)).toBe(true);
      expect(s).toBeGreaterThanOrEqual(0);
      expect(s).toBeLessThan(ACCOUNT_COLOR_SLOTS);
    }
  });
  it("空串不抛，返回确定 slot", () => {
    expect(accountColorSlot("")).toBe(accountColorSlot(""));
    expect(accountColorSlot("")).toBeGreaterThanOrEqual(0);
    expect(accountColorSlot("")).toBeLessThan(ACCOUNT_COLOR_SLOTS);
  });
  it("大小写 / 相近名区分（各自确定，不要求不同，但不得抛）", () => {
    expect(() => accountColorSlot("a")).not.toThrow();
    expect(() => accountColorSlot("A")).not.toThrow();
    expect(() => accountColorSlot("工作")).not.toThrow();
  });
  it("分布：一组不同名字覆盖 ≥4 个不同 slot（粗略均匀性，非严格）", () => {
    const names = ["z", "b", "wei", "amy", "dev", "prod", "test", "main", "alt", "work"];
    const slots = new Set(names.map(accountColorSlot));
    expect(slots.size).toBeGreaterThanOrEqual(4);
  });
});
