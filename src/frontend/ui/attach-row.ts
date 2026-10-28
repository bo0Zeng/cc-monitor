/**
 * **输入行上方那一排附件**（终端面板）：一颗颗 chip ＋ 输入框左边那颗［＋］，三个入口进同一排（［＋］系统文件框可多选 · 拖到面板上 · 输入框里粘贴文件 / 图片）。
 *
 * - chip：图片画本机那份的小图，别的画类别字（那台写好的 `class_text`）· 名字 · 状态（排队 · 传输中带一条细进度 · 已完成 · 失败带那台那一句）· ✕。
 *   离线那几颗画虚线、跟连接层写好的离线那一句；失败可重试的带［重试］；悬停看复制详情。
 * - 拖：面板上整块画目标框（放进哪条会话、哪台）；面板上没有可送的会话 ⇒ 不画、光标禁止。拖进来的目录照样进这一排，由那台判不收。
 * - 粘贴：剪贴板里只有文件 / 图片、没有字 ⇒ 进这一排（图片名 `image-<n>.<扩展名>`）；有字照旧贴字。
 *
 * 草稿与传都在 [`AttachDraft`]；这里只画、只接三个入口。宿主说此刻的草稿（没有 ⇒ 这一排与［＋］都收起）与送往哪里。
 */
import { copyText } from "./copy-table";
import { el } from "./kit/el";
import { button, setDisabled } from "./kit/button";
import type { AttachDraft, AttachInput, Chip, ChipAt } from "./attach-draft";
import s from "./attach-row.module.css";

export interface AttachRowHost {
  /** 此刻那条会话的草稿；`null` ＝ 此刻送不了附件（占位 · 没认出会话）。 */
  draft(): AttachDraft | null;
  /** 送往哪条会话、哪台（拖进来时框里那一行）。 */
  target(): { title: string; machine: string };
  /** 那台断着时 chip 上跟的那一句（连接层写好的离线那一句）。 */
  offline(): string;
  /** 输入此刻送不了的那一句（只读 · 离线）；`null` ＝ 送得了。 */
  blocked(): string | null;
}

/** 每一态的那个词（界面自己的态，不是线上的码）。 */
const STATE_WORD: { readonly [K in ChipAt["at"]]: () => string } = {
  queued: () => copyText("attach.chip.queued"),
  sending: () => copyText("attach.chip.sending"),
  offline: () => copyText("attach.chip.queued"),
  done: () => copyText("attach.chip.done"),
  failed: () => copyText("attach.chip.failed"),
};

/** 词后面跟的那一格：离线那一句 · 那台说的原因；别的态没有（传了多少只看进度条，不写字节）。 */
function stateDetail(c: Chip, offline: string): string {
  const at = c.at;
  if (at.at === "offline") return offline;
  if (at.at === "failed") return at.said;
  return "";
}

/** 粘贴进来的图片的扩展名（`image/png` ⇒ `png`；认不出 ⇒ `png`）。 */
function imageExt(type: string): string {
  const m = /^image\/([a-z0-9]+)/.exec(type);
  return m ? (m[1] === "jpeg" ? "jpg" : m[1]) : "png";
}

export class AttachRow {
  /** 输入行上方那一排（没有 chip ⇒ 收起）。 */
  readonly row: HTMLElement;
  /** 输入框左边那颗［＋］。 */
  readonly plus: HTMLButtonElement;
  private readonly picker: HTMLInputElement;
  /** 本机那份图片的小图（按 chip 记；chip 不在了就收回）。 */
  private readonly thumbs = new Map<Blob, string>();
  /** 这一条草稿里粘贴进来的图片的流水号（名字里的 n）。 */
  private pasted = 0;

  constructor(private readonly host: AttachRowHost) {
    this.row = el("div", s.attachRow);
    this.row.dataset.role = "attach-row";
    this.picker = el("input");
    this.picker.type = "file";
    this.picker.multiple = true;
    this.picker.style.display = "none";
    this.picker.addEventListener("change", () => {
      const files = Array.from(this.picker.files ?? []);
      this.picker.value = "";
      this.add(files.map((f) => ({ name: f.name, blob: f })));
    });
    this.plus = button({ label: copyText("attach.add.aria"), kind: "icon", size: "compact", icon: "plus", hint: copyText("attach.add.hint"), onClick: () => this.picker.click() });
    this.plus.dataset.role = "attach-plus";
    this.plus.appendChild(this.picker);
  }

  /** 加进此刻那条草稿（没有草稿 ⇒ 不收）。 */
  add(files: readonly AttachInput[]): void {
    if (files.length === 0) return;
    this.host.draft()?.add(files);
  }

  /** 输入框里粘贴：剪贴板里只有文件 / 图片、没有字 ⇒ 进这一排，回 `true`（调用方别再贴字）。 */
  paste(data: DataTransfer | null): boolean {
    if (!data || this.host.draft() === null || this.host.blocked() !== null) return false;
    const files = Array.from(data.files ?? []);
    if (files.length === 0 || data.getData("text/plain") !== "") return false;
    this.add(
      files.map((f) => {
        // 剪贴板里的截图没有自己的名字（浏览器给的是 `image.png` 一类）：按这一条里的先后编号。
        const shot = f.type.startsWith("image/") && (f.name === "" || /^image\.[a-z0-9]+$/i.test(f.name));
        return { name: shot ? `image-${++this.pasted}.${imageExt(f.type)}` : f.name, blob: f };
      }),
    );
    return true;
  }

