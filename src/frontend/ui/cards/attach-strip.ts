/**
 * **消息流里人说的那一句带的附件**：画在气泡上方一排 —— 图片块（记录里自带原图字节）画缩略图，别的附件（`said.attachments[]`，适配层认出、
 * 正文里那几行引用已去掉）画 chip（类别字 · 名字 · 大小，类别与大小那台写好）。那台已清掉（`gone`）⇒ 虚线「已清理」、不可点。
 * 点开一份 ⇒ 调用方给的 `open`（文件窗口定位到那一份）；不给 ⇒ 不可点。
 */
import { copyText } from "../copy-table";
import { classWord } from "../attach-row";
import type { Attachment } from "../generated/Attachment";
import type { Block } from "../generated/Block";
import s from "./attach-strip.module.css";

/** 图片块的来源（照那一家的写法透传）里认得出的那一形：base64 ＋ 图片类型。 */
function imageSrc(source: unknown): string | null {
  if (typeof source !== "object" || source === null) return null;
  const o = source as { type?: unknown; media_type?: unknown; data?: unknown };
  if (o.type !== "base64" || typeof o.media_type !== "string" || !o.media_type.startsWith("image/") || typeof o.data !== "string") return null;
  return `data:${o.media_type};base64,${o.data}`;
}

/** 这一句的附件那一排；没有图片也没有附件 ⇒ `null`（不建）。 */
export function attachStrip(blocks: readonly Block[], attachments: readonly Attachment[], open?: (rel: string) => void): HTMLElement | null {
  const images = blocks.flatMap((b) => (b.type === "image" ? [imageSrc(b.source)] : [])).filter((x): x is string => x !== null);
  if (images.length === 0 && attachments.length === 0) return null;
  const strip = document.createElement("div");
  strip.className = s.strip;
  strip.dataset.role = "said-attachments";
  if (images.length > 0) {
    const row = document.createElement("div");
    row.className = s.images;
    for (const src of images) {
      const img = document.createElement("img");
      img.className = s.image;
      img.src = src;
      img.alt = copyText("attach.msg.image");
      img.loading = "lazy";
      row.appendChild(img);
    }
    strip.appendChild(row);
  }
  if (attachments.length > 0) {
    const row = document.createElement("div");
    row.className = s.files;
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
  chip.className = s.file;
  chip.dataset.role = "said-attachment";
  chip.dataset.gone = String(a.gone === true);
  chip.title = a.rel;
  const tag = document.createElement("span");
  tag.className = s.tag;
  tag.textContent = classWord(a.class);
  const name = document.createElement("span");
  name.className = s.name;
  name.textContent = a.name;
  const size = document.createElement("span");
  size.className = s.size;
  size.textContent = a.gone ? copyText("attach.msg.cleared") : (a.sizeText ?? "");
  chip.append(tag, name, size);
  return chip;
}
