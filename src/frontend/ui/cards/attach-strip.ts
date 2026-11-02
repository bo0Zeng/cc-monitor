/**
 * **消息流里人说的那一句带的附件**：画在气泡外、上方一排 —— 图片块画缩略图，别的附件（`said.attachments[]`，适配层认出、
 * 正文里那几行引用已去掉）画 chip（类别字 · 名字 · 大小，类别字与大小那台写好）。那台已清掉（`gone`）⇒ 虚线「已清理」、不可点。
 * 点开一份 ⇒ 调用方给的 `open`（文件窗口定位到那一份）；不给 ⇒ 不可点。
 *
 * 图片块只带引用（类型 · 字节数 · 哪一行 · 第几张），字节要另取：先画定尺寸的占位，`load` 取回才填 `src`。
 * 惰路（`lazy`）批里建的卡不当场取 —— 占位标 `data-image-pending`，进视口那一下由 `render.ts::enhanceCard` 叫 [`fillImage`]；
 * 取不到 ⇒ 占位留着，`title` 写那一句。
 */
import { copyText } from "../copy-table";
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import type { Machine } from "../ipc/origin";
import { saidOf } from "../kit/detail";
import type { Attachment } from "../generated/Attachment";
import type { Block } from "../generated/Block";
import s from "./attach-strip.module.css";

/** 一张图的引用（记录里那一块）。 */
export type ImageRef = Extract<Block, { type: "image" }>;

/** 图怎么取：引用 ⇒ 能放进 `src` 的地址。 */
export interface ImageLoad {
  load: (image: ImageRef) => Promise<string>;
  /** 惰路的批里建的卡：进视口再取。 */
  lazy: boolean;
}

/**
 * **按引用取回一张图**（`history-image`）：哪台 · 哪份记录 · 那一条的 `id` · 那一块的 `at` · `n`，回能放进 `<img src>` 的地址。
 * 取不到 ⇒ 抛（那一句是后端 / 那一跳写的）。
 */
export async function fetchImage(origin: Machine, jsonlPath: string, id: string, { at, n }: ImageRef): Promise<string> {
  try {
    const got = await callTyped(origin, "history-image", { path: jsonlPath, at, n, id }, budgetWithin(IMAGE_BUDGET_MS));
    return `data:${got.mediaType};base64,${got.data}`;
  } catch (e) {
    throw new Error(saidOf(e));
  }
}

/** 取一张图的期限（同记录读面那一段）。 */
const IMAGE_BUDGET_MS = 120_000;

/** 等着进视口的占位 ⇒ 去取它的那一下。 */
const waiting = new WeakMap<HTMLImageElement, () => void>();

/** 进视口了：取这一张（`render.ts::enhanceCard` 叫；不是等着的占位 ⇒ 不做事）。 */
export function fillImage(img: HTMLElement): void {
  if (!(img instanceof HTMLImageElement)) return;
  const go = waiting.get(img);
  waiting.delete(img);
  delete img.dataset.imagePending;
  go?.();
}

function placeholder(image: ImageRef, how: ImageLoad | undefined): HTMLImageElement {
  const img = document.createElement("img");
  img.className = s.saImage;
  img.alt = copyText("attach.msg.image");
  if (!how) return img;
  const go = () =>
    void how.load(image).then(
      (src) => {
        img.src = src;
      },
      (e: unknown) => {
        img.title = e instanceof Error ? e.message : String(e);
      },
    );
  if (how.lazy) {
    img.dataset.imagePending = "";
    waiting.set(img, go);
  } else go();
  return img;
}

/** 这一句的附件那一排；没有图片也没有附件 ⇒ `null`（不建）。 */
export function attachStrip(blocks: readonly Block[], attachments: readonly Attachment[], open?: (rel: string) => void, images?: ImageLoad): HTMLElement | null {
  const refs = blocks.filter((b): b is ImageRef => b.type === "image");
  if (refs.length === 0 && attachments.length === 0) return null;
  const strip = document.createElement("div");
  strip.className = s.saStrip;
  strip.dataset.role = "said-attachments";
  if (refs.length > 0) {
    const row = document.createElement("div");
    row.className = s.saImages;
    for (const r of refs) row.appendChild(placeholder(r, images));
    strip.appendChild(row);
  }
  if (attachments.length > 0) {
    const row = document.createElement("div");
    row.className = s.saFiles;
    for (const a of attachments) row.appendChild(fileChip(a, open));
    strip.appendChild(row);
  }
  return strip;
}

function fileChip(a: Attachment, open?: (rel: string) => void): HTMLElement {
  const clickable = !a.gone && open !== undefined;
  const chip = document.createElement(clickable ? "button" : "span");
  if (chip instanceof HTMLButtonElement) {
    chip.type = "button";
    chip.addEventListener("click", () => open?.(a.rel));
  }
  chip.className = s.saFile;
  chip.dataset.role = "said-attachment";
  chip.dataset.gone = String(a.gone === true);
  chip.title = a.rel;
  const tag = document.createElement("span");
  tag.className = s.saTag;
  tag.textContent = a.classText;
  const name = document.createElement("span");
  name.className = s.saName;
  name.textContent = a.name;
  const size = document.createElement("span");
  size.className = s.saSize;
  size.textContent = a.gone ? copyText("attach.msg.cleared") : (a.sizeText ?? "");
  chip.append(tag, name, size);
  return chip;
}
