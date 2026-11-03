/**
 * 桌面附件草稿：加进来先问那台收不收 ⇒ 收的交壳送进暂存区、提交 ⇒ 送出那一下要的编号与挡着送出的那一句。问那台的几口与交壳那一口换成假的。
 * 逐块 · 摘要 · 暂存键在壳里的 `chunk_core`（判据 `tests/frontend/shell/attach_stage_tests.rs`），这里只看草稿怎么排、怎么记。
 */
import { describe, expect, it, vi } from "vitest";

import { AttachDraft, type AttachIo, type Staging } from "../../../src/frontend/ui/attach-draft";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { AttachItem } from "../../../src/frontend/ui/generated/AttachItem";
import type { AttachVerdict } from "../../../src/frontend/ui/generated/AttachVerdict";
import type { SessionAttachmentCommitReq } from "../../../src/frontend/ui/generated/SessionAttachmentCommitReq";
import type { Staged } from "../../../src/frontend/ui/generated/Staged";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

/** 一份 `n` 字节的假文件（只用到 size）。 */
function blob(n: number): Blob {
  return new Blob([new Uint8Array(n)]);
}

const OFFLINE = new Error("offline");
const SHA = "ab".repeat(32);

/** 交壳那一趟的替身：报一次进度（整份）、回暂存区那一份；`run` 换掉它的过程。 */
function staging(run?: (blob: Blob, moved: (done: number) => void, n: number) => Promise<Staged>) {
  let n = 0;
  const stops: ReturnType<typeof vi.fn>[] = [];
  const stage = vi.fn((b: Blob, moved: (done: number) => void): Staging => {
    const k = ++n;
    const stop = vi.fn();
    stops.push(stop);
    const done = run
      ? run(b, moved, k)
      : (async () => {
          moved(b.size);
          return { key: `${String(k).padStart(4, "0")}${"0".repeat(28)}`, chunks: 3, bytes: b.size, sha256: SHA };
        })();
    return { done, stop };
  });
  return { stage, stops };
}

function rig(over: Partial<AttachIo> = {}) {
  const io = {
    check: vi.fn(async (_sid: string, items: AttachItem[]): Promise<AttachVerdict[]> => items.map((it) => ({ ok: !it.dir, class: it.dir ? "folder" : it.name.endsWith(".png") ? "image" : "code", class_text: `class-of-${it.name}`, ...(it.dir ? { why: "is_dir" as const, said: "refused-dir" } : {}) }))),
    stage: staging().stage,
    commit: vi.fn(async (req: SessionAttachmentCommitReq) => ({ id: `1027-1200-${req.key.slice(0, 4)}`, name: req.name, path: `/w/.ccm-attach/x/${req.name}`, rel: `.ccm-attach/x/${req.name}`, bytes: req.bytes, sha256: req.expect.sha256, class: "code" as const })),
    drop: vi.fn(async (_sid: string, _id: string) => {}),
    offline: vi.fn((e: unknown) => e === OFFLINE),
  };
  Object.assign(io, over);
  const changed = vi.fn();
  const draft = new AttachDraft("s-1", io as AttachIo, changed);
  return { draft, io, changed };
}

