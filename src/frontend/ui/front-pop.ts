/**
 * ↗ 的三样回执：在飞超过 300ms 在 ↗ 旁出「查找终端…」· 切过去了 ↗ 换成对勾 1 秒 · 其余结局一个浮层锚在 ↗ 下面
 * （不自动消失；Esc / 点外面 / × 关；按钮做完就关；同一个会话再失败一次 ⇒ 原地换内容，不另弹）。
 */
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { closePopover, openPopover, popoverOpenOn, swapPopoverContent } from "./kit/popover";
import { copyText } from "./copy-table";
import { frontActLabel, type FrontAct, type FrontView } from "./front-result";
import s from "./front-pop.module.css";

/** 对勾停多久。 */
export const FRONT_DONE_MS = 1000;

const busy = new Map<HTMLElement, HTMLElement>();

/** 「查找终端…」：贴在 ↗ 下面（在飞超过 300ms 才叫到这里）。 */
export function setFrontBusy(anchor: HTMLElement | null, on: boolean): void {
  if (!anchor) return;
  const had = busy.get(anchor);
  if (!on) {
    had?.remove();
    busy.delete(anchor);
    return;
  }
  if (had) return;
  const tag = document.createElement("span");
  tag.className = s.fpBusy;
  tag.dataset.role = "front-busy";
  tag.textContent = copyText("front.pending.label");
  document.body.appendChild(tag);
  // 贴在 ↗ 正下方（结局浮层出来也在那里），不盖住旁边那几颗按钮；右端对齐，躲窗口右边。
  const r = anchor.getBoundingClientRect();
  const w = tag.getBoundingClientRect().width;
  tag.style.left = `${Math.max(4, Math.round(Math.min(r.right, window.innerWidth - 4) - w))}px`;
  tag.style.top = `${Math.round(r.bottom + 4)}px`;
  busy.set(anchor, tag);
}

/** 切过去了：↗ 的图标换成对勾 1 秒。 */
export function flashFrontDone(anchor: HTMLElement | null): void {
  if (!anchor) return;
  const was = anchor.querySelector("svg");
  if (!was) return;
  const ok = icon("check", was.getAttribute("data-size") === "compact" ? "compact" : "regular");
  ok.dataset.role = "front-done";
  was.replaceWith(ok);
  anchor.dataset.done = "true";
  // 调度：一次性 —— ↗ 切过去了：1 秒后把对勾换回 ↗
  window.setTimeout(() => {
    ok.replaceWith(was);
    delete anchor.dataset.done;
  }, FRONT_DONE_MS);
}

/** 此刻浮层是给哪个会话开的（同一个会话再来一次 ⇒ 原地换内容）。 */
let shownFor: { sid: string; anchor: HTMLElement } | null = null;

/** 结局浮层。`act` 接按钮（做完浮层就关）。 */
export function showFrontResult(anchor: HTMLElement, sid: string, view: FrontView, act: (a: FrontAct) => void | Promise<void>): void {
  const box = document.createElement("div");
  box.className = s.fpBox;
  box.dataset.shade = view.tone;
  box.dataset.role = "front-result";
  const head = document.createElement("div");
  head.className = s.fpHead;
  const title = document.createElement("div");
  title.className = s.fpTitle;
  title.textContent = view.title;
  head.append(title, button({ label: copyText("front.pop.close"), kind: "icon", size: "compact", icon: "close", onClick: () => closePopover() }));
  box.appendChild(head);
  if (view.body !== "") {
    const body = document.createElement("div");
    body.className = s.fpBody;
    body.textContent = view.body;
    box.appendChild(body);
  }
  if (view.hint) {
    const hint = document.createElement("div");
    hint.className = s.fpHint;
    hint.textContent = view.hint;
    box.appendChild(hint);
  }
  if (view.acts.length > 0) {
    const acts = document.createElement("div");
    acts.className = s.fpActs;
    for (const a of view.acts) {
      acts.appendChild(
        button({
          label: frontActLabel(a),
          size: "compact",
          onClick: () => {
            closePopover();
            void act(a);
          },
        }),
      );
    }
    box.appendChild(acts);
  }
  if (shownFor?.sid === sid && shownFor.anchor === anchor && popoverOpenOn(anchor) && swapPopoverContent(anchor, box)) return;
  if (popoverOpenOn(anchor)) closePopover();
  shownFor = { sid, anchor };
  openPopover(anchor, box, {
    label: copyText("front.pop.aria"),
    onClose: () => {
      if (shownFor?.anchor === anchor) shownFor = null;
    },
  });
}

/** ［复制详情］：那一族的细节原文复制给人看（不贴进任何配置）。 */
export function copyFrontDetail(text: string): Promise<void> {
  return navigator.clipboard?.writeText(text).catch(() => {}) ?? Promise.resolve();
}

/** 这个会话切过去了：它开着的结局浮层收起。 */
export function closeFrontResult(sid: string): void {
  if (shownFor?.sid === sid) closePopover();
}
