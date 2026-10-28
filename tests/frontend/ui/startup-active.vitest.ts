/**
 * 要求：「启动时记住的那一格只在那个会话出现时恢复、就绪前不许被覆盖、超时仍没出现就明说一句、不静默换」——
 * `src/frontend/ui/startup-active.ts` 的真值表（按事件判：`live` 到了 · 每台报完清单 · 用户手动切）。
 */
import { describe, it, expect } from "vitest";
import { StartupActive, type StartupIo } from "../../../src/frontend/ui/startup-active";
import { AllListed } from "../../../src/frontend/ui/all-listed";

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

/**
 * 每台一条会话流；壳说「各台都报完」（`listed.all`）落在某一台那条流上，别的台那条流上它自己的宣告可能还在路上
 * （就绪点重放一条订阅接一条交：本机那条的终局带着 `all`，远端那条的骨架排在它后面）。
 * 记住的那一格在远端 ⇒ 本机那条先说完不算，每台都在自己那条流上报完了才判「未出现」。
 */
describe("「各台都报完」那一刻：每台都在自己那条流上报完了才算", () => {
  function wired(machines: string[]) {
    const { log, io } = rig();
    const s = new StartupActive("far", false, io);
    const book = new AllListed(() => s.onAllListed());
    book.setMachines(machines);
    return { log, s, book };
  }
  /** 那台那条流上来了一格 `listed`（同 main.ts 的接法：先记这台报完，带 `all` 再记壳那一句）。 */
  function listed(book: AllListed, origin: string, all: boolean): void {
    book.reported(origin);
    if (all) book.shellSays();
  }

  it("★ 本机那条先到「报完了 ＋ 各台都报完」、记住的在 box-b 那台 ⇒ 不说未出现；box-b 的宣告到了 ⇒ 切过去", () => {
    const { log, s, book } = wired(["<local>", "box-b"]);
    listed(book, "<local>", true);
    expect(log, "box-b 那条流还没报完：判不了，不许说未出现").not.toContain("gone far");
    expect(s.waitingFor).toBe("far");
    s.onAppeared("far");
    listed(book, "box-b", true);
    expect(log).toEqual(["hold true", "hold false", "switch far"]);
  });

  it("每台都在自己那条流上报完了、它仍没出现 ⇒ 明说一句（只一次）", () => {
    const { log, book } = wired(["<local>", "box-b"]);
    listed(book, "box-b", false);
    listed(book, "<local>", true);
    listed(book, "box-b", true);
    expect(log).toEqual(["hold true", "hold false", "gone far"]);
  });

  it("壳还没说各台都报完 ⇒ 每台自己报完了也不算；机器表摘掉唯一没报完的那台 ⇒ 当场算", () => {
    const a = wired(["<local>", "box-b"]);
    listed(a.book, "<local>", false);
    listed(a.book, "box-b", false);
    expect(a.book.settled).toBe(false);
    expect(a.log).toEqual(["hold true"]);
    const b = wired(["<local>", "box-b"]);
    listed(b.book, "<local>", true);
    b.book.setMachines(["<local>"]);
    expect(b.log).toEqual(["hold true", "hold false", "gone far"]);
  });

  it("只订一台、它报完了 ⇒ 当场算（一台的老样子不变）", () => {
    const { log, book } = wired(["<local>"]);
    listed(book, "<local>", true);
    expect(log).toEqual(["hold true", "hold false", "gone far"]);
  });
});
