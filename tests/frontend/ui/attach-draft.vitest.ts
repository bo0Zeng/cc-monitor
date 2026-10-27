/**
 * 桌面附件草稿：加进来先问那台收不收 ⇒ 收的逐块进暂存区、提交 ⇒ 送出那一下要的编号与挡着送出的那一句。问那台的几口换成假的。
 */
import { describe, expect, it, vi } from "vitest";

import { AttachDraft, CHUNK, type AttachIo } from "../../../src/frontend/ui/attach-draft";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { AttachItem } from "../../../src/frontend/ui/generated/AttachItem";
import type { AttachVerdict } from "../../../src/frontend/ui/generated/AttachVerdict";
import type { SessionAttachmentCommitReq } from "../../../src/frontend/ui/generated/SessionAttachmentCommitReq";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

/** 一份 `n` 字节的假文件（只用到 size 与切片）。 */
function blob(n: number): Blob {
  return new Blob([new Uint8Array(n)]);
}

const OFFLINE = new Error("offline");

function rig(over: Partial<AttachIo> = {}) {
  let keys = 0;
  const io = {
    check: vi.fn(async (_sid: string, items: AttachItem[]): Promise<AttachVerdict[]> => items.map((it) => ({ ok: !it.dir, class: it.dir ? "folder" : it.name.endsWith(".png") ? "image" : "code", ...(it.dir ? { why: "is_dir" as const, said: "不收目录" } : {}) }))),
    stage: vi.fn(async (_key: string, _seq: number, _bytes: Uint8Array) => {}),
    commit: vi.fn(async (req: SessionAttachmentCommitReq) => ({ id: `1027-1200-${req.key.slice(0, 4)}`, name: req.name, path: `/w/.ccm-attach/x/${req.name}`, rel: `.ccm-attach/x/${req.name}`, bytes: req.bytes, sha256: req.expect.sha256, class: "code" as const })),
    drop: vi.fn(async (_sid: string, _id: string) => {}),
    digest: vi.fn(async (_b: Blob) => "ab".repeat(32)),
    read: vi.fn(async (_b: Blob, start: number, end: number) => new Uint8Array(end - start)),
    newKey: vi.fn(() => `${String(++keys).padStart(4, "0")}${"0".repeat(28)}`),
    offline: vi.fn((e: unknown) => e === OFFLINE),
    ...over,
  };
  const changed = vi.fn();
  const draft = new AttachDraft("s-1", io as AttachIo, changed);
  return { draft, io, changed };
}

