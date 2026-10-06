/**
 * 要求：「启动时记住的那一格只在那个会话出现时恢复、就绪前不许被覆盖、超时仍没出现就明说一句、不静默换」——
 * `src/frontend/ui/startup-active.ts` 的真值表（按事件判：`live` 到了 · 每台报完清单 · 用户手动切）。
 */
import { describe, it, expect } from "vitest";
import { StartupActive, type StartupIo } from "../../../src/frontend/ui/startup-active";

function rig() {
  const log: string[] = [];
  const io: StartupIo = {
    switchTo: (s) => log.push(`switch ${s}`),
    holdMemory: (h) => log.push(`hold ${h}`),
    rememberCurrent: () => log.push("remember"),
    sayGone: (s) => log.push(`gone ${s}`),
  };
  return { log, io };
}

describe("启动时记住的那一格", () => {
  it("它出现了 ⇒ 切过去；在那之前记忆不许被写（hold）；别的会话出现不算", () => {
    const { log, io } = rig();
    const s = new StartupActive("want", false, io);
    expect(log).toEqual(["hold true"]);
    s.onAppeared("other");
    expect(s.waitingFor).toBe("want");
    s.onAppeared("want");
    expect(log).toEqual(["hold true", "hold false", "switch want"]);
    expect(s.waitingFor).toBeNull();
    s.onAppeared("want"); // 应用一次即清
    expect(log.filter((l) => l === "switch want")).toHaveLength(1);
  });

  it("壳说各台都报完了、它没出现 ⇒ 明说一句、放下等待（不静默换）", () => {
    const { log, io } = rig();
    const s = new StartupActive("want", false, io);
    expect(log).toEqual(["hold true"]);
    s.onAllListed();
    expect(log).toEqual(["hold true", "hold false", "gone want"]);
    s.onAppeared("want"); // 放下之后迟到的它不再抢焦点
    expect(log).not.toContain("switch want");
  });

  it("用户手动切了 ⇒ 用户的选择优先：放下等待、记下此刻那一格", () => {
    const { log, io } = rig();
    const s = new StartupActive("want", false, io);
    s.onManualSwitch();
    expect(log).toEqual(["hold true", "hold false", "remember"]);
    s.onAppeared("want");
    s.onAllListed();
    expect(log).toEqual(["hold true", "hold false", "remember"]);
  });

  it("起步那一刻它就在 ⇒ 当场切、不等不压记忆；没有记住的 ⇒ 什么都不做", () => {
    const a = rig();
    new StartupActive("here", true, a.io);
    expect(a.log).toEqual(["switch here"]);
    const b = rig();
    const s = new StartupActive(null, false, b.io);
    s.onAllListed();
    s.onManualSwitch();
    expect(b.log).toEqual([]);
  });
});