describe("桌面附件草稿", () => {
  it("★ 收的交壳送进暂存区再提交（键 · 块数 · 字节 · 摘要照壳回的交），完了才给编号；送出不再挡", async () => {
    const { draft, io } = rig();
    draft.add([{ name: "login.css", blob: blob(70) }]);
    expect(draft.blocker("")).toBe(copyText("attach.blocked.sending"));
    await flush();
    expect(io.check).toHaveBeenCalledWith("s-1", [{ name: "login.css", bytes: 70 }]);
    expect(io.stage).toHaveBeenCalledTimes(1);
    expect(io.commit).toHaveBeenCalledWith({ sid: "s-1", key: `0001${"0".repeat(28)}`, name: "login.css", bytes: 70, chunks: 3, expect: { sha256: SHA } });
    expect(draft.ids()).toEqual(["1027-1200-0001"]);
    expect(draft.chips()[0].class).toBe("code");
    expect(draft.chips()[0].classText).toBe("class-of-login.css");
    expect(draft.blocker("")).toBeNull();
  });

  it("壳逐块报的进度照着画（传输中 · 已传字节）", async () => {
    let finish: (s: Staged) => void = () => {};
    const { stage } = staging(
      (_b, moved) =>
        new Promise<Staged>((r) => {
          moved(40);
          finish = r;
        }),
    );
    const { draft } = rig({ stage });
    draft.add([{ name: "a.log", blob: blob(100) }]);
    await flush();
    expect(draft.chips()[0].at).toEqual({ at: "sending", done: 40 });
    finish({ key: "k".repeat(32), chunks: 1, bytes: 100, sha256: SHA });
    await flush();
    expect(draft.chips()[0].at.at).toBe("done");
  });

  it("那台不收的那份当场落失败（那台写好的一句，不给重试），不传；挡着送出", async () => {
    const { draft, io } = rig();
    draft.add([
      { name: "assets", blob: blob(0), dir: true },
      { name: "a.png", blob: blob(3) },
    ]);
    await flush();
    const [dir, png] = draft.chips();
    expect(dir.at).toEqual({ at: "failed", said: "refused-dir", retry: false, detail: "" });
    expect(dir.class).toBe("folder");
    expect(png.at.at).toBe("done");
    expect(io.stage).toHaveBeenCalledTimes(1);
    expect(draft.blocker("")).toBe(copyText("attach.blocked.failed"));
    draft.remove(dir.n);
    expect(draft.blocker("")).toBeNull();
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

  it("传到一半断了 ⇒ 排队 · 离线（传了多少照着画）；连上 resume 从头再交壳传一趟", async () => {
    const { stage } = staging(async (b, moved, n) => {
      moved(30);
      if (n === 1) throw OFFLINE;
      moved(b.size);
      return { key: "k".repeat(32), chunks: 2, bytes: b.size, sha256: SHA };
    });
    const { draft, io } = rig({ stage });
    draft.add([{ name: "big.log", blob: blob(60) }]);
    await flush();
    expect(draft.chips()[0].at).toEqual({ at: "offline", done: 30 });
    expect(draft.blocker("那台离线那一句（夹具）"), "有断着的 ⇒ 悬停是连接层那一句").toBe("那台离线那一句（夹具）");
    draft.resume();
    await flush();
    expect(io.stage).toHaveBeenCalledTimes(2);
    expect(draft.chips()[0].at.at).toBe("done");
  });

  it("别的失败 ⇒ 失败 · 可重试；重试再交壳传一趟", async () => {
    let fail = true;
    const { draft, io } = rig({
      commit: vi.fn(async (req: SessionAttachmentCommitReq) => {
        if (fail) {
          fail = false;
          throw new Error("disk-full");
        }
        return { id: "1027-1200-beef", name: req.name, path: "/p", rel: "r", bytes: req.bytes, sha256: req.expect.sha256, class: "text" as const };
      }),
    });
    draft.add([{ name: "x.txt", blob: blob(4) }]);
    await flush();
    const c = draft.chips()[0];
    expect(c.at).toMatchObject({ at: "failed", said: "disk-full", retry: true });
    draft.retry(c.n);
    await flush();
    expect(draft.ids()).toEqual(["1027-1200-beef"]);
    expect(io.stage).toHaveBeenCalledTimes(2);
    expect(io.commit.mock.calls[0][0].key).not.toBe(io.commit.mock.calls[1][0].key);
  });

  it("✕ 传完的那份 ⇒ 那台删掉（drop）；✕ 传着的那份 ⇒ 撤单（下一块不再送）、不提交", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    const { stage, stops } = staging(async (b) => {
      await gate;
      return { key: "k".repeat(32), chunks: 1, bytes: b.size, sha256: SHA };
    });
    const { draft, io } = rig({ stage });
    draft.add([{ name: "a.png", blob: blob(9) }]);
    await flush();
    draft.remove(draft.chips()[0].n);
    expect(stops[0]).toHaveBeenCalledTimes(1);
    release();
    await flush();
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
