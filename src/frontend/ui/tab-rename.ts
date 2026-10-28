/**
 * **改一个标签页的名**：名字住会话那台的注解（`history-annotate {customTitle}`，历史页「改标题」同一格），
 * 叫它什么由那台后端定（改过的名 ＞ agent 起的标题 ＞ 第一句）、随帧交来（`Tab.label`）—— 这里只交命令，tab 上的名字等推帧来了才换。
 *
 * 入口：标题双击 · 右键「改标题」· 命令面板「改标题」，都经 {@link renameTab}；历史页「改标题」经 {@link renameOver}（交法 · 撤销 · 失败那一句同一处）。
 * 只有就地一种样子：有标签页栏的窗口 ⇒ 栏上就地改（栏建起来时经 {@link mountInline} 登记自己）；没有（撕出来的查看窗）、或那一行此刻看不见 ⇒ 会话头标题上就地改。
 */
import type { Tab } from "./tab-model";
import { annotate } from "./history-reads";
import { failToast, undoToast } from "./kit/toast";
import { copyText } from "./copy-table";
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
import rs from "./tab-group-rename.module.css";

/** 栏登记的「在栏上就地改这一个」：改得了 ⇒ `true`（开了输入框）；这一行不在栏上 / 看不见 ⇒ `false`。 */
type Inline = (tab: Tab) => boolean;

let inline: Inline | null = null;

/** 标签页栏建起来时登记就地改的那一口（一个窗口一个栏）；回摘掉它的函数。 */
export function mountInline(begin: Inline): () => void {
  inline = begin;
  return () => {
    if (inline === begin) inline = null;
  };
}

/** 改这个标签页的名：栏上就地改；这一行不在栏上（撕出来的查看窗 · 看不见）⇒ 在会话头的标题上就地改。 */
export function renameTab(tab: Tab): void {
  if (inline?.(tab) === true) return;
  const head = document.querySelector<HTMLElement>("[data-head-title]");
  if (head !== null) renameOver(head, tab, tab.label ?? "");
}

/** 在 `over` 那一格上就地改名（历史页那一行 · 会话头标题）：`initial` 是框里的初值；取消 ⇒ 不交。 */
export function renameOver(over: HTMLElement, who: Renamed, initial: string): void {
  editOver(over, initial, (next) => {
    if (next !== null) void commitRename(who, next);
  });
}

/** 改谁：会话那一台 · 会话 · 此刻的名（与它相同 ⇒ 不交）。 */
export type Renamed = Pick<Tab, "origin" | "sessionId" | "label">;

const failed = (e: unknown): void => void failToast(copyText("history.rename.failed"), e);

/**
 * 交改好的名：去掉首尾空白；空 ⇒ 交空串（回到原标题）；与此刻的名相同 ⇒ 不交。
 * 交成 ⇒ 可撤销的提示，撤销照回包里改之前那一格（`prev_title`；没有 ⇒ 空串）交回去。交给会话那一台（`origin`）。
 */
export async function commitRename(who: Renamed, next: string): Promise<void> {
  const name = next.trim();
  if (name === (who.label ?? "")) return;
  const { origin, sessionId } = who;
  try {
    const r = await annotate(origin, sessionId, { customTitle: name });
    undoToast(copyText("history.rename.done"), () => void annotate(origin, sessionId, { customTitle: r.prev_title ?? "" }).catch(failed), () => {});
  } catch (e) {
    failed(e);
  }
}

/**
 * 在 `over` 那一格上浮出一个输入框（挂在 body 上、按那一格此刻的位置摆：标签页是一颗按钮，里面不放输入框）。
 * Enter 定 · 失焦定 · 外面一滚 / 窗口一缩 ⇒ 定；Esc 取消（走浮层栈：只取消改名，不连带关别的）。`done(值)`：定 ⇒ 框里的值，取消 ⇒ `null`；只调一次。
 */
export function editOver(over: HTMLElement, initial: string, done: (next: string | null) => void): void {
  const at = over.getBoundingClientRect();
  const input = document.createElement("input");
  input.type = "text";
  input.className = `${rs.input} ${rs.floating}`;
  input.value = initial;
  input.placeholder = copyText("history.rename.prompt");
  input.setAttribute("aria-label", copyText("history.rename.label"));
  input.style.left = `${at.left}px`;
  input.style.top = `${at.top}px`;
  input.style.width = `${Math.max(at.width, 120)}px`;
  let finished = false;
  const overlay: OverlayHandle = {
    handleEsc: () => {
      finish(false);
      return true;
    },
  };
  const moved = (): void => finish(true);
  const finish = (commit: boolean): void => {
    if (finished) return;
    finished = true;
    dispatcher.popOverlay(overlay);
    window.removeEventListener("scroll", moved, true);
    window.removeEventListener("resize", moved);
    const value = input.value;
    input.remove();
    done(commit ? value : null);
  };
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.isComposing) {
      ev.preventDefault();
      finish(true);
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      finish(false);
    }
  });
  input.addEventListener("blur", () => finish(true));
  document.body.append(input);
  window.addEventListener("scroll", moved, true);
  window.addEventListener("resize", moved);
  dispatcher.pushOverlay(overlay);
  input.focus();
  input.select();
}
