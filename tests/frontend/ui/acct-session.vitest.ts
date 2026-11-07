/**
 * 会话里的换号条与停发提示（`acct-session.ts`）：照这个会话的 `rotation-view`（appStore）画；字是后端写好的。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";
import { __resetSeenLogForTests, onSessionEntry } from "../../../src/frontend/ui/acct-session";
import { holdOf } from "../../../src/frontend/ui/rot-words";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { SessionRotationEntry } from "../../../src/frontend/ui/app-store";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const NOW = 1_800_000_000;
const read = (over: Partial<RotationViewReply> = {}): RotationViewReply => ({
  now: NOW,
  nowText: "14:30", title: null,
  strategy: null,
  board: { all: [], rows: [] },
  start: "work",
  current: "team",
  lineup: [],
  manual: false,
  off: [],
  accounts: [],
  listed: 0,
  decision: null,
  log: [],
  hints: [],
  ...over,
});
const entry = (r: RotationViewReply): SessionRotationEntry => ({ origin: "<local>", read: r });

let content: HTMLElement;
const host = { streamContentOf: () => content, openPanel: vi.fn() };

beforeEach(() => {
  __resetSeenLogForTests();
  content = document.createElement("div");
  document.body.replaceChildren(content);
});

describe("换号条", () => {
  it("★ 第一次见到的记录不回填；之后新来的一条 ⇒ 流末尾一行 `work → team · 为什么`（为什么照后端配好的那一句）", () => {
    const old = { at: NOW - 600, from: "work", to: "team", cause: { k: "order" as const }, because: "旧的那一次" };
    onSessionEntry("s1", entry(read({ log: [old] })), host);
    expect(content.querySelector("[data-acct-strip]")).toBeNull();
    const fresh = { at: NOW, from: "team", to: "lab", cause: { k: "unusable" as const }, because: "后端配好的为什么" };
    onSessionEntry("s1", entry(read({ log: [old, fresh] })), host);
    const strips = content.querySelectorAll<HTMLElement>("[data-acct-strip]");
    expect(strips).toHaveLength(1);
    expect(strips[0].textContent).toContain(copyText("rot.strip.move", { from: "team", to: "lab" }));
    expect(strips[0].textContent).toContain("后端配好的为什么");
    [...strips[0].querySelectorAll("button")].find((b) => b.textContent === copyText("acct.menu.open"))!.click();
    expect(host.openPanel).toHaveBeenCalledWith("s1", "<local>");
  });
});

describe("停发提示", () => {
  const hold = read({ hints: [{ hint: { k: "hold", until: NOW + 3600, account: "lab" }, tone: "warn", text: "停发 · 15:30 lab · {dur}" }] });

  it("★ 停发 ⇒ 流最前一条（字照后端那一句，{dur} 填上）；不停了 ⇒ 摘掉", () => {
    onSessionEntry("s1", entry(hold), host);
    const notice = content.querySelector<HTMLElement>("[data-acct-notice]");
    expect(notice?.textContent).toContain("15:30 lab");
    expect(notice?.textContent).not.toContain("{dur}");
    onSessionEntry("s1", entry(read()), host);
    expect(content.querySelector("[data-acct-notice]")).toBeNull();
  });

  it("holdOf：没停发 ⇒ null；标签页的悬停照它", () => {
    expect(holdOf(read())).toBeNull();
    expect(holdOf(null)).toBeNull();
    expect(holdOf(hold)).toContain("15:30 lab");
  });
});

/** 换号条里长号名 · 长原因不伸出流宽（10-19 用户：文本超出去）。门禁里没有排版引擎，这里钉住让截断成立的那几条样式。 */
describe("换号条长字截断", () => {
  const css = (sel: string): Map<string, string> => {
    const out = new Map<string, string>();
    postcss.parse(readFileSync(resolve(__dirname, "../../../src/frontend/ui/acct-session.module.css"), "utf8")).walkRules((r: Rule) => {
      if (r.selectors.map((x) => x.trim()).includes(sel)) r.walkDecls((d) => void out.set(d.prop, d.value));
    });
    return out;
  };

  it("★ 号名那一段与原因那一段：单行省略（one-line）· 能缩到比字窄；装它们的那一格也能缩", () => {
    for (const cls of ["acctSessStripMove", "acctSessStripWhy"]) {
      expect(css(`.${cls}`).get("composes"), cls).toBe("one-line from global");
      const sel = `.acctSessStripText > .${cls}`;
      const d = css(sel);
      expect(d.get("min-width"), sel).toBe("0");
      expect(d.get("flex"), sel).toMatch(/^0 [1-9]\d* auto$/);
    }
    expect(css(".acctSessStripText").get("min-width")).toBe("0");
  });
});
