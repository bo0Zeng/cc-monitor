/**
 * 答题块（`src/frontend/ui/answer-block.ts`）：会话在等批准 / 回答 / 批计划时，消息流末尾展开的那一块。
 *
 * - 只排版：选项的字、语气、要不要附话、题目，全照核心那一份 `needs.ask`（跨语言金样 `needs-ask.golden.json`，Rust 造）；
 * - 分组：允许那一族一行 · 拒绝那一族另起一行（只有「拒绝并停下」红字）· 改到终端里答排末尾、幽灵样式；
 * - 交互：点一项即答；要附话的原地换输入框（Enter 交 · Esc 回）；数字键只在焦点在块里时生效；出现不抢焦点；
 * - 别处先答了（already_answered / closed_elsewhere）⇒ 块淡出、toast 照核心那一句、重读；
 * - 只能在终端里答（`ask: null`）⇒ 核心那一句 ＋［看终端］。
 *
 * 夹具只造结构；字一律取自金样或文案表。
 */
import { readFileSync } from "node:fs";
vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));
import { terminalFrontAvailable } from "../../../src/frontend/ui/terminal-front";
import { resolve } from "node:path";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { AnswerBlock, type AnswerHost, type AnswerOutcome } from "../../../src/frontend/ui/answer-block";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { Ask, Needs } from "../../../src/frontend/ui/session-reads";
import { LIVE, LIVE_ATTACHABLE } from "../../../src/frontend/ui/tab-session-state";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { waitedNow } from "../../../src/frontend/ui/duration-format";
import { nonEmpty } from "../../test-support/non-empty";

const ASKS = JSON.parse(readFileSync(resolve(__dirname, "../../__fixtures__/needs-ask.golden.json"), "utf8")) as Record<"approve" | "subAgent" | "plan" | "question", Ask>;
const NOW = Date.parse("2026-10-11T12:00:00Z");
const waiting = { doing: "needs_you" as const, waitingFor: "permission prompt", text: copyText("beSession.activity.needsYou"), tone: "need", order: 1 };

function needs(ask: Ask | null, over: Partial<Needs> = {}): Needs {
  return { kind: "approve", tool: "Bash", call: "toolu_1", what: "w", sinceMs: NOW - 12_000, text: "核心·等批准", head: "核心·等批准 · Bash", headCode: null, tone: "need", rank: 1, waitedMs: 12_000, waitedText: "核心·12s", ask, manualText: ask ? null : "核心·只能在终端里答", receivedAt: NOW, ...over };
}

function tab(n: Needs | null, sid = "s1"): Tab {
  return { sessionId: sid, origin: LOCAL_ORIGIN, state: LIVE, activity: n ? waiting : null, needs: n } as unknown as Tab;
}

function rig(t: Tab | null, outcome: AnswerOutcome = { kind: "answered" }) {
  let cur = t;
  const answers: unknown[] = [];
  const host = {
    active: () => cur,
    answer: vi.fn(async (_t: Tab, req: unknown): Promise<AnswerOutcome> => {
      answers.push(req);
      return outcome;
    }),
    reread: vi.fn<(sid: string) => void>(),
    toast: vi.fn<(title: string, detail?: string) => void>(),
  } satisfies AnswerHost;
  const b = new AnswerBlock(host);
  document.body.append(b.el);
  b.render(NOW);
  return { b, host, answers, set: (x: Tab | null) => (cur = x) };
}

