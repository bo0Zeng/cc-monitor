/**
 * S4a：「当前在看哪台机器」store。
 *
 * 三条性质，每条都对应一个具体的坏结果：
 * - 同值不通知 → 否则四个订阅者互相激起 ssh 往返（变相轮询，撞 §1-2）。
 * - 订阅者异常隔离 → 否则其余分节**停在上一台机器**上，界面看不出异常。
 * - 退订真的退 → 否则详情页来回切会越积越多订阅者，每次切换 reload N 遍。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  getCurrentMachine,
  setCurrentMachine,
  subscribeMachine,
  __resetMachineContextForTests,
} from "../../../../src/frontend/ui/settings/machine-context";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

beforeEach(() => __resetMachineContextForTests());

describe("machine-context", () => {
  // 〔C4a · `设计/05 §8` 步 2〕本机是具名的 `LOCAL_ORIGIN`，不再是 `null`。
  it("初始是本机（`LOCAL_ORIGIN`）", () => {
    expect(getCurrentMachine()).toBe(LOCAL_ORIGIN);
  });

  // 〔C4a〕上一版「空串归一成 null（= 本机）」：那是「没说」被当成本机 —— 步 2 治的正是这个
  //   （Rust `Origin::route` 同样拒空白名）。现在空白名什么都不改。
  it("设了就读得到；空白名不是任何一台机器 ⇒ 不切", () => {
    setCurrentMachine("devbox");
    expect(getCurrentMachine()).toBe("devbox");
    setCurrentMachine("");
    expect(getCurrentMachine()).toBe("devbox");
    setCurrentMachine("   ");
    expect(getCurrentMachine()).toBe("devbox");
  });

  it("订阅者收到新值", () => {
    const seen: string[] = [];
    subscribeMachine((o) => seen.push(o));
    setCurrentMachine("devbox");
    setCurrentMachine("nano");
    setCurrentMachine(LOCAL_ORIGIN);
    expect(seen).toEqual(["devbox", "nano", LOCAL_ORIGIN]);
  });

  it("★ 同值重复设置**不通知**（每次通知都是一次 ssh 往返）", () => {
    const fn = vi.fn();
    subscribeMachine(fn);
    setCurrentMachine("devbox");
    setCurrentMachine("devbox");
    setCurrentMachine("devbox");
    expect(fn).toHaveBeenCalledTimes(1);
    // 〔C4a〕切回本机通知一次；随后的空白名不是任何一台机器 ⇒ 不通知
    setCurrentMachine(LOCAL_ORIGIN);
    setCurrentMachine("");
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("★ 一个订阅者抛异常，其余照样收到", () => {
    // 后果比白屏隐蔽：其余分节会停在上一台机器上，用户以为在看 devbox，其实在看 nano。
    const good1 = vi.fn();
    const good2 = vi.fn();
    subscribeMachine(good1);
    subscribeMachine(() => {
      throw new Error("BOOM");
    });
    subscribeMachine(good2);
    expect(() => setCurrentMachine("devbox")).not.toThrow();
    expect(good1).toHaveBeenCalledWith("devbox");
    expect(good2).toHaveBeenCalledWith("devbox");
  });

  it("退订之后不再收到（否则来回切页会越积越多订阅者）", () => {
    const fn = vi.fn();
    const off = subscribeMachine(fn);
    setCurrentMachine("devbox");
    off();
    setCurrentMachine("nano");
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("通知过程中退订不会漏掉/重复通知别人（遍历的是快照）", () => {
    const late = vi.fn();
    let off2: (() => void) | null = null;
    subscribeMachine(() => off2?.());
    off2 = subscribeMachine(late);
    setCurrentMachine("devbox");
    // 第一个订阅者在通知过程中退掉了第二个；因为遍历的是快照，第二个这一轮仍收到。
    expect(late).toHaveBeenCalledTimes(1);
    setCurrentMachine("nano");
    // 下一轮才真的不收
    expect(late).toHaveBeenCalledTimes(1);
  });
});
