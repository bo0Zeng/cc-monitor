/**
 * 查看器跟着长（在跑的会话）：先订 `session-lines/<sid>`、再读；读完流里来的接在后面（按 `seq` 去重）；
 * 在底部跟着贴底、往上翻了出「↓ 新内容」；底一行只在真订着流、会话在跑时说「运行中 · 实时」；起停交宿主。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));
// 流那一侧：记下订的是谁、把 sink 交出来（流怎么翻成事件住 `events.ts`，另有判据）。
const follow = vi.hoisted(() => ({
  calls: [] as { origin: string; sid: string }[],
  sink: null as ((e: unknown) => void) | null,
  stopped: 0,
  order: [] as string[],
}));
vi.mock("../../../../src/frontend/ui/events", () => ({
  followSession: vi.fn(async (origin: string, sid: string, sink: (e: unknown) => void) => {
    follow.calls.push({ origin, sid });
    follow.order.push("subscribe");
    follow.sink = sink;
    return { stop: () => void (follow.stopped += 1) };
  }),
}));
vi.mock("../../../../src/frontend/ui/record-reads", async (orig) => {
  const real = await orig<typeof import("../../../../src/frontend/ui/record-reads")>();
  return {
    ...real,
    readRange: vi.fn(async (o: string, p: string, a: number, b: number, seq: number) => {
      follow.order.push("read");
      return real.readRange(o as never, p, a, b, seq);
    }),
    readLines: vi.fn(async () => ({ from: 0, next: 0, eof: true, payloads: [] })),
  };
});

import { installViewerRig, userLine, assistantLine, viewerRig, type RigPayload } from "../../../test-support/session-viewer-rig";
import { SessionViewer, type ViewerOptions } from "../../../../src/frontend/ui/views/session-viewer";
import { readLines } from "../../../../src/frontend/ui/record-reads";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const cards = (v: SessionViewer): string[] =>
  [...v.element.querySelectorAll<HTMLElement>("[data-id]")].map((e) => e.dataset.id ?? "");
const status = (v: SessionViewer): string => v.element.querySelector('[data-role="status"]')?.textContent ?? "";
const pill = (v: SessionViewer): HTMLButtonElement => v.element.querySelector<HTMLButtonElement>('[data-role="new-content"]')!;
const emit = (e: unknown): void => follow.sink!(e);

async function mount(lines: RigPayload[], over: Partial<ViewerOptions> = {}): Promise<SessionViewer> {
  viewerRig.chunk = lines;
  const v = new SessionViewer();
  document.body.appendChild(v.element);
  await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true, ...over });
  return v;
}

/** 让消息流看起来「往上翻过」：高 1000、视口 100、停在顶上，再发一次 scroll。 */
function scrollUp(v: SessionViewer): void {
  const el = v.element.querySelector<HTMLElement>(".session-viewer-stream")!;
  Object.defineProperty(el, "scrollHeight", { value: 1000, configurable: true });
  Object.defineProperty(el, "clientHeight", { value: 100, configurable: true });
  el.scrollTop = 0;
  el.dispatchEvent(new Event("scroll"));
}