describe("桌面附件草稿", () => {
  it("★ 收的逐块进暂存区再提交（块数 · 字节 · 摘要照交），完了才给编号；送出不再挡", async () => {
    const { draft, io } = rig();
    draft.add([{ name: "login.css", blob: blob(CHUNK * 2 + 5) }]);
    expect(draft.blocker()).toBe(copyText("attach.blocked.sending", { done: 0, n: 1 }));
    await flush();
    expect(io.check).toHaveBeenCalledWith("s-1", [{ name: "login.css", bytes: CHUNK * 2 + 5 }]);
    expect(io.stage.mock.calls.map((c) => [c[1], c[2].length])).toEqual([
      [0, CHUNK],
      [1, CHUNK],
      [2, 5],
    ]);
    expect(io.commit).toHaveBeenCalledWith({ sid: "s-1", key: io.stage.mock.calls[0][0], name: "login.css", bytes: CHUNK * 2 + 5, chunks: 3, expect: { sha256: "ab".repeat(32) } });
    expect(draft.ids()).toEqual(["1027-1200-0001"]);
    expect(draft.chips()[0].class).toBe("code");
    expect(draft.blocker()).toBeNull();
  });

  it("那台不收的那份当场落失败（那台写好的一句，不给重试），不传；挡着送出", async () => {
    const { draft, io } = rig();
    draft.add([
      { name: "assets", blob: blob(0), dir: true },
      { name: "a.png", blob: blob(3) },
    ]);
    await flush();
    const [dir, png] = draft.chips();
    expect(dir.at).toEqual({ at: "failed", said: "不收目录", retry: false, detail: "" });
    expect(dir.class).toBe("folder");
    expect(png.at.at).toBe("done");
    expect(io.stage).toHaveBeenCalledTimes(1);
    expect(draft.blocker()).toBe(copyText("attach.blocked.failed", { n: 1 }));
    draft.remove(dir.n);
    expect(draft.blocker()).toBeNull();
  });

  it("后加的那一批也把已在草稿里的列进那一问（按草稿里的先后）", async () => {
    const { draft, io } = rig();
    draft.add([{ name: "a.png", blob: blob(1) }]);
    await flush();
    draft.add([{ name: "b.css", blob: blob(2) }]);
    await flush();
    expect(io.check.mock.calls[1][1]).toEqual([
      { name: "a.png", bytes: 1 },
      { name: "b.css", bytes: 2 },
    ]);
  });

  it("传到一半断了 ⇒ 排队 · 离线（传了多少留着）；连上 resume 从断的那一块接着传", async () => {
    let failOnce = true;
    const { draft, io } = rig({
      stage: vi.fn(async (_k: string, seq: number) => {
        if (seq === 1 && failOnce) {
          failOnce = false;
          throw OFFLINE;
        }
      }),
    });
    draft.add([{ name: "big.log", blob: blob(CHUNK * 2) }]);
    await flush();
    expect(draft.chips()[0].at).toEqual({ at: "offline", done: CHUNK });
    expect(draft.blocker()).toBe(copyText("attach.blocked.sending", { done: 0, n: 1 }));
    draft.resume();
    await flush();
    expect(io.stage.mock.calls.map((c) => c[1])).toEqual([0, 1, 1]);
    expect(new Set(io.stage.mock.calls.map((c) => c[0])).size).toBe(1);
    expect(draft.chips()[0].at.at).toBe("done");
  });

  it("别的失败 ⇒ 失败 · 可重试；重试换一个新键从头传", async () => {
    let fail = true;
    const { draft, io } = rig({
      commit: vi.fn(async (req: SessionAttachmentCommitReq) => {
        if (fail) {
          fail = false;
          throw new Error("磁盘满");
        }
        return { id: "1027-1200-beef", name: req.name, path: "/p", rel: "r", bytes: req.bytes, sha256: req.expect.sha256, class: "text" as const };
      }),
    });
    draft.add([{ name: "x.txt", blob: blob(4) }]);
    await flush();
    const c = draft.chips()[0];
    expect(c.at).toMatchObject({ at: "failed", said: "磁盘满", retry: true });
    draft.retry(c.n);
    await flush();
    expect(draft.ids()).toEqual(["1027-1200-beef"]);
    expect(io.stage.mock.calls[0][0]).not.toBe(io.stage.mock.calls[1][0]);
  });

  it("✕ 传完的那份 ⇒ 那台删掉（drop）；✕ 传着的那份 ⇒ 下一块不再送、不提交", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    const { draft, io } = rig({
      stage: vi.fn(async (_k: string, seq: number) => {
        if (seq === 0) await gate;
      }),
    });
    draft.add([{ name: "a.png", blob: blob(CHUNK + 1) }]);
    await flush();
    draft.remove(draft.chips()[0].n);
    release();
    await flush();
    expect(io.stage).toHaveBeenCalledTimes(1);
    expect(io.commit).not.toHaveBeenCalled();

    const done = rig();
    done.draft.add([{ name: "b.css", blob: blob(1) }]);
    await flush();
    done.draft.remove(done.draft.chips()[0].n);
    expect(done.io.drop).toHaveBeenCalledWith("s-1", "1027-1200-0001");
    expect(done.draft.chips()).toEqual([]);
  });

  it("送字时那台说那几份已不在 ⇒ 那几颗变失败 · 可重传", async () => {
    const { draft } = rig();
    draft.add([{ name: "a.css", blob: blob(1) }, { name: "b.css", blob: blob(1) }]);
    await flush();
    draft.markGone(["1027-1200-0002"]);
    const [a, b] = draft.chips();
    expect(a.at.at).toBe("done");
    expect(b.at).toMatchObject({ at: "failed", said: copyText("attach.chip.gone"), retry: true });
    expect(draft.ids()).toEqual(["1027-1200-0001"]);
  });
});
