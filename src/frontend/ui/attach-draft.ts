/**
 * **桌面附件草稿**：一条会话那一条没送的话里带的附件（按会话各一份，宿主记着）。
 *
 * 那一圈（问收不收 · 逐块送 · 提交 · 断了等连上再传 · 不收的标上）是本机后端的事（`backend/control/attach_upload.rs`，
 * 同手机嵌的核心那一份 `chunk_core::attach`）；这里只交名单、照它推来的那一排画：
 * - 加一份：字节交壳放进本机暂存区（`attach_stage`，webview 只给 File 对象、没有本机路径）⇒ 那条路径交 `attach-draft-put`。
 * - ✕ ⇒ `attach-draft-drop`；［重试］⇒ `attach-draft-retry`；送出那一下 ⇒ `attach-draft-take`（全落好了才回编号）。
 * - 那一排什么样只照本机后端那一份（命令回的 · `changed/attach` 推的，key 是这条草稿号；推的没带体 ⇒ 重问 `attach-draft-view`）。
 *
 * 问本机后端的几口与交壳那一口都经 [`AttachIo`]，测试换假的。
 */
import { copyText } from "./copy-table";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { commands } from "./ipc/commands";
import { LOCAL, type Machine } from "./ipc/origin";
import { pushedProducts } from "./changed-stream";
import { subscribeTopics } from "./events";
import { detailOf, saidOf } from "./kit/detail";
import { ChanError, decodeFail, detailOfFail, saidOfFail } from "../../comms/inward/chan";
import type { AttachDraftCard } from "./generated/AttachDraftCard";
import type { AttachDraftShown } from "./generated/AttachDraftShown";
import type { AttachDraftShownCard } from "./generated/AttachDraftShownCard";
import type { FileClass } from "./generated/FileClass";

/** 交壳 · 问本机后端的几口（缺省走壳与通道；测试换假的）。`draft` 是这条草稿号。 */
export interface AttachIo {
  /** 那一份字节交壳放进本机暂存区 ⇒ 那条路径。 */
  stage(blob: Blob): Promise<string>;
  put(draft: string, sid: string, cards: AttachDraftCard[]): Promise<AttachDraftShown>;
  drop(draft: string, key: string): Promise<AttachDraftShown>;
  retry(draft: string): Promise<AttachDraftShown>;
  /** 全落好了 ⇒ 编号（那一排清空）；还有没落好的 ⇒ `null`。 */
  take(draft: string): Promise<string[] | null>;
  view(draft: string): Promise<AttachDraftShown>;
  /** 订这条草稿那一排（推来的带体 ⇒ 那一排；没带 · 可能漏了 ⇒ `null`，调用方重问）；回的函数退订。 */
  watch(draft: string, on: (shown: AttachDraftShown | null) => void): () => void;
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
  /** 这一份在草稿里的键（✕ 按它找）。 */
  readonly key: string;
  readonly name: string;
  readonly bytes: number;
  /** 本机那一份（画小图）；已不在这一窗里 ⇒ `null`。 */
  readonly blob: Blob | null;
  /** 那一种（本机后端写；还没到 ⇒ `null`）。 */
  readonly class: FileClass | null;
  readonly classText: string;
  readonly at: ChipAt;
}

/** 加进来的一份：名字 · 字节 · 是不是目录（拖进来的目录）。 */
export interface AttachInput {
  name: string;
  blob: Blob;
  dir?: boolean;
}

/** 本机后端那一张卡 ⇒ chip 在哪（`offline` ＝ 那一排断着）。 */
function atOf(c: AttachDraftShownCard, offline: boolean): ChipAt {
  switch (c.phase.kind) {
    case "queued":
      return offline ? { at: "offline", done: 0 } : { at: "queued" };
    case "moving":
      return { at: "sending", done: c.done };
    case "landed":
      return { at: "done", id: c.id ?? "" };
    case "refused":
      return { at: "failed", said: c.phase.said, retry: false, detail: "" };
    case "failed":
      return { at: "failed", said: c.phase.said, retry: true, detail: "" };
    case "lost":
      return { at: "failed", said: c.line ?? "", retry: true, detail: "" };
  }
}

