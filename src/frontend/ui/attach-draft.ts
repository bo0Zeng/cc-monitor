/**
 * **桌面附件草稿**：一条会话那一条没送的话里带的附件（按会话各一份，宿主记着）。
 *
 * - 加进来先问那台收不收（`session-attachment-check`，草稿里已有的也列进去、按先后）⇒ 不收的那份当场落失败（那台写好的一句，不给重试）。
 * - 收的逐份传：那一份交给壳（`attach_stage`）逐块送进会话那台的暂存区 —— 块多大 · 整份摘要 · 暂存键都是壳里的 `chunk_core`
 *   （文件窗口与手机同一份），这里只交 Blob、收进度 ⇒ `session-attachment-commit {chunks, bytes, expect}` 落进会话工作目录、回编号。
 *   字节本机远端同一条路（webview 只给 File 对象，没有本机路径），与手机同一形。
 * - 那台断着 ⇒ 排队 · 离线（传了多少照着画）；连上宿主调 [`AttachDraft.resume`] 从头再传（同手机：断了回排队、重走一遍）。
 *   别的失败 ⇒ 失败 · 可重试（从头传）。
 * - ✕：传着的那份撤单（下一块不再送，已送的块交那台的孤儿扫）；传完的那份 `session-attachment-drop`。
 * - 送出那一下要的：编号（[`AttachDraft.ids`]，按草稿先后）· 挡着送出的那一句（[`AttachDraft.blocker`]）。
 *
 * 判收不收 · 上限 · 落点 · 类别都是那台的事（`class` · `class_text` 从那一问的答来）；这里只排先后、记进度。问那台的几口与交壳那一口都经 [`AttachIo`]，测试换假的。
 */
import { copyText } from "./copy-table";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { commands } from "./ipc/commands";
import { settleTyped } from "./control-said";
import { ChanError, decodeFail, detailOfFail, saidOfFail } from "../../comms/inward/chan";
import { detailOf, saidOf } from "./kit/detail";
import type { Machine } from "./ipc/origin";
import type { AttachItem } from "./generated/AttachItem";
import type { AttachVerdict } from "./generated/AttachVerdict";
import type { FileClass } from "./generated/FileClass";
import type { SessionAttachmentCommitReply } from "./generated/SessionAttachmentCommitReply";
import type { SessionAttachmentCommitReq } from "./generated/SessionAttachmentCommitReq";
import type { Staged } from "./generated/Staged";

/** 交给壳传的那一趟：传完回暂存区里那一份（键 · 块数 · 字节 · 摘要）；`stop()` ⇒ 下一块不再送。 */
export interface Staging {
  readonly done: Promise<Staged>;
  stop(): void;
}

/** 问那台的几口 ＋ 交壳传那一口（缺省走通道与壳；测试换假的）。 */
export interface AttachIo {
  check(sid: string, items: AttachItem[]): Promise<AttachVerdict[]>;
  /** 那一份交给壳逐块送进那台暂存区；`moved` 逐块报已送字节。 */
  stage(blob: Blob, moved: (done: number) => void): Staging;
  commit(req: SessionAttachmentCommitReq): Promise<SessionAttachmentCommitReply>;
  drop(sid: string, id: string): Promise<void>;
  /** 没成的原因是会话所在机器未连接（⇒ 排队 · 离线，不算失败）。 */
  offline(e: unknown): boolean;
}

/** 一颗 chip 此刻在哪：排队 · 传输中（已传字节）· 那台断着（已传字节）· 已完成（编号）· 失败（那一句 · 能不能重试 · 复制详情）。 */
export type ChipAt =
  | { at: "queued" }
  | { at: "sending"; done: number }
  | { at: "offline"; done: number }
  | { at: "done"; id: string }
  | { at: "failed"; said: string; retry: boolean; detail: string };

/** 草稿里的一份。 */
export interface Chip {
  /** 这一份在草稿里的号（✕ · 重试按它找）。 */
  readonly n: number;
  readonly name: string;
  readonly bytes: number;
  readonly blob: Blob;
  readonly dir: boolean;
  /** 那台判的那一种（问过之前 ⇒ `null`）。 */
  class: FileClass | null;
  /** 那一种的词（那台写好；问过之前 ⇒ 空）。 */
  classText: string;
  at: ChipAt;
}

