// `K-R100` `KR100D3` 的**下游**判据：「有结果但没显示」不许与「真的没有结果」同形。
//
// 🔴 收口前的形状（本文件锁的就是它不许回来）：
//   · daemon 的 `--search` 被自己的 `--limit` 砍掉 snippet 后，wire 上只剩
//     `hitCount: 12, hits: []` —— **没有任何字段说「我被砍了」**；
//   · monitor 合并时逐字 `truncated: local.truncated` ⇒ **远端那一半整个丢掉**，
//     状态行照旧只报总数；
//   · 卡片里那行「…还有 N 条命中（点任意条打开会话查看全部）」在 `hits: []` 时
//     指向**一个不存在的东西** —— 整张卡一条可点的行都没有。
//
// 所以这里逐条锁三种形状**各自该长什么样**：
//   (A) 真的没有命中           → "无匹配"，一张卡都没有、一个入口都没有
//   (B) 有命中、全给了 snippet → 有命中行，**没有**截断提示
//   (C) 有命中、预算砍了       → 说得出「预算用完」**且点得开**（`hits: []` 时那一行是唯一入口）
// ⚠ **刻意不写「三者两两不同」那种形状** —— 见文件末尾那条注释：它在死值验里没死。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("./session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_c: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runNewSessionRemote: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../format", () => ({ formatTimestampSmart: () => "时间" }));

import { HistoryView } from "./history";

type AnySession = Record<string, unknown>;

function hit(uuid = "u1"): Record<string, unknown> {
  return { uuid, tsMs: 1, kind: "user", before: "b", matched: "m", after: "a" };
}
function session(over: AnySession = {}): AnySession {
  return {
    sessionId: "s1",
    projectPath: "/p",
    projectName: "P",
    jsonlPath: "/p/s1.jsonl",
    title: "T",
    updatedAt: 1,
    hitCount: 1,
    hits: [hit()],
    hitsTruncated: false,
    ...over,
  };
}
function response(over: AnySession = {}): AnySession {
  return {
    status: "ready",
    totalHits: 1,
    sessionCount: 1,
    truncated: false,
    indexedSessions: 1,
    indexedMessages: 1,
    sessions: [session()],
    ...over,
  };
}

/** 渲染整份响应，返回 (状态行文本, 结果区元素)。 */
function render(view: HistoryView, resp: AnySession): { status: string; results: HTMLElement } {
  const v = view as unknown as {
    renderSearchResults(r: unknown, q: string): void;
    statusEl: HTMLElement;
    resultsEl: HTMLElement;
  };
  v.renderSearchResults(resp, "docker");
  return { status: v.statusEl.textContent ?? "", results: v.resultsEl };
}
function card(view: HistoryView, s: AnySession): HTMLElement {
  return (view as unknown as { buildSearchSession(s: unknown): HTMLElement }).buildSearchSession(s);
}

describe("K-R100 KR100D3：截断说得出话，且与「真的没有结果」分得开", () => {
  let view: HistoryView;
  beforeEach(() => {
    document.body.replaceChildren();
    view = new HistoryView();
  });

  it("(A) 真的没有命中 → 「无匹配」，一张卡都没有", () => {
    const { status, results } = render(view, response({ totalHits: 0, sessionCount: 0, sessions: [] }));
    expect(results.textContent).toContain("无匹配");
    expect(results.querySelectorAll(".search-session")).toHaveLength(0);
    expect(status).not.toContain("预算");
  });

  it("(B) 有命中、全给了 snippet → 有命中行，没有任何截断提示", () => {
    const { status, results } = render(view, response());
    expect(results.querySelectorAll(".search-hit").length).toBeGreaterThan(0);
    expect(results.querySelector(".search-hit-more")).toBeNull();
    expect(status).not.toContain("预算");
  });

  it("(C) 远端被预算砍了 → 状态行说得出，卡片说得出，且点得开", () => {
    const remote = session({
      sessionId: "rem",
      origin: "pi",
      hitCount: 12,
      hits: [], // 🔴 收口前这就是全部信息：一个空数组
      hitsTruncated: true,
    });
    const { status, results } = render(
      view,
      response({ totalHits: 12, sessionCount: 1, truncated: true, sessions: [remote] }),
    );

    // ① 截断而不说 ⇒ 红。状态行必须点名。
    expect(status).toContain("预算");
    expect(status).toContain("1 个会话只列了标题");

    // ② 卡片自己也必须说，而且不许再写「点任意条」——那时候一条都没有。
    const more = results.querySelector<HTMLElement>(".search-hit-more");
    expect(more).toBeTruthy();
    expect(more!.textContent).toContain("预算已用完");
    expect(more!.textContent).not.toContain("点任意条");
    expect(more!.classList.contains("search-hit-more-truncated")).toBe(true);

    // ③ 🔴 点得开：`hits: []` 时这一行是**唯一**入口。
    expect(results.querySelectorAll(".search-hit")).toHaveLength(0);
    more!.click();
    expect((view as unknown as { viewer: unknown }).viewer).not.toBeNull();
  });

  it("🔴 「预算砍的」与「本会话只列前 N 条」文案不同 —— 一个值不许再装两件事", () => {
    const starved = card(view, session({ hitCount: 40, hits: [hit()], hitsTruncated: true }));
    const capped = card(view, session({ hitCount: 40, hits: [hit()], hitsTruncated: false }));
    const a = starved.querySelector(".search-hit-more")!.textContent!;
    const b = capped.querySelector(".search-hit-more")!.textContent!;
    expect(a).not.toEqual(b);
    expect(a).toContain("预算已用完");
    expect(b).not.toContain("预算");
    // 一个是「你的结果被砍了」，一个是「这个会话话多」——只有前者该染色出声。
    expect(starved.querySelector(".search-hit-more-truncated")).toBeTruthy();
    expect(capped.querySelector(".search-hit-more-truncated")).toBeNull();
  });

  // ⚠ 一条**写过又改掉**的判据，留着经过：第一版是「三种形状序列化后两两不同」。
  // 它在死值验里**没死** —— 三份 fixture 的 `hitCount`/卡片数本来就不同，
  // 于是「两两不同」在退回收口前的渲染时照样成立。那是一句真话摆错了格。
  // ⇒ 换成下面这条：分得开的**那一维**是「用户被告知了什么 · 够不够得到那些结果」。
  it("🔴 (C) 与 (A) 在「够不够得到那些结果」这一维上必须不同", () => {
    // (A) 真的没有命中：没有卡、没有任何可点开会话的入口
    const va = new HistoryView();
    const a = render(va, response({ totalHits: 0, sessionCount: 0, sessions: [] }));
    expect(a.results.querySelectorAll(".search-session")).toHaveLength(0);
    expect(a.results.querySelector(".search-hit, .search-hit-more")).toBeNull();

    // (C) 有 12 条命中、一条 snippet 都没显示：必须**说得出**、且**够得到**
    const vc = new HistoryView();
    const c = render(
      vc,
      response({
        totalHits: 12,
        truncated: true,
        sessions: [session({ hitCount: 12, hits: [], hitsTruncated: true })],
      }),
    );
    expect(c.status).toContain("预算");
    const entry = c.results.querySelector<HTMLElement>(".search-hit-more");
    expect(entry).toBeTruthy();
    // 🔴 卡片里一条命中行都没有 ⇒ 这一行是**唯一**入口，它必须真的能打开会话。
    expect(c.results.querySelectorAll(".search-hit")).toHaveLength(0);
    expect(entry!.textContent).not.toContain("点任意条");
    entry!.click();
    expect((vc as unknown as { viewer: unknown }).viewer).not.toBeNull();
  });
});