/** 这一窗里的草稿号（一扇窗一条会话一个；本机后端按它分排）。 */
function freshDraft(): string {
  return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function" ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`;
}

export class AttachDraft {
  readonly draft = freshDraft();
  /** 本机后端最近给的那一排。 */
  private shown: AttachDraftShown | null = null;
  /** 交出去了、本机后端那一排里还没有的那几份（字节在交壳 · put 在路上）。 */
  private readonly pending: { key: string; name: string; bytes: number; blob: Blob }[] = [];
  /** 本机那几份（按键；画小图）。 */
  private readonly blobs = new Map<string, Blob>();
  /** 交壳 · put 没成的那几份（键 ⇒ 那一句）。 */
  private readonly lost = new Map<string, { name: string; bytes: number; said: string; detail: string }>();
  private seq = 0;
  private unwatch: (() => void) | null = null;

  constructor(
    readonly sid: string,
    private readonly io: AttachIo,
    private readonly changed: () => void,
  ) {}

  /** 此刻的几颗（按草稿先后）：本机后端那一排 ＋ 还在交的 ＋ 没交成的。 */
  chips(): readonly Chip[] {
    const offline = this.shown?.offline === true;
    const out: Chip[] = (this.shown?.cards ?? []).map((c) => ({
      key: c.key,
      name: c.name,
      bytes: c.bytes,
      blob: this.blobs.get(c.key) ?? null,
      class: c.class,
      classText: c.class_text,
      at: atOf(c, offline),
    }));
    for (const p of this.pending) out.push({ key: p.key, name: p.name, bytes: p.bytes, blob: p.blob, class: null, classText: "", at: { at: "queued" } });
    for (const [key, l] of this.lost) out.push({ key, name: l.name, bytes: l.bytes, blob: this.blobs.get(key) ?? null, class: null, classText: "", at: { at: "failed", said: l.said, retry: false, detail: l.detail } });
    return out;
  }

  /** 加一批：字节交壳放进本机暂存区，名单交本机后端。 */
  add(files: readonly AttachInput[]): void {
    if (files.length === 0) return;
    this.follow();
    const batch = files.map((f) => ({ key: String(++this.seq), name: f.name, bytes: f.blob.size, blob: f.blob, dir: f.dir === true }));
    for (const b of batch) {
      this.pending.push(b);
      this.blobs.set(b.key, b.blob);
    }
    this.changed();
    void this.send(batch);
  }

  private async send(batch: { key: string; name: string; bytes: number; blob: Blob; dir: boolean }[]): Promise<void> {
    const cards: AttachDraftCard[] = [];
    for (const b of batch) {
      try {
        const path = b.dir ? "" : await this.io.stage(b.blob);
        cards.push({ key: b.key, name: b.name, path, bytes: b.bytes, ...(b.dir ? { dir: true } : {}) });
      } catch (e) {
        this.settlePending(b.key);
        this.lost.set(b.key, { name: b.name, bytes: b.bytes, said: saidOf(e), detail: detailOf(e) });
      }
    }
    if (cards.length > 0) {
      try {
        this.take_(await this.io.put(this.draft, this.sid, cards));
      } catch (e) {
        for (const c of cards) {
          this.settlePending(c.key);
          this.lost.set(c.key, { name: c.name, bytes: c.bytes, said: saidOf(e), detail: detailOf(e) });
        }
      }
    }
    this.changed();
  }

  private settlePending(key: string): void {
    const i = this.pending.findIndex((p) => p.key === key);
    if (i >= 0) this.pending.splice(i, 1);
  }

  /** 收下本机后端那一排（交出去还没在里面的照旧挂着）。 */
  private take_(shown: AttachDraftShown): void {
    this.shown = shown;
    for (const c of shown.cards) this.settlePending(c.key);
  }

  /** 订这条草稿那一排（头一次加的时候订）。 */
  private follow(): void {
    if (this.unwatch !== null) return;
    this.unwatch = this.io.watch(this.draft, (shown) => {
      if (shown !== null) {
        this.take_(shown);
        this.changed();
        return;
      }
      void this.io.view(this.draft).then(
        (s) => {
          this.take_(s);
          this.changed();
        },
        () => {},
      );
    });
  }

