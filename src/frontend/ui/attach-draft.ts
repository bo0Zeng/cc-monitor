/**
 * **桌面附件草稿**：一条会话那一条没送的话里带的附件（按会话各一份，宿主记着）。
 *
 * - 加进来先问那台收不收（`session-attachment-check`，草稿里已有的也列进去、按先后）⇒ 不收的那份当场落失败（那台写好的一句，不给重试）。
 * - 收的逐份传：逐块 `files-stage-chunk`（[`CHUNK`] 一块，同 `chunk_core::CHUNK`）进会话那台的暂存区 ⇒ `session-attachment-commit {chunks, bytes, expect}`
 *   落进会话工作目录、回编号。字节只经通道走，本机远端同一条路（webview 只给 File 对象，没有本机路径），与手机同一形。
 * - 那台断着 ⇒ 排队 · 离线（传到哪一块留着，同一个暂存键）；连上宿主调 [`AttachDraft.resume`] 接着传。别的失败 ⇒ 失败 · 可重试（换新键从头传）。
 * - ✕：传着的那份下一块不再送（已送的块交那台的孤儿扫）；传完的那份 `session-attachment-drop`。
 * - 送出那一下要的：编号（[`AttachDraft.ids`]，按草稿先后）· 挡着送出的那一句（[`AttachDraft.blocker`]）。
 *
 * 判收不收 · 上限 · 落点 · 类别都是那台的事（`class` · `class_text` 从那一问的答来）；这里只排先后、记进度。问那台的几口 · 读文件 · 摘要都经 [`AttachIo`]，测试换假的。
 */
import { copyText } from "./copy-table";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { settleTyped } from "./control-said";
import { ChanError } from "../../comms/inward/chan";
import { detailOf, saidOf } from "./kit/detail";
import type { Origin } from "./ipc/origin";
import type { AttachItem } from "./generated/AttachItem";
import type { AttachVerdict } from "./generated/AttachVerdict";
import type { FileClass } from "./generated/FileClass";
import type { SessionAttachmentCommitReply } from "./generated/SessionAttachmentCommitReply";
import type { SessionAttachmentCommitReq } from "./generated/SessionAttachmentCommitReq";

/** 一块多少原始字节（== `chunk_core::CHUNK` == 后端读写一块的上限）。 */
export const CHUNK = 256 * 1024;

