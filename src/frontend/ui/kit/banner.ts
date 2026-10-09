/**
 * 错误条 · 警告条：放在出错的那块顶上；`--bg-2` 底、左 3px 色条、16px 图标、一句话 ＋ 右侧动作。
 * 不带关闭 ×：事解决了条由调用方摘掉。
 * `detail`（出错那一端写好的复制详情）非空 ⇒ 动作最后多一颗［复制详情］；复制不了时那块原文展开在条里下一行。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「错误条 · 空态」那一节。
 */
import { icon } from "./icon";
import { copyDetailButton } from "./detail";
import s from "./banner.module.css";

export type BannerTone = "error" | "warn";

export function banner(tone: BannerTone, text: string, actions: HTMLElement[] = [], detail = ""): HTMLDivElement {
  const b = document.createElement("div");
  b.className = s.banner;
  b.dataset.intent = tone;
  b.dataset.detailHost = "";
  b.setAttribute("role", tone === "error" ? "alert" : "status");
  const t = document.createElement("span");
  t.className = s.bannerText;
  t.textContent = text;
  const copy = copyDetailButton(text, detail);
  b.append(icon(tone === "error" ? "error" : "warning"), t, ...actions, ...(copy ? [copy] : []));
  return b;
}