/** 加进来的一份：名字 · 字节 · 是不是目录（拖进来的目录）。 */
export interface AttachInput {
  name: string;
  blob: Blob;
  dir?: boolean;
}

/** 草稿里一份的内账：已送多少 · 在传那一趟的撤单 · 问过没有 · 撤了没有。 */
interface Slot extends Chip {
  sent: number;
  stop: (() => void) | null;
  checked: boolean;
  dropped: boolean;
}

export class AttachDraft {
  private readonly slots: Slot[] = [];
  private seq = 0;
  /** 正在传的那一份（同一时刻只传一份）。 */
  private busy = false;
  private checking = false;

  constructor(
    readonly sid: string,
    private readonly io: AttachIo,
    private readonly changed: () => void,
  ) {}

  /** 此刻的几颗（按草稿先后）。 */
  chips(): readonly Chip[] {
    return this.slots;
  }

  /** 加一批：先问那台收不收，收的排进去传。 */
  add(files: readonly AttachInput[]): void {
    for (const f of files) {
      this.slots.push({ n: ++this.seq, name: f.name, bytes: f.blob.size, blob: f.blob, dir: f.dir === true, class: null, classText: "", at: { at: "queued" }, sent: 0, stop: null, checked: false, dropped: false });
    }
    this.changed();
    void this.ask();
  }

  /** ✕ 一份：传着的撤单、传完的让那台删掉。 */
  remove(n: number): void {
    const i = this.slots.findIndex((s) => s.n === n);
    if (i < 0) return;
    const [s] = this.slots.splice(i, 1);
    s.dropped = true;
    s.stop?.();
    if (s.at.at === "done") void this.io.drop(this.sid, s.at.id).catch(() => {});
    this.changed();
  }

  /** 失败那颗上的［重试］：从头传。 */
  retry(n: number): void {
    const s = this.slots.find((x) => x.n === n);
    if (!s || s.at.at !== "failed" || !s.at.retry) return;
    s.at = { at: "queued" };
    this.changed();
    void this.pump();
  }

  /** 那台连上了：断着的从头再传、没问成的再问。 */
  resume(): void {
    for (const s of this.slots) if (s.at.at === "offline") s.at = { at: "queued" };
    this.changed();
    void this.ask();
  }

  /** 送字时那台说这几份已不在 ⇒ 变失败 · 可重传。 */
  markGone(ids: readonly string[]): void {
    const said = copyText("attach.chip.gone");
    for (const s of this.slots) {
      if (s.at.at === "done" && ids.includes(s.at.id)) s.at = { at: "failed", said, retry: true, detail: "" };
    }
    this.changed();
  }

  /** 送到了：chip 清空（那台的文件留着，消息里引着它）。 */
  clear(): void {
    for (const s of this.slots) s.dropped = true;
    this.slots.length = 0;
    this.changed();
  }

  /** 传完的那几份的编号（按草稿先后）。 */
  ids(): string[] {
    return this.slots.flatMap((s) => (s.at.at === "done" ? [s.at.id] : []));
  }

  /** 挡着送出的那一句（一句，不数个数）：有失败 ⇒「附件失败」；有断着的 ⇒ `offline`（连接层写好的离线那一句）；有没传完的 ⇒「附件传输中」；都传完了 ⇒ `null`。 */
  blocker(offline: string): string | null {
    const { failed, cut, pending } = this.tally();
    if (failed) return copyText("attach.blocked.failed");
    if (cut) return offline;
    return pending ? copyText("attach.blocked.sending") : null;
  }

  /** 有没有失败的 · 有没有断着的 · 有没有还没传完的。 */
  private tally(): { failed: boolean; cut: boolean; pending: boolean } {
    return {
      failed: this.slots.some((s) => s.at.at === "failed"),
      cut: this.slots.some((s) => s.at.at === "offline"),
      pending: this.slots.some((s) => s.at.at !== "done"),
    };
  }