/** 按钮上的字（数字键那一枚键帽不算）。 */
const label = (x: Element) => [...x.childNodes].filter((n) => n.nodeName !== "KBD").map((n) => n.textContent).join("");
const rows = (b: AnswerBlock) => [...b.el.querySelectorAll<HTMLElement>("[data-row]")].map((r) => [r.dataset.row, [...r.querySelectorAll("button")].map(label)]);
const btn = (b: AnswerBlock, text: string) => [...b.el.querySelectorAll("button")].find((x) => label(x) === text)!;
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("甲1 批准一步", () => {
  it("★ 头一行是核心那一格 · 已等多久 · 命令全文不截 · 允许一族一行、拒绝一族另起一行，字一个不改", () => {
    const { b } = rig(tab(needs(ASKS.approve)));
    expect(b.el.hidden).toBe(false);
    expect(b.el.querySelector("[data-part=head]")!.textContent).toContain("核心·等批准 · Bash");
    expect(b.el.querySelector("[data-part=since]")!.textContent).toBe(copyText("askBlock.head.since", { waited: waitedNow(needs(ASKS.approve), NOW)! }));
    expect(b.el.querySelector("pre")!.textContent).toBe(ASKS.approve.detail);
    const c = ASKS.approve.choices.map((x) => x.text);
    expect(rows(b)).toEqual([
      ["main", [c[0], c[1], c[2]]],
      ["deny", [c[3], copyText("askBlock.choice.more", { label: c[4] }), c[5]]],
    ]);
  });

  it("★ 一色一义：主选项只第一颗、红字只「拒绝并停下」那一颗；数字键标在允许那一行", () => {
    const { b } = rig(tab(needs(ASKS.approve)));
    const kinds = [...b.el.querySelectorAll<HTMLButtonElement>("button[data-choice]")].map((x) => [x.dataset.choice, x.dataset.kind, x.dataset.key ?? "", x.querySelector("kbd")?.textContent ?? ""]);
    expect(kinds).toEqual([
      ["allow", "primary", "1", "1"],
      ["always-0", "secondary", "2", "2"],
      ["always-1", "secondary", "3", "3"],
      ["deny", "secondary", "", ""],
      ["deny-note", "secondary", "", ""],
      ["deny-stop", "danger-text", "", ""],
    ]);
  });

  it("★ 点一项即答：交回核心给的 ask id 与选项 id；答出去后等重读（块还在、按钮进行中不可再点）", async () => {
    const { b, answers } = rig(tab(needs(ASKS.approve)));
    btn(b, ASKS.approve.choices[1].text).click();
    await flush();
    expect(answers).toEqual([{ ask: ASKS.approve.id, choice: "always-0" }]);
    btn(b, ASKS.approve.choices[0].text).click();
    await flush();
    expect(answers.length, "答出去了 ⇒ 不再交第二次").toBe(1);
  });

  it("★ 数字键只在焦点在块里时生效；块出现不抢焦点", async () => {
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    const { b, answers } = rig(tab(needs(ASKS.approve)));
    expect(document.activeElement, "出现不抢焦点").toBe(outside);
    outside.dispatchEvent(new KeyboardEvent("keydown", { key: "2", bubbles: true }));
    await flush();
    expect(answers).toEqual([]);
    btn(b, ASKS.approve.choices[0].text).focus();
    b.el.querySelector("button")!.dispatchEvent(new KeyboardEvent("keydown", { key: "3", bubbles: true }));
    await flush();
    expect(answers).toEqual([{ ask: ASKS.approve.id, choice: "always-1" }]);
  });

  it("★ 不在等了（没挂请求的那一种）⇒ 收起、不留痕；换成下一条 ⇒ 换上来（挂着请求的被别处答掉见甲7）", () => {
    const r = rig(tab(needs(null)));
    r.set(tab(null));
    r.b.render(NOW);
    expect([r.b.el.hidden, r.b.el.childElementCount]).toEqual([true, 0]);
    r.set(tab(needs(ASKS.plan, { kind: "plan", head: "核心·等批准计划" })));
    r.b.render(NOW);
    expect(r.b.el.querySelector("[data-part=head]")!.textContent).toContain("核心·等批准计划");
  });
});

describe("甲2 拒绝并说明（note: required）", () => {
  it("★ 原地换成输入框：Enter 交、带上那句话；空的不交；Esc 回到选项", async () => {
    const { b, answers } = rig(tab(needs(ASKS.approve)));
    btn(b, copyText("askBlock.choice.more", { label: ASKS.approve.choices[4].text })).click();
    const input = b.el.querySelector<HTMLInputElement>("input[data-note]")!;
    expect(input).not.toBeNull();
    expect(document.activeElement).toBe(input);
    expect(b.el.querySelector("[data-row=main]"), "选项那两行收起").toBeNull();
    expect([...b.el.querySelectorAll("button")].map(label)).toEqual([ASKS.approve.choices[4].text, copyText("askBlock.note.cancel")]);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(answers, "必附话的空着不交").toEqual([]);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(b.el.querySelector("input[data-note]")).toBeNull();
    expect(b.el.querySelector("[data-row=main]")).not.toBeNull();
    btn(b, copyText("askBlock.choice.more", { label: ASKS.approve.choices[4].text })).click();
    const again = b.el.querySelector<HTMLInputElement>("input[data-note]")!;
    again.value = "先只删 tmp";
    again.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(answers).toEqual([{ ask: ASKS.approve.id, choice: "deny-note", note: "先只删 tmp" }]);
  });
});