  /** ✕ 一份。 */
  remove(key: string): void {
    this.blobs.delete(key);
    if (this.lost.delete(key)) return void this.changed();
    void this.io.drop(this.draft, key).then(
      (s) => {
        this.take_(s);
        this.changed();
      },
      () => {},
    );
  }

  /** 失败那颗上的［重试］：本机后端那一排里失败的都从头传。 */
  retry(): void {
    void this.io.retry(this.draft).then(
      (s) => {
        this.take_(s);
        this.changed();
      },
      () => {},
    );
  }

  /** 送出那一下：全落好了 ⇒ 编号（按先后）；还有没落好的 ⇒ `null`（不送）。没有附件 ⇒ 空。 */
  async ids(): Promise<string[] | null> {
    if (this.pending.length > 0 || this.lost.size > 0) return null;
    if (this.shown === null || this.shown.cards.length === 0) return [];
    return this.io.take(this.draft);
  }

  /** 送到了：chip 清空（那台的文件留着，消息里引着它）。 */
  clear(): void {
    this.shown = null;
    this.pending.length = 0;
    this.lost.clear();
    this.blobs.clear();
    this.changed();
  }

  /** 挡着送出的那一句（一句，不数个数）：有失败 ⇒「附件失败」；断着 ⇒ `offline`（连接层写好的离线那一句）；有没传完的 ⇒「附件传输中」；都传完了 ⇒ `null`。 */
  blocker(offline: string): string | null {
    const { failed, cut, pending } = this.tally();
    if (failed) return copyText("attach.blocked.failed");
    if (cut) return offline;
    return pending ? copyText("attach.blocked.sending") : null;
  }

  /** 有没有失败的 · 有没有断着的 · 有没有还没传完的。 */
  private tally(): { failed: boolean; cut: boolean; pending: boolean } {
    const chips = this.chips();
    return {
      failed: chips.some((c) => c.at.at === "failed"),
      cut: chips.some((c) => c.at.at === "offline"),
      // 全落好没有只照本机后端那一排（`ready`）；还在交的那几份也算没传完。
      pending: this.pending.length > 0 || this.shown?.ready === false,
    };
  }
}

/** 一问的期限（问本机后端）。 */
const STEP_BUDGET_MS = 30_000;

/** 生产那一份：字节交壳、名单与那几问交本机后端（会话在 `origin` 那台）。 */
export function channelIo(origin: Machine): AttachIo {
  const ask = budgetWithin(STEP_BUDGET_MS);
  return {
    stage: async (blob) =>
      commands.attach_stage(new Uint8Array(await blob.arrayBuffer())).catch((raw: unknown) => {
        // 失败是通道那一形（`chan/webview.rs::Fail`，本侧 `ours` 层）：照 `chan.call` 解回三层。
        throw new ChanError(decodeFail(raw), saidOfFail(raw), detailOfFail(raw));
      }),
    put: (draft, sid, cards) => callTyped(LOCAL, "attach-draft-put", { draft, machine: origin, sid, cards }, ask),
    drop: (draft, key) => callTyped(LOCAL, "attach-draft-drop", { draft, key }, ask),
    retry: (draft) => callTyped(LOCAL, "attach-draft-retry", { draft }, ask),
    take: async (draft) => (await callTyped(LOCAL, "attach-draft-take", { draft }, ask)).ids,
    view: (draft) => callTyped(LOCAL, "attach-draft-view", { draft }, ask),
    watch: (draft, on) => {
      let stop: (() => void) | null = null;
      let gone = false;
      void subscribeTopics([LOCAL], ["attach"], (_o, changes) => {
        const c = changes.get("attach");
        if (c === undefined) return;
        const { got, ask: again, all } = pushedProducts(c, "attach");
        const mine = got.get(draft);
        if (mine !== undefined) on(mine);
        else if (all || again.includes(draft)) on(null);
      }).then(
        (s) => {
          if (gone) s();
          else stop = s;
        },
        (e: unknown) => console.warn("[attach-draft] 订不上本机后端那一排：", e),
      );
      return () => {
        gone = true;
        stop?.();
      };
    },
  };
}
