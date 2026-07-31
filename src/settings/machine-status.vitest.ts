/**
 * S3：机器状态账本。
 *
 * 这里最要紧的两条不是 CRUD，而是**它拒绝做什么**：
 * 1. 读的时候不发起任何 IO（主计划 §1-2「状态灯绝不引入轮询」）；
 * 2. 没记录过就说「未测过」，不猜一个 ✓ 出来。
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  recordFacet,
  readStatus,
  forgetMachine,
  renameMachine,
  formatAge,
  describeFacet,
} from "./machine-status";
import { LS_KEYS } from "../local-storage";

const T0 = 1_700_000_000_000;

beforeEach(() => localStorage.clear());

describe("账本读写", () => {
  it("写了才读得到；没写过的机器是空表（不是编造的默认值）", () => {
    expect(readStatus("aya")).toEqual({});
    recordFacet("aya", "connection", { kind: "ok", at: T0 });
    expect(readStatus("aya")).toEqual({
      connection: { kind: "ok", at: T0 },
    });
    // 别的机器不受影响
    expect(readStatus("nano")).toEqual({});
  });

  it("同一台机器的不同 facet 各存各的，互不覆盖", () => {
    recordFacet("aya", "connection", { kind: "ok", at: T0 });
    recordFacet("aya", "daemon", { kind: "fail", detail: "没装", at: T0 + 1 });
    const s = readStatus("aya");
    expect(s.connection?.kind).toBe("ok");
    expect(s.daemon).toEqual({ kind: "fail", detail: "没装", at: T0 + 1 });
  });

  it("同一 facet 再记一次 = 覆盖（要的就是「上次那次」）", () => {
    recordFacet("aya", "ccm", { kind: "fail", at: T0 });
    recordFacet("aya", "ccm", { kind: "ok", detail: "v1.2", at: T0 + 5000 });
    expect(readStatus("aya").ccm).toEqual({
      kind: "ok",
      detail: "v1.2",
      at: T0 + 5000,
    });
  });

  it("forgetMachine 清掉一台（删机器后不该被下一台同名的继承）", () => {
    recordFacet("aya", "connection", { kind: "ok", at: T0 });
    recordFacet("nano", "connection", { kind: "fail", at: T0 });
    forgetMachine("aya");
    expect(readStatus("aya")).toEqual({});
    expect(readStatus("nano").connection?.kind).toBe("fail");
  });

  it("renameMachine 把记录挪过去（改个名字不该让状态凭空清零）", () => {
    recordFacet("aya", "daemon", { kind: "ok", at: T0 });
    renameMachine("aya", "aya-2");
    expect(readStatus("aya")).toEqual({});
    expect(readStatus("aya-2").daemon?.kind).toBe("ok");
  });

  it("坏存档当空处理，不炸（它只是缓存，丢了无所谓；炸了整页就没了）", () => {
    localStorage.setItem(LS_KEYS.machineStatus, "{ 这不是 JSON");
    expect(() => readStatus("aya")).not.toThrow();
    expect(readStatus("aya")).toEqual({});
    // 数组也不是合法形状
    localStorage.setItem(LS_KEYS.machineStatus, "[1,2,3]");
    expect(readStatus("aya")).toEqual({});
  });

  it("跨「重启」保留（换个模块实例仍读得到——存的是 localStorage 不是内存）", () => {
    recordFacet("aya", "connection", { kind: "ok", at: T0 });
    const raw = localStorage.getItem(LS_KEYS.machineStatus);
    expect(raw).toBeTruthy();
    expect(JSON.parse(raw!)).toEqual({ aya: { connection: { kind: "ok", at: T0 } } });
  });
});

describe("formatAge —— 刻意粗粒度", () => {
  it("分钟 / 小时 / 天三档", () => {
    expect(formatAge(T0, T0 + 30_000)).toBe("刚刚");
    expect(formatAge(T0, T0 + 3 * 60_000)).toBe("3 分钟前");
    expect(formatAge(T0, T0 + 2 * 3_600_000)).toBe("2 小时前");
    expect(formatAge(T0, T0 + 5 * 86_400_000)).toBe("5 天前");
  });

  it("边界不跳档跳错（59 秒还是刚刚，60 秒才是 1 分钟前）", () => {
    expect(formatAge(T0, T0 + 59_999)).toBe("刚刚");
    expect(formatAge(T0, T0 + 60_000)).toBe("1 分钟前");
    expect(formatAge(T0, T0 + 3_599_999)).toBe("59 分钟前");
    expect(formatAge(T0, T0 + 3_600_000)).toBe("1 小时前");
  });

  it("时钟回拨不显示负数（存档可能来自另一台机器 / 系统时间被改过）", () => {
    expect(formatAge(T0, T0 - 10_000)).toBe("刚刚");
  });
});

describe("describeFacet", () => {
  it("★ 没记录过 = 「未测过」，**不猜**", () => {
    // 填个好看的 ✓ 等于替用户下了一个他从没做过的结论。
    expect(describeFacet(undefined, T0)).toEqual({
      icon: "·",
      text: "未测过",
      tone: "unknown",
    });
  });

  it("★ 「不适用」与「没测过」是两回事", () => {
    // 本机不需要 daemon（主计划 §2.4 逐字写「不需要」）。混成一个值的话，
    // 用户会以为本机缺了个组件。
    const na = describeFacet({ kind: "na", detail: "不需要", at: T0 }, T0);
    expect(na.tone).toBe("na");
    expect(na.text).toBe("不需要");
    expect(na.tone).not.toBe(describeFacet(undefined, T0).tone);
    // 不适用没有新鲜度可言 —— 不带时间
    expect(na.text).not.toMatch(/前|刚刚/);
  });

  it("ok / fail 都带年龄（这是「绝不显示成实时」的落点）", () => {
    const ok = describeFacet({ kind: "ok", at: T0 }, T0 + 3 * 60_000);
    expect(ok.icon).toBe("✓");
    expect(ok.text).toBe("3 分钟前");
    const fail = describeFacet(
      { kind: "fail", detail: "连不上", at: T0 },
      T0 + 2 * 3_600_000,
    );
    expect(fail.icon).toBe("✗");
    expect(fail.text).toBe("连不上 · 2 小时前");
  });
});
