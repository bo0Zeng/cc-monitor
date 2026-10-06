/**
 * 会话里的换号条与提示条：第一次见到的换号记录不回填进流；之后新出现的按发生那一刻追加在内容层末尾；卡住才出提示条。期望手写。
 */
import { describe, expect, it } from "vitest";

import { onSessionEntry } from "../../../src/frontend/ui/acct-session.ts";
import type { SessionRotationEntry } from "../../../src/frontend/ui/app-store.ts";
import type { SwitchRecord } from "../../../src/frontend/ui/generated/SwitchRecord.ts";

const NOW = 1_791_189_600;
const entry = (history: SwitchRecord[], blocked?: { account: string; at: number }): SessionRotationEntry => ({
  origin: "<local>",
  now: NOW,
  read: {
    state: "present",
    agent: "claude-code",
    follow: true,
    account: { start: "work", current: history.at(-1)?.to ?? "work", since: NOW, history, inPlace: "ok" },
    atLimit: "continue",
    ...(blocked ? { blocked: { earliest: blocked } } : {}),
    quota: { kind: "sub", state: blocked ? "refused" : "ok", stale: false, limiting: "5h", slots: [], login: "ok" },
  },
});

describe("换号条 · 提示条", () => {
  it("先有的不回填；新来的一条追加；from == to 的（跳过）不出条", () => {
    const content = document.createElement("div");
    const host = { streamContentOf: () => content, openPanel: () => {} };
    const h1: SwitchRecord = { at: NOW - 600, from: "work", to: "personal", why: { full: { w: "5h" } } };
    onSessionEntry("s1", entry([h1]), host);
    expect(content.querySelectorAll("[data-acct-strip]").length).toBe(0);
    const h2: SwitchRecord = { at: NOW, from: "personal", to: "team", why: "manualHot" };
    const skip: SwitchRecord = { at: NOW, from: "team", to: "team", why: { skipped: { account: "api", reason: "needsKey" } } };
    onSessionEntry("s1", entry([h1, h2, skip]), host);
    const strips = [...content.querySelectorAll<HTMLElement>("[data-acct-strip]")];
    expect(strips.length).toBe(1);
    expect(strips[0].textContent).toContain("personal → team");
    expect(strips[0].textContent).toContain("手动 · 热切换");
  });
  it("卡住 ⇒ 内容层最前面一条提示条；能发了 ⇒ 摘掉", () => {
    const content = document.createElement("div");
    content.appendChild(document.createElement("p"));
    const host = { streamContentOf: () => content, openPanel: () => {} };
    onSessionEntry("s2", entry([], { account: "team", at: Math.floor(Date.now() / 1000) + 5400 }), host); // 提示条按画的那一刻算「还有多久」
    expect(content.firstElementChild?.textContent).toMatch(/轮换内账号均已满 · 最早 team/);
    onSessionEntry("s2", entry([]), host);
    expect(content.textContent).not.toMatch(/轮换内账号均已满/);
  });
});