beforeEach(() => {
  installViewerRig();
  follow.calls = [];
  follow.sink = null;
  follow.stopped = 0;
  follow.order = [];
  vi.mocked(readLines).mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe("查看器跟着长", () => {
  it("先订、再读；没给 follow ⇒ 不订", async () => {
    await mount([userLine(1, "u1", "a")]);
    expect(follow.calls).toEqual([]);
    await mount([userLine(1, "u1", "a")], { follow: { sid: "s1", live: true } });
    expect(follow.calls).toEqual([{ origin: LOCAL_ORIGIN, sid: "s1" }]);
    expect(follow.order, "先订、再读").toEqual(["read", "subscribe", "read"]);
  });

  it("读完流里来的接在后面；留存里与已读重叠的按 seq 去掉；底一行「运行中 · 实时」", async () => {
    const v = await mount([userLine(1, "u1", "a"), assistantLine(2, "a2", "b")], { follow: { sid: "s1", live: true } });
    expect(status(v)).toBe(copyText("sessionViewer.status.live"));
    emit({ t: "lines", lines: [assistantLine(2, "a2", "b"), userLine(3, "u3", "c"), assistantLine(4, "a4", "d")] });
    expect(cards(v)).toEqual(["u1", "a2", "u3", "a4"]);
    expect(v.element.querySelector('[data-role="meta"]')?.textContent).toContain(copyText("sessionViewer.head.count", { n: 4 }));
    expect(pill(v).hidden, "在底部 ⇒ 不出「↓ 新内容」").toBe(true);
  });

  it("读的时候流里先来的行等读完再接（一行不漏、不重）", async () => {
    viewerRig.chunk = [userLine(1, "u1", "a")];
    const v = new SessionViewer();
    document.body.appendChild(v.element);
    const done = v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true, follow: { sid: "s1", live: true } });
    await vi.waitFor(() => expect(follow.sink).not.toBeNull());
    emit({ t: "lines", lines: [userLine(1, "u1", "a"), assistantLine(2, "a2", "b")] });
    expect(cards(v)).toEqual([]);
    await done;
    expect(cards(v)).toEqual(["u1", "a2"]);
  });

  it("往上翻了 ⇒ 不拽人，出「↓ 新内容」；点它 ⇒ 回到底部、收起", async () => {
    const v = await mount([userLine(1, "u1", "a")], { follow: { sid: "s1", live: true } });
    scrollUp(v);
    emit({ t: "lines", lines: [assistantLine(2, "a2", "b")] });
    expect(pill(v).hidden).toBe(false);
    expect(pill(v).textContent).toContain(copyText("sessionViewer.stream.newContent"));
    pill(v).click();
    expect(pill(v).hidden).toBe(true);
  });

  it("结束了 ⇒ 交宿主、底一行回到条数；那台看不见 ⇒ 不再说「实时」；又看得见 ⇒ 按行号补那段", async () => {
    const onLive = vi.fn();
    const v = await mount([userLine(1, "u1", "a")], { follow: { sid: "s1", live: true, onLive } });
    emit({ t: "sight", seen: false });
    expect(status(v)).toBe(copyText("sessionViewer.status.all", { n: 1 }));
    vi.mocked(readLines).mockResolvedValueOnce({ from: 2, next: 3, eof: true, payloads: [assistantLine(2, "a2", "b")] } as never);
    emit({ t: "sight", seen: true });
    await vi.waitFor(() => expect(cards(v)).toEqual(["u1", "a2"]));
    expect(vi.mocked(readLines).mock.calls[0].slice(0, 3)).toEqual([LOCAL_ORIGIN, "/p/s1.jsonl", 2]);
    expect(status(v)).toBe(copyText("sessionViewer.status.live"));
    emit({ t: "live", live: false });
    expect(onLive).toHaveBeenCalledWith(false);
    expect(status(v)).toBe(copyText("sessionViewer.status.all", { n: 2 }));
  });

  it("流里丢了行 ⇒ 从已有的最后一行之后按行号补到末尾", async () => {
    const v = await mount([userLine(1, "u1", "a")], { follow: { sid: "s1", live: true } });
    vi.mocked(readLines)
      .mockResolvedValueOnce({ from: 2, next: 3, eof: false, payloads: [assistantLine(2, "a2", "b")] } as never)
      .mockResolvedValueOnce({ from: 3, next: 4, eof: true, payloads: [userLine(3, "u3", "c")] } as never);
    emit({ t: "gap" });
    await vi.waitFor(() => expect(cards(v)).toEqual(["u1", "a2", "u3"]));
    expect(vi.mocked(readLines).mock.calls.map((c) => c[2])).toEqual([2, 3]);
  });

  it("换会话 / 收起 ⇒ 撤掉那一条订阅；换了之后迟到的行不认", async () => {
    const v = await mount([userLine(1, "u1", "a")], { follow: { sid: "s1", live: true } });
    const old = follow.sink!;
    await v.load({ jsonlPath: "/p/s2.jsonl", displayTitle: "T2", origin: LOCAL_ORIGIN, suppressBranch: true });
    expect(follow.stopped).toBe(1);
    old({ t: "lines", lines: [assistantLine(9, "late", "x")] });
    expect(cards(v)).not.toContain("late");
    v.dispose();
    expect(follow.stopped).toBe(1);
  });

  it("刚进「在看」名单（from）⇒ 已有的最后一行之后按行号补到上流点；期间流里先来的上流点之后的行等补完再接（不丢前面那段）", async () => {
    const v = await mount([userLine(1, "u1", "a")], { follow: { sid: "s1", live: true } });
    let release!: () => void;
    vi.mocked(readLines).mockImplementationOnce(
      () =>
        new Promise((r) => {
          release = () => r({ from: 2, next: 4, eof: false, payloads: [assistantLine(2, "a2", "b"), userLine(3, "u3", "c")] } as never);
        }),
    );
    emit({ t: "from", path: "/p/s1.jsonl", seq: 4 });
    emit({ t: "lines", lines: [assistantLine(4, "a4", "d")] }); // 上流点那一行比补的先到
    expect(cards(v), "补完之前不该先接后面的").toEqual(["u1"]);
    release();
    await vi.waitFor(() => expect(cards(v)).toEqual(["u1", "a2", "u3", "a4"]));
    expect(vi.mocked(readLines).mock.calls.map((c) => [c[2], c[3]])).toEqual([[2, 4]]);
    emit({ t: "from", path: "/p/other.jsonl", seq: 9 }); // 别的那份记录
    emit({ t: "lines", lines: [userLine(5, "u5", "e")] });
    expect(cards(v)).toEqual(["u1", "a2", "u3", "a4", "u5"]);
    expect(vi.mocked(readLines)).toHaveBeenCalledTimes(1);
  });
});

