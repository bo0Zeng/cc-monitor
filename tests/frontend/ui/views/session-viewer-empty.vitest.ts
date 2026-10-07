/**
 * 查看器 0 条消息：读到了、确实没有可显示的消息 ⇒ 消息流位置画空态（规范 I5「空」· C16），不留一片空白；
 * 在跑的会话后来长出消息 ⇒ 空态收起。读失败走错误条，不出空态（出错 ≠ 空）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));
const follow = vi.hoisted(() => ({ sink: null as ((e: unknown) => void) | null }));
vi.mock("../../../../src/frontend/ui/events", () => ({
  followSession: vi.fn(async (_o: string, _sid: string, sink: (e: unknown) => void) => {
    follow.sink = sink;
    return { stop: () => {} };
  }),
}));

import { installViewerRig, line, userLine, viewerRig, type RigPayload } from "../../../test-support/session-viewer-rig";
import { SessionViewer, type ViewerOptions } from "../../../../src/frontend/ui/views/session-viewer";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const empty = (v: SessionViewer): HTMLElement | null => v.element.querySelector<HTMLElement>('[data-role="empty"]');
const shown = (v: SessionViewer): boolean => {
  const e = empty(v);
  return !!e && !e.hidden && (e.textContent ?? "").includes(copyText("sessionViewer.empty.none"));
};

async function mount(lines: RigPayload[], over: Partial<ViewerOptions> = {}): Promise<SessionViewer> {
  viewerRig.chunk = lines;
  const v = new SessionViewer();
  document.body.appendChild(v.element);
  await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true, ...over });
  return v;
}

beforeEach(() => {
  installViewerRig();
  follow.sink = null;
});
afterEach(() => vi.unstubAllGlobals());

describe("查看器 0 条消息", () => {
  it("一行都没有 ⇒ 空态「无消息」", async () => {
    const v = await mount([]);
    expect(shown(v)).toBe(true);
  });

  it("只有不显示的行（标题行）⇒ 也是空态", async () => {
    const v = await mount([line(0, { type: "ai-title", aiTitle: "标题", sessionId: "s1" })]);
    expect(v.element.querySelector("[data-uuid]")).toBeNull();
    expect(shown(v)).toBe(true);
  });

  it("有一条消息 ⇒ 不出空态", async () => {
    const v = await mount([userLine(0, "u0", "你好")]);
    expect(v.element.querySelector('[data-uuid="u0"]')).not.toBeNull();
    expect(shown(v)).toBe(false);
  });

  it("读失败 ⇒ 错误条，不出空态", async () => {
    viewerRig.failPage = true;
    const v = await mount([]);
    expect(shown(v)).toBe(false);
  });

  it("在跑的会话先空、后来长出一条 ⇒ 空态收起", async () => {
    const v = await mount([], { follow: { sid: "s1", live: true } });
    expect(shown(v)).toBe(true);
    follow.sink!({ t: "lines", lines: [userLine(1, "u1", "来了")] });
    expect(v.element.querySelector('[data-uuid="u1"]')).not.toBeNull();
    expect(shown(v)).toBe(false);
  });
});
