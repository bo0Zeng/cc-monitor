/**
 * 桌面附件草稿：那一圈是本机后端的事（`backend/control/attach_upload.rs`，判据在 `tests/backend/control/attach_upload_tests.rs`），
 * 这里只看界面交了什么（字节交壳拿路径 ⇒ put 带路径 · ✕ ⇒ drop · ［重试］⇒ retry · 送出 ⇒ take）、照推来的那一排怎么画。
 */
import { describe, expect, it, vi } from "vitest";

import { AttachDraft, type AttachIo } from "../../../src/frontend/ui/attach-draft";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { AttachCardPhase } from "../../../src/frontend/ui/generated/AttachCardPhase";
import type { AttachDraftCard } from "../../../src/frontend/ui/generated/AttachDraftCard";
import type { AttachDraftShown } from "../../../src/frontend/ui/generated/AttachDraftShown";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

function blob(n: number): Blob {
  return new Blob([new Uint8Array(n)]);
}

/** 本机后端那一排（每张卡照给的那一步）。 */
function shown(draft: string, cards: { key: string; name: string; phase: AttachCardPhase; id?: string; done?: number }[], over: Partial<AttachDraftShown> = {}): AttachDraftShown {
  return {
    draft,
    cards: cards.map((c) => ({ key: c.key, name: c.name, phase: c.phase, done: c.done ?? 0, bytes: 5, id: c.id ?? null, line: null, remove: "✕", class: "image", class_text: "图样" })),
    note: null,
    offline: false,
    ready: cards.every((c) => c.phase.kind === "landed"),
    ...over,
  };
}

function rig(over: Partial<AttachIo> = {}) {
  let push: (s: AttachDraftShown | null) => void = () => {};
  const io = {
    stage: vi.fn(async (_b: Blob) => "/home/u/.cc-monitor/staging/ab.part"),
    put: vi.fn(async (draft: string, _sid: string, cards: AttachDraftCard[]) => shown(draft, cards.map((c) => ({ key: c.key, name: c.name, phase: { kind: "queued" } as AttachCardPhase })))),
    drop: vi.fn(async (draft: string, _key: string) => shown(draft, [])),
    retry: vi.fn(async (draft: string) => shown(draft, [])),
    take: vi.fn(async (_draft: string): Promise<string[] | null> => null),
    view: vi.fn(async (draft: string) => shown(draft, [{ key: "1", name: "a.png", phase: { kind: "moving" }, done: 3 }])),
    watch: vi.fn((_draft: string, on: (s: AttachDraftShown | null) => void) => {
      push = on;
      return () => {};
    }),
  };
  Object.assign(io, over);
  const changed = vi.fn();
  const draft = new AttachDraft("s-1", io as AttachIo, changed);
  return { draft, io, changed, push: (s: AttachDraftShown | null) => push(s) };
}

describe("桌面附件草稿", () => {
  it("★ 加一份：字节交壳拿路径 ⇒ put 带那条路径；那一排照回的画", async () => {
    const { draft, io } = rig();
    draft.add([{ name: "a.png", blob: blob(5) }]);
    expect(draft.chips().map((c) => c.at.at)).toEqual(["queued"]);
    await flush();
    expect(io.put).toHaveBeenCalledWith(draft.draft, "s-1", [{ key: "1", name: "a.png", path: "/home/u/.cc-monitor/staging/ab.part", bytes: 5 }]);
    expect(draft.chips()).toHaveLength(1);
    expect(draft.chips()[0].classText).toBe("图样");
    expect(draft.blocker("")).toBe(copyText("attach.blocked.sending"));
  });

  it("★ 推来的体 ⇒ chips：传输中带已送字节 · 落好带编号 · 不收的不给重试 · 断着的画离线", async () => {
    const { draft, push } = rig();
    draft.add([{ name: "a.png", blob: blob(5) }]);
    await flush();
    push(shown(draft.draft, [{ key: "1", name: "a.png", phase: { kind: "moving" }, done: 3 }]));
    expect(draft.chips()[0].at).toEqual({ at: "sending", done: 3 });
    push(shown(draft.draft, [{ key: "1", name: "a.png", phase: { kind: "landed" }, id: "id-1" }]));
    expect(draft.chips()[0].at).toEqual({ at: "done", id: "id-1" });
    expect(draft.blocker("")).toBeNull();
    push(shown(draft.draft, [{ key: "1", name: "a.png", phase: { kind: "refused", said: "那台不收" } }]));
    expect(draft.chips()[0].at).toEqual({ at: "failed", said: "那台不收", retry: false, detail: "" });
    expect(draft.blocker("")).toBe(copyText("attach.blocked.failed"));
    push(shown(draft.draft, [{ key: "1", name: "a.png", phase: { kind: "queued" } }], { offline: true }));
    expect(draft.chips()[0].at.at).toBe("offline");
    expect(draft.blocker("离线了")).toBe("离线了");
  });

  it("推的那一帧没带体 ⇒ 重问 view", async () => {
    const { draft, io, push } = rig();
    draft.add([{ name: "a.png", blob: blob(5) }]);
    await flush();
    push(null);
    await flush();
    expect(io.view).toHaveBeenCalledWith(draft.draft);
    expect(draft.chips()[0].at).toEqual({ at: "sending", done: 3 });
  });

  it("★ 送出拿 take 回的编号；还有没落好的 ⇒ null（不送）", async () => {
    const take = vi.fn(async (_d: string): Promise<string[] | null> => null);
    const { draft, push } = rig({ take });
    draft.add([{ name: "a.png", blob: blob(5) }]);
    await flush();
    expect(await draft.ids()).toBeNull();
    push(shown(draft.draft, [{ key: "1", name: "a.png", phase: { kind: "landed" }, id: "id-1" }]));
    take.mockResolvedValueOnce(["id-1"]);
    expect(await draft.ids()).toEqual(["id-1"]);
    expect(take).toHaveBeenCalledWith(draft.draft);
  });

  it("没有附件 ⇒ 编号是空的，不问 take", async () => {
    const { draft, io } = rig();
    expect(await draft.ids()).toEqual([]);
    expect(io.take).not.toHaveBeenCalled();
  });

  it("✕ ⇒ drop(草稿号, 键)；［重试］⇒ retry(草稿号)", async () => {
    const { draft, io } = rig();
    draft.add([{ name: "a.png", blob: blob(5) }]);
    await flush();
    draft.retry();
    draft.remove("1");
    await flush();
    expect(io.retry).toHaveBeenCalledWith(draft.draft);
    expect(io.drop).toHaveBeenCalledWith(draft.draft, "1");
    expect(draft.chips()).toHaveLength(0);
  });

  it("交壳没成 ⇒ 那一颗失败（那一句）、不交 put", async () => {
    const stage = vi.fn(async (_b: Blob): Promise<string> => {
      throw new Error("写不进暂存区");
    });
    const { draft, io } = rig({ stage });
    draft.add([{ name: "a.png", blob: blob(5) }]);
    await flush();
    expect(io.put).not.toHaveBeenCalled();
    expect(draft.chips()[0].at.at).toBe("failed");
    expect(draft.blocker("")).toBe(copyText("attach.blocked.failed"));
  });

  it("目录不交壳（路径空着），由那台判不收", async () => {
    const { draft, io } = rig();
    draft.add([{ name: "dir", blob: blob(0), dir: true }]);
    await flush();
    expect(io.stage).not.toHaveBeenCalled();
    expect(io.put).toHaveBeenCalledWith(draft.draft, "s-1", [{ key: "1", name: "dir", path: "", bytes: 0, dir: true }]);
  });
});