/** 问那台的几口 ＋ 读本机那一份（缺省走通道；测试换假的）。 */
export interface AttachIo {
  check(sid: string, items: AttachItem[]): Promise<AttachVerdict[]>;
  stage(key: string, seq: number, bytes: Uint8Array): Promise<void>;
  commit(req: SessionAttachmentCommitReq): Promise<SessionAttachmentCommitReply>;
  drop(sid: string, id: string): Promise<void>;
  /** 整份 SHA-256（小写十六进制）。 */
  digest(blob: Blob): Promise<string>;
  read(blob: Blob, start: number, end: number): Promise<Uint8Array>;
  /** 现造一个暂存键（32 位小写十六进制）。 */
  newKey(): string;
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

/** 草稿里一份的内账：暂存键 · 送到第几块 · 问过没有 · 撤了没有。 */
interface Slot extends Chip {
  stash: string;
  seq: number;
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
      this.slots.push({ n: ++this.seq, name: f.name, bytes: f.blob.size, blob: f.blob, dir: f.dir === true, class: null, classText: "", at: { at: "queued" }, stash: "", seq: 0, checked: false, dropped: false });
    }
    this.changed();
    void this.ask();
  }

  /** ✕ 一份：传着的停、传完的让那台删掉。 */
  remove(n: number): void {
    const i = this.slots.findIndex((s) => s.n === n);
    if (i < 0) return;
    const [s] = this.slots.splice(i, 1);
    s.dropped = true;
    if (s.at.at === "done") void this.io.drop(this.sid, s.at.id).catch(() => {});
    this.changed();
  }

  /** 失败那颗上的［重试］：换新键从头传。 */
  retry(n: number): void {
    const s = this.slots.find((x) => x.n === n);
    if (!s || s.at.at !== "failed" || !s.at.retry) return;
    this.restart(s);
    this.changed();
    void this.pump();
  }

  /** 那台连上了：断着的接着传、没问成的再问。 */
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
    const has = (at: ChipAt["at"]): boolean => this.slots.some((s) => s.at.at === at);
    if (has("failed")) return copyText("attach.blocked.failed");
    if (has("offline")) return offline;
    return this.slots.some((s) => s.at.at !== "done") ? copyText("attach.blocked.sending") : null;
  }

  private restart(s: Slot): void {
    s.at = { at: "queued" };
    s.stash = "";
    s.seq = 0;
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
        if (!v.ok) s.at = { at: "failed", said: v.said || copyText("attach.chip.unknown"), retry: false, detail: "" };
      });
    } catch (e) {
      for (const s of fresh) s.at = this.io.offline(e) ? { at: "offline", done: 0 } : { at: "failed", said: saidOf(e) || copyText("attach.chip.unknown"), retry: true, detail: detailOf(e) };
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
    if (s.stash === "") {
      s.stash = this.io.newKey();
      s.seq = 0;
    }
    const chunks = Math.ceil(s.bytes / CHUNK);
    s.at = { at: "sending", done: Math.min(s.bytes, s.seq * CHUNK) };
    this.changed();
    try {
      while (s.seq < chunks) {
        const start = s.seq * CHUNK;
        const end = Math.min(s.bytes, start + CHUNK);
        const bytes = await this.io.read(s.blob, start, end);
        if (s.dropped) return;
        await this.io.stage(s.stash, s.seq, bytes);
        if (s.dropped) return;
        s.seq++;
        s.at = { at: "sending", done: end };
        this.changed();
      }
      const sha256 = await this.io.digest(s.blob);
      if (s.dropped) return;
      const got = await this.io.commit({ sid: this.sid, key: s.stash, name: s.name, bytes: s.bytes, chunks, expect: { sha256 } });
      if (s.dropped) {
        void this.io.drop(this.sid, got.id).catch(() => {});
        return;
      }
      s.class = got.class;
      s.at = { at: "done", id: got.id };
    } catch (e) {
      if (s.dropped) return;
      if (this.io.offline(e)) s.at = { at: "offline", done: Math.min(s.bytes, s.seq * CHUNK) };
      else {
        s.at = { at: "failed", said: saidOf(e) || copyText("attach.chip.unknown"), retry: true, detail: detailOf(e) };
        s.stash = "";
        s.seq = 0;
      }
    }
    this.changed();
  }
}

/** 一块 / 一问的期限（远端一块 512 KiB 十六进制，慢线上也够）。 */
const STEP_BUDGET_MS = 30_000;

const hex = (b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

/** 生产那一份：经通道问会话那台（本机远端同一条路）。 */
export function channelIo(origin: Origin): AttachIo {
  return {
    check: async (sid, items) =>
      (await settleTyped(origin, "session-attachment-check", callTyped(origin, "session-attachment-check", { sid, items }, budgetWithin(STEP_BUDGET_MS)))).items,
    stage: async (key, seq, bytes) => {
      await settleTyped(origin, "files-stage-chunk", callTyped(origin, "files-stage-chunk", { key, seq, content: { b16: hex(bytes) } }, budgetWithin(STEP_BUDGET_MS)));
    },
    commit: (req) => settleTyped(origin, "session-attachment-commit", callTyped(origin, "session-attachment-commit", req, budgetWithin(STEP_BUDGET_MS))),
    drop: async (sid, id) => {
      await settleTyped(origin, "session-attachment-drop", callTyped(origin, "session-attachment-drop", { sid, id }, budgetWithin(STEP_BUDGET_MS)));
    },
    digest: async (blob) => hex(new Uint8Array(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer()))),
    read: async (blob, start, end) => new Uint8Array(await blob.slice(start, end).arrayBuffer()),
    newKey: () => hex(crypto.getRandomValues(new Uint8Array(16))),
    offline: (e) => e instanceof ChanError && e.error.layer === "hop",
  };
}