  /** 问那台收不收：没问过的那几份连同草稿里已收的（按先后）一起问，答落到没问过的那几份上。 */
  private async ask(): Promise<void> {
    if (this.checking) return;
    const fresh = this.slots.filter((s) => !s.checked && s.at.at === "queued");
    if (fresh.length === 0) return void this.pump();
    this.checking = true;
    const listed = this.slots.filter((s) => s.at.at !== "failed" && s.at.at !== "offline");
    const items: AttachItem[] = listed.map((s) => ({ name: s.name, bytes: s.bytes, ...(s.dir ? { dir: true } : {}) }));
    try {
      const verdicts = await this.io.check(this.sid, items);
      listed.forEach((s, i) => {
        const v = verdicts[i];
        if (s.checked || !v) return;
        s.checked = true;
        s.class = v.class;
        s.classText = v.class_text;
        if (!v.ok) s.at = { at: "failed", said: v.said ?? "", retry: false, detail: "" };
      });
    } catch (e) {
      for (const s of fresh) s.at = this.io.offline(e) ? { at: "offline", done: 0 } : { at: "failed", said: saidOf(e), retry: true, detail: detailOf(e) };
      for (const s of fresh) if (s.at.at === "failed") s.checked = false;
    } finally {
      this.checking = false;
    }
    this.changed();
    if (this.slots.some((s) => !s.checked && s.at.at === "queued")) return void this.ask();
    void this.pump();
  }

  /** 排在最前、问过了的那一份开传；一份传完接下一份。 */
  private async pump(): Promise<void> {
    if (this.busy) return;
    const s = this.slots.find((x) => x.checked && x.at.at === "queued");
    if (!s) return;
    this.busy = true;
    try {
      await this.upload(s);
    } finally {
      this.busy = false;
    }
    void this.pump();
  }

  private async upload(s: Slot): Promise<void> {
    s.sent = 0;
    s.at = { at: "sending", done: 0 };
    this.changed();
    const run = this.io.stage(s.blob, (done) => {
      if (s.dropped) return;
      s.sent = done;
      s.at = { at: "sending", done };
      this.changed();
    });
    s.stop = run.stop;
    try {
      const staged = await run.done;
      s.stop = null;
      if (s.dropped) return;
      const got = await this.io.commit({ sid: this.sid, key: staged.key, name: s.name, bytes: staged.bytes, chunks: staged.chunks, expect: { sha256: staged.sha256 } });
      if (s.dropped) {
        void this.io.drop(this.sid, got.id).catch(() => {});
        return;
      }
      s.class = got.class;
      s.at = { at: "done", id: got.id };
    } catch (e) {
      s.stop = null;
      if (s.dropped) return;
      s.at = this.io.offline(e) ? { at: "offline", done: s.sent } : { at: "failed", said: saidOf(e), retry: true, detail: detailOf(e) };
    }
    this.changed();
  }
}

/** 一问的期限（问收不收 · 提交 · 删）。 */
const STEP_BUDGET_MS = 30_000;

/** 生产那一份：问那台经通道、传字节交壳（本机远端同一条路）。 */
export function channelIo(origin: Machine): AttachIo {
  return {
    check: async (sid, items) =>
      (await settleTyped(origin, "session-attachment-check", callTyped(origin, "session-attachment-check", { sid, items }, budgetWithin(STEP_BUDGET_MS)))).items,
    stage: (blob, moved) => {
      const ticket = nextTicket();
      const done = blob.arrayBuffer().then((buf) =>
        commands.attach_stage(origin, ticket, new Uint8Array(buf), moved).catch((raw: unknown) => {
          // 失败是通道那一形（`chan/webview.rs::Fail`）：照 `chan.call` 解回三层，离线照 `hop` 层认。
          throw new ChanError(decodeFail(raw), saidOfFail(raw), detailOfFail(raw));
        }),
      );
      return { done, stop: () => void commands.attach_stage_stop({ ticket }).catch(() => {}) };
    },
    commit: (req) => settleTyped(origin, "session-attachment-commit", callTyped(origin, "session-attachment-commit", req, budgetWithin(STEP_BUDGET_MS))),
    drop: async (sid, id) => {
      await settleTyped(origin, "session-attachment-drop", callTyped(origin, "session-attachment-drop", { sid, id }, budgetWithin(STEP_BUDGET_MS)));
    },
    offline: (e) => e instanceof ChanError && e.error.layer === "hop",
  };
}

/** 这一窗里交壳那几趟的撤单号（壳按窗口 · 单号记）。 */
let tickets = 0;
const nextTicket = (): number => ++tickets;