  /** 面板 `zone` 收拖进来的文件：拖着时整块画目标框（盖在上面），松手进这一排。 */
  dropOn(zone: HTMLElement): void {
    const overlay = el("div", s.attachDrop);
    overlay.dataset.role = "attach-drop";
    overlay.hidden = true;
    const title = el("div", s.attachDropTitle, copyText("attach.drop.title"));
    const where = el("div", s.attachDropSub);
    overlay.append(title, where);
    zone.appendChild(overlay);
    const carriesFiles = (ev: DragEvent): boolean => Array.from(ev.dataTransfer?.types ?? []).includes("Files");
    const can = (): boolean => this.host.draft() !== null && this.host.blocked() === null;
    let depth = 0;
    zone.addEventListener("dragenter", (ev) => {
      if (!carriesFiles(ev)) return;
      ev.preventDefault();
      depth++;
      if (!can()) return;
      const t = this.host.target();
      where.textContent = [t.title, t.machine].filter((x) => x !== "").join(copyText("kit.text.sep"));
      overlay.hidden = false;
    });
    zone.addEventListener("dragover", (ev) => {
      if (!carriesFiles(ev)) return;
      ev.preventDefault();
      if (ev.dataTransfer) ev.dataTransfer.dropEffect = can() ? "copy" : "none";
    });
    zone.addEventListener("dragleave", (ev) => {
      if (!carriesFiles(ev)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) overlay.hidden = true;
    });
    zone.addEventListener("drop", (ev) => {
      if (!carriesFiles(ev)) return;
      ev.preventDefault();
      depth = 0;
      overlay.hidden = true;
      if (!can() || !ev.dataTransfer) return;
      const items = Array.from(ev.dataTransfer.items ?? []);
      const files: AttachInput[] = [];
      items.forEach((it) => {
        if (it.kind !== "file") return;
        const f = it.getAsFile();
        const dir = it.webkitGetAsEntry?.()?.isDirectory === true;
        if (f) files.push({ name: f.name, blob: f, ...(dir ? { dir: true } : {}) });
      });
      if (items.length === 0) for (const f of Array.from(ev.dataTransfer.files)) files.push({ name: f.name, blob: f });
      this.add(files);
    });
  }

  /** 照此刻的草稿重画这一排与［＋］。 */
  paint(): void {
    const draft = this.host.draft();
    // kit 按钮自带 display，`hidden` 切不动它 ⇒ 收起走 style。
    this.plus.style.display = draft === null ? "none" : "";
    setDisabled(this.plus, this.host.blocked());
    const chips = draft?.chips() ?? [];
    if (chips.length === 0) this.pasted = 0;
    const keep = new Set(chips.map((c) => c.blob));
    for (const [b, url] of this.thumbs) {
      if (keep.has(b)) continue;
      URL.revokeObjectURL?.(url);
      this.thumbs.delete(b);
    }
    this.row.hidden = chips.length === 0;
    if (draft === null) return void this.row.replaceChildren();
    const offline = this.host.offline();
    this.row.replaceChildren(...chips.map((c) => this.chipEl(draft, c, offline)));
  }

  private thumbOf(c: Chip): string | null {
    if (c.class !== "image" || typeof URL.createObjectURL !== "function") return null;
    let url = this.thumbs.get(c.blob);
    if (url === undefined) {
      url = URL.createObjectURL(c.blob);
      this.thumbs.set(c.blob, url);
    }
    return url;
  }

  private chipEl(draft: AttachDraft, c: Chip, offline: string): HTMLElement {
    const chip = el("div", s.attachChip);
    chip.dataset.role = "attach-chip";
    chip.dataset.state = c.at.at;
    const thumb = this.thumbOf(c);
    if (thumb !== null) {
      const img = el("img", s.attachThumb);
      img.src = thumb;
      img.alt = "";
      chip.appendChild(img);
    } else chip.appendChild(el("span", s.attachTag, c.classText));
    const name = el("span", s.attachName, c.name);
    const word = STATE_WORD[c.at.at]();
    const sep = copyText("kit.text.sep");
    const detail = stateDetail(c, offline);
    const state = el("span", s.attachState, detail === "" ? word : `${word}${sep}${detail}`);
    state.dataset.role = "attach-state";
    chip.append(name, state);
    const at = c.at;
    if (at.at === "failed") chip.title = at.detail === "" ? at.said : `${at.said}\n${at.detail}`;
    if (at.at === "failed" && at.retry) chip.appendChild(this.retryBtn(draft, c));
    chip.appendChild(button({ label: copyText("attach.chip.remove"), kind: "icon", size: "compact", icon: "close", hint: copyText("attach.chip.removeHint"), onClick: () => draft.remove(c.n) }));
    if (at.at === "sending" || at.at === "offline") {
      const bar = el("span", s.attachBar);
      bar.style.width = `${c.bytes === 0 ? 0 : Math.round((at.done / c.bytes) * 100)}%`;
      chip.appendChild(bar);
    }
    return chip;
  }

  private retryBtn(draft: AttachDraft, c: Chip): HTMLButtonElement {
    return button({ label: copyText("attach.chip.retry"), kind: "ghost", size: "compact", onClick: () => draft.retry(c.n) });
  }
}