describe("甲3 回答提问", () => {
  const q = () => rig(tab(needs(ASKS.question, { kind: "answer", head: "核心·等回答" })));

  it("★ 每题照原样排：题头（多选的标多选）· 题目 · 选项与说明 · 末尾一格「其他…」；头一行带几题", () => {
    const { b } = q();
    expect(b.el.querySelector("[data-part=head]")!.textContent).toContain(copyText("askBlock.head.questions", { head: "核心·等回答", n: 2 }));
    const qs = [...b.el.querySelectorAll<HTMLElement>("[data-question]")];
    expect(qs.map((x) => x.querySelector("[data-part=qhead]")?.textContent)).toEqual([ASKS.question.questions[0].header, copyText("askBlock.question.multi", { header: ASKS.question.questions[1].header! })]);
    expect(qs.map((x) => x.querySelector("[data-part=qtext]")?.textContent)).toEqual(ASKS.question.questions.map((x) => x.text));
    expect(qs[0].querySelectorAll("[data-option]").length).toBe(ASKS.question.questions[0].options.length);
    expect(qs[0].textContent).toContain("warm");
    expect(qs[0].querySelector<HTMLInputElement>("input[data-other]")!.placeholder).toBe(copyText("askBlock.question.other"));
    expect(b.el.querySelector("pre"), "提问那一种不另出全文块").toBeNull();
    expect(rows(b)).toEqual([["main", ASKS.question.choices.map((x) => x.text)]]);
  });

  it("★ 每题都答了「提交答案」才可点；单选换着点只留一项；多选可几项；自由文本算答；按题 id 交 picks", async () => {
    const { b, answers } = q();
    const submit = btn(b, ASKS.question.choices[0].text);
    expect(submit.getAttribute("aria-disabled")).toBe("true");
    const [q0, q1] = [...b.el.querySelectorAll<HTMLElement>("[data-question]")];
    const opt = (el: HTMLElement, i: number) => el.querySelectorAll<HTMLElement>("[data-option]")[i];
    opt(q0, 0).click();
    opt(q0, 1).click();
    expect([...q0.querySelectorAll("[data-option][aria-checked=true]")].length, "单选只留一项").toBe(1);
    expect(submit.getAttribute("aria-disabled")).toBe("true");
    opt(q1, 0).click();
    opt(q1, 1).click();
    expect(submit.getAttribute("aria-disabled")).toBeNull();
    submit.click();
    await flush();
    expect(answers).toEqual([{ ask: ASKS.question.id, choice: "answer", picks: { q0: { options: ["q0o1"], text: "" }, q1: { options: ["q1o0", "q1o1"], text: "" } } }]);
  });

  it("单选写了自由文本 ⇒ 不再算选了哪一项；只写自由文本也算答", async () => {
    const { b, answers } = q();
    const [q0, q1] = [...b.el.querySelectorAll<HTMLElement>("[data-question]")];
    q0.querySelectorAll<HTMLElement>("[data-option]")[0].click();
    const other = q0.querySelector<HTMLInputElement>("input[data-other]")!;
    other.value = "PURPLE";
    other.dispatchEvent(new Event("input", { bubbles: true }));
    const o1 = q1.querySelector<HTMLInputElement>("input[data-other]")!;
    o1.value = "Kiwi";
    o1.dispatchEvent(new Event("input", { bubbles: true }));
    btn(b, ASKS.question.choices[0].text).click();
    await flush();
    expect(answers).toEqual([{ ask: ASKS.question.id, choice: "answer", picks: { q0: { options: [], text: "PURPLE" }, q1: { options: [], text: "Kiwi" } } }]);
  });
});

