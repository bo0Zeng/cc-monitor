/**
 * G6：分叉起会话前的追问小窗。
 *
 * 三条要紧的：
 * ① **取消 ≠ 空答案** —— 取消要 resolve `null`，不能退化成 `{}`（那会被当成"确认默认值"照起）
 * ② 账号默认落**账号 0**，不是"当前账号"——默认值会被大量用户直接确认掉
 * ③ 只渲染真的要问的那几格
 */
import { describe, it, expect, beforeEach } from "vitest";
import { askForkLaunch, ACCOUNT_ZERO_VALUE } from "./fork-ask";
import type { ForkLaunchFacts } from "./fork-launch";

const FACTS: ForkLaunchFacts = {
  cwd: { kind: "known", value: "/home/u/p", from: "会话记录里的 cwd" },
  account: { kind: "unknown", why: "源会话已退出：账号只记在 pidfile 里" },
  tmux: { kind: "unknown", why: "源会话已退出：它当初在不在 tmux 里无从查起" },
};

const ACCOUNTS = [
  { name: "z", configDir: "/home/u/.claude-accts/z" },
  { name: "b", configDir: "/home/u/.claude-accts/b" },
];

const q = <T extends Element>(sel: string): T => {
  const el = document.querySelector<T>(sel);
  if (!el) throw new Error(`没找到 ${sel}`);
  return el;
};

describe("askForkLaunch", () => {
  beforeEach(() => document.body.replaceChildren());

  it("只渲染要问的那几格（cwd 已知 → 不出现输入框）", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["account", "tmux"],
      accounts: ACCOUNTS,
      defaultUseTmux: true,
    });
    expect(document.querySelector(".fork-ask-account")).not.toBeNull();
    expect(document.querySelector(".fork-ask-tmux")).not.toBeNull();
    expect(document.querySelector(".fork-ask-cwd"), "cwd 已知就别问").toBeNull();
    q<HTMLButtonElement>(".fork-ask-cancel").click();
    await p;
  });

  /** ★★ 默认值会被大量用户直接确认掉，所以默认位上不许摆「当前账号」。 */
  it("★★ 账号默认落在账号 0（不注入），列表里的账号都不是默认", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: ACCOUNTS,
      defaultUseTmux: false,
    });
    const sel = q<HTMLSelectElement>(".fork-ask-account");
    expect(sel.value).toBe(ACCOUNT_ZERO_VALUE);
    expect(sel.options[0].textContent).toContain("账号 0");
    q<HTMLButtonElement>(".fork-ask-ok").click();
    expect((await p)?.configDir, "账号 0 ⇒ null，不是空串").toBeNull();
  });

  it("选了某个账号 → 回它的 configDir", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: ACCOUNTS,
      defaultUseTmux: false,
    });
    const sel = q<HTMLSelectElement>(".fork-ask-account");
    sel.value = "/home/u/.claude-accts/b";
    q<HTMLButtonElement>(".fork-ask-ok").click();
    expect((await p)?.configDir).toBe("/home/u/.claude-accts/b");
  });

  /** ★★ 取消必须是 `null`。`{}` 会被 `startForkedSession` 当成"用户确认了默认值"照常起。 */
  it("★★ 取消 → null（三条路都是：按钮 / Esc / 点背景）", async () => {
    for (const how of ["button", "esc", "backdrop"] as const) {
      const p = askForkLaunch({
        facts: FACTS,
        slots: ["account"],
        accounts: ACCOUNTS,
        defaultUseTmux: false,
      });
      if (how === "button") q<HTMLButtonElement>(".fork-ask-cancel").click();
      else if (how === "esc")
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      else q<HTMLElement>(".fork-ask-backdrop").click();
      expect(await p, `${how} 该取消`).toBeNull();
    }
  });

  it("★ 点弹窗本体（不是背景）不算取消", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: ACCOUNTS,
      defaultUseTmux: false,
    });
    q<HTMLElement>(".fork-ask").click();
    expect(document.querySelector(".fork-ask-backdrop"), "窗还在").not.toBeNull();
    q<HTMLButtonElement>(".fork-ask-ok").click();
    expect(await p).not.toBeNull();
  });

  it("tmux 复选框的初始态由调用方给（远端默认勾上）", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["tmux"],
      accounts: [],
      defaultUseTmux: true,
    });
    expect(q<HTMLInputElement>(".fork-ask-tmux").checked).toBe(true);
    q<HTMLButtonElement>(".fork-ask-ok").click();
    expect((await p)?.useTmux).toBe(true);
  });

  it("★ 把推断层给的「为什么要问」原样端出来，不在这儿另写一套说法", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: [],
      defaultUseTmux: false,
    });
    expect(q<HTMLElement>(".fork-ask-why").textContent).toBe(
      FACTS.account.kind === "unknown" ? FACTS.account.why : "",
    );
    q<HTMLButtonElement>(".fork-ask-cancel").click();
    await p;
  });

  it("账号列表为空（账号功能没启用）→ 仍能选账号 0，不把整条路堵死", async () => {
    const p = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: [],
      defaultUseTmux: false,
    });
    expect(q<HTMLSelectElement>(".fork-ask-account").options).toHaveLength(1);
    q<HTMLButtonElement>(".fork-ask-ok").click();
    expect((await p)?.configDir).toBeNull();
  });

  it("关掉之后 DOM 不留残渣（含 keydown 监听——连开两次不会互相干扰）", async () => {
    const p1 = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: [],
      defaultUseTmux: false,
    });
    q<HTMLButtonElement>(".fork-ask-cancel").click();
    await p1;
    expect(document.querySelector(".fork-ask-backdrop")).toBeNull();

    const p2 = askForkLaunch({
      facts: FACTS,
      slots: ["account"],
      accounts: [],
      defaultUseTmux: false,
    });
    q<HTMLButtonElement>(".fork-ask-ok").click();
    expect(await p2, "第一次的监听若没摘，这次 ok 可能被它抢先 resolve 成 null").not.toBeNull();
  });
});