describe("甲4 批计划 · 甲5 子 agent", () => {
  it("★ 批计划：四项一行（核心给的序与字），附话可选的那一项点开输入框、空着也能交", async () => {
    const { b, answers } = rig(tab(needs(ASKS.plan, { kind: "plan", head: "核心·等批准计划" })));
    const c = ASKS.plan.choices;
    expect(rows(b)).toEqual([["main", [c[0].text, c[1].text, c[2].text, copyText("askBlock.choice.more", { label: c[3].text })]]]);
    expect(b.el.querySelector("pre")!.textContent).toBe(ASKS.plan.detail);
    btn(b, copyText("askBlock.choice.more", { label: c[3].text })).click();
    b.el.querySelector<HTMLInputElement>("input[data-note]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(answers).toEqual([{ ask: ASKS.plan.id, choice: "plan-keep" }]);
  });

  it("★ 子 agent：头上标是哪一种 agent；「改到终端里答」排末尾、幽灵样式", () => {
    const { b } = rig(tab(needs(ASKS.subAgent)));
    expect(b.el.querySelector("[data-part=tag]")!.textContent).toBe(copyText("askBlock.tag.run", { kind: "general-purpose" }));
    const last = [...b.el.querySelectorAll<HTMLButtonElement>("button[data-choice]")].at(-1)!;
    expect([last.dataset.choice, last.dataset.kind, label(last)]).toEqual(["release", "ghost", ASKS.subAgent.choices.at(-1)!.text]);
    expect(b.el.querySelector("[data-row=release]")).not.toBeNull();
  });

  it("子 agent 没说种类 ⇒ 只标 agent", () => {
    const { b } = rig(tab(needs({ ...ASKS.subAgent, run: { id: "a1", kind: null } })));
    expect(b.el.querySelector("[data-part=tag]")!.textContent).toBe(copyText("askBlock.tag.runBare"));
  });
});

describe("甲6 只能在终端里答 · 甲7 别处先答了", () => {
  it("★ 没挂上请求 ⇒ 只写核心那一句（为什么在这里答不了），块里一颗按钮都没有（终端只在「⋯」里当逃生口）", () => {
    const { b } = rig(tab(needs(null, { head: "核心·等批准 · 信任工作区" })));
    expect(b.el.querySelector("[data-part=manual]")!.textContent).toBe("核心·只能在终端里答");
    expect(b.el.querySelectorAll("button").length).toBe(0);
  });

  it("★ 点了才知道别处先答了 ⇒ 块淡出、按钮不可点、下面留一行核心那一句、随后重读；重读把请求收了 ⇒ 块收起、那一行还在", async () => {
    const r = rig(tab(needs(ASKS.approve)), { kind: "gone", said: "核心·已在终端答过或已关闭" });
    btn(r.b, ASKS.approve.choices[0].text).click();
    await flush();
    expect(r.b.el.dataset.gone).toBe("true");
    expect([...r.b.el.querySelectorAll("button")].every((x) => x.getAttribute("aria-disabled") === "true")).toBe(true);
    expect(r.b.el.querySelector("[data-part=gone]")!.textContent).toBe("核心·已在终端答过或已关闭");
    expect(r.host.toast, "那一句留在块里，不另弹 toast").not.toHaveBeenCalled();
    expect(r.host.reread).toHaveBeenCalledWith("s1");
    r.set(tab(null));
    r.b.render(NOW);
    expect(r.b.el.hidden).toBe(false);
    expect(r.b.el.querySelectorAll("button").length, "块收起").toBe(0);
    expect(r.b.el.querySelector("[data-part=gone]")!.textContent).toBe("核心·已在终端答过或已关闭");
  });

  it("★ 没点、请求在别处被答掉（重读里没了它）⇒ 块收起、留一行「已在别处答过」；下一条请求来了 ⇒ 换上来；切走 ⇒ 不留", () => {
    const r = rig(tab(needs(ASKS.approve)));
    r.set(tab(null));
    r.b.render(NOW);
    expect([r.b.el.hidden, r.b.el.querySelectorAll("button").length]).toEqual([false, 0]);
    expect(r.b.el.querySelector("[data-part=gone]")!.textContent).toBe(copyText("askBlock.gone.elsewhere"));
    r.b.render(NOW + 5_000);
    expect(r.b.el.querySelector("[data-part=gone]"), "一直留着").not.toBeNull();
    r.set(tab(needs(ASKS.plan, { kind: "plan" })));
    r.b.render(NOW);
    expect(r.b.el.querySelector("[data-part=gone]")).toBeNull();
    expect(r.b.el.querySelectorAll("button[data-choice]").length).toBe(ASKS.plan.choices.length);
    r.set(tab(null));
    r.b.render(NOW);
    r.set(tab(null, "s2"));
    r.b.render(NOW);
    expect([r.b.el.hidden, r.b.el.childElementCount], "切到别的会话 ⇒ 不留").toEqual([true, 0]);
  });

  it("★ 没人答、那台替人答了 ⇒ 留的那一行是核心那一句（不是「已在别处答过」）；切走再回来还在；下一条请求来了才换掉", () => {
    const said = "核心·10 分钟没人答，已自动答：继续";
    const auto = (n: Needs | null, sid = "s1"): Tab => ({ ...tab(n, sid), autoAnswered: n ? null : said }) as Tab;
    const r = rig(auto(needs(ASKS.question, { kind: "answer" })));
    r.set(auto(null));
    r.b.render(NOW);
    expect([r.b.el.hidden, r.b.el.querySelectorAll("button").length]).toEqual([false, 0]);
    expect(r.b.el.querySelector("[data-part=gone]")!.textContent).toBe(said);
    r.set(tab(null, "s2"));
    r.b.render(NOW);
    expect(r.b.el.hidden).toBe(true);
    r.set(auto(null));
    r.b.render(NOW);
    expect(r.b.el.querySelector("[data-part=gone]")!.textContent, "切回来照会话事实那一格画").toBe(said);
    r.set(auto(needs(ASKS.plan, { kind: "plan" })));
    r.b.render(NOW);
    expect(r.b.el.querySelector("[data-part=gone]")).toBeNull();
  });

  it("★ 自己答出去的那一条被收了 ⇒ 块收起、不留那一行", async () => {
    const r = rig(tab(needs(ASKS.approve)));
    btn(r.b, ASKS.approve.choices[0].text).click();
    await flush();
    r.set(tab(null));
    r.b.render(NOW);
    expect([r.b.el.hidden, r.b.el.childElementCount]).toEqual([true, 0]);
  });

  it("没答成（别的原因）⇒ 块照旧可答、说一声", async () => {
    const { b, host, answers } = rig(tab(needs(ASKS.approve)), { kind: "failed", said: "核心·该项不可选", detail: "d" });
    btn(b, ASKS.approve.choices[0].text).click();
    await flush();
    expect(host.toast).toHaveBeenCalledWith("核心·该项不可选", "d");
    expect(b.el.dataset.gone).toBeUndefined();
    btn(b, ASKS.approve.choices[0].text).click();
    await flush();
    expect(answers.length).toBe(2);
  });
});

describe("入口：跳到等批准的会话 ⇒ 焦点进块", () => {
  it("★ focusFor：块已画 ⇒ 焦点落在第一颗；还没到（会话事实在路上）⇒ 画出来那一刻再落", () => {
    const r = rig(tab(null));
    r.b.focusFor("s1");
    expect(document.activeElement).toBe(document.body);
    r.set(tab(needs(ASKS.approve)));
    r.b.render(NOW);
    expect(document.activeElement).toBe(btn(r.b, ASKS.approve.choices[0].text));
    // 之后再画不再抢。
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    r.set(tab(needs(ASKS.plan, { kind: "plan" })));
    r.b.render(NOW);
    expect(document.activeElement).toBe(outside);
  });
});

describe("每一种需手动都有地方出（钉条删了以后）", () => {
  it("★ 核心能出的每一种 needs.kind（读 Rust 那份枚举）在桌面都画成块：头一行 ＋ 那一句（没有按钮）", () => {
    const src = readFileSync(resolve(__dirname, "../../../src/common/cmd-wire/src/needs.rs"), "utf8");
    const body = src.slice(src.indexOf("pub enum NeedsKind {"));
    const variants = [...body.slice(0, body.indexOf("}")).matchAll(/^\s*([A-Z][A-Za-z]*),/gm)].map((m) => m[1][0].toLowerCase() + m[1].slice(1));
    nonEmpty(variants);
    for (const kind of variants) {
      document.body.innerHTML = "";
      const { b } = rig(tab(needs(null, { kind: kind as Needs["kind"], head: `核心·${kind}`, manualText: `核心·${kind}·那一句` })));
      expect(b.el.hidden, kind).toBe(false);
      expect(b.el.querySelector("[data-part=head]")!.textContent, kind).toContain(`核心·${kind}`);
      expect(b.el.querySelector("[data-part=manual]")!.textContent, kind).toBe(`核心·${kind}·那一句`);
      expect(b.el.querySelectorAll("button").length, kind).toBe(0);
    }
  });

  it("★ 会话事实还没到（只有活动信号说在等）⇒ 也出：头一行是活动信号那一句", () => {
    const t = { sessionId: "s1", origin: LOCAL_ORIGIN, state: LIVE, activity: waiting, needs: null } as unknown as Tab;
    const { b } = rig(t);
    expect(b.el.hidden).toBe(false);
    expect(b.el.querySelector("[data-part=head]")!.textContent).toContain(waiting.text);
  });

  it("★ 能不能去终端都一样：远端在 tmux 里 · Windows 有 ↗ · 都不行 ⇒ 块里都不放终端那几颗主按钮", () => {
    const remote = { ...tab(needs(null)), origin: "devbox", state: LIVE_ATTACHABLE } as unknown as Tab;
    expect(rig(remote).b.el.querySelectorAll("button").length).toBe(0);
    document.body.innerHTML = "";
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    expect(rig(tab(needs(null))).b.el.querySelectorAll("button").length).toBe(0);
    vi.mocked(terminalFrontAvailable).mockReturnValue(false);
  });
});
