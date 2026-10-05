/**
 * 读数据的区块（I5）：七态一个家 —— 加载 · 空 · 出错 · 过期 · 部分 · 很多 · 禁用（外加「有数据」）。
 *
 * - 加载：<300ms 什么都不画；之后骨架；>10s 写正在做什么。
 * - 出错 ≠ 空：出错是区块内错误条 ＋［重试］，不留「加载中」。
 * - 过期：照常显示旧数据，顶上一条警告条（`devbox 离线 · 采样 3m 前`）＋［重试］；旧数据不灰。
 * - 部分：答了的照常显示，没答的那台单独一行状态，不拖住全表、不静默漏掉。
 * - 很多：底下一行 `前 500 · 搜索可找全部`。
 * - 禁用：灰 ＋ 悬停说为什么。
 * - 刷新（已有数据时再读）：不清空、不出骨架，右上角转圈。
 * 区块里说什么由调用方给（文案表）；本件只管摆法与时机。
 */
import { banner } from "./banner";
import { button } from "./button";
import { emptyState, type EmptySpec } from "./empty";
import { spinner } from "./progress";
import { skeletonRows } from "./skeleton";
import { copyText } from "../copy-table";
import s from "./block.module.css";

export type BlockState =
  | { kind: "loading"; doing?: string }
  | { kind: "ready"; content: HTMLElement }
  | { kind: "empty"; empty: EmptySpec }
  | { kind: "error"; text: string; retry: () => void }
  | { kind: "stale"; text: string; retry: () => void; content: HTMLElement }
  | { kind: "partial"; content: HTMLElement; missing: string[] }
  | { kind: "many"; content: HTMLElement; note: string }
  | { kind: "disabled"; why: string; content?: HTMLElement };

export const BLOCK_SKELETON_AFTER_MS = 300;
export const BLOCK_SAY_DOING_AFTER_MS = 10_000;

export interface BlockHandle {
  root: HTMLDivElement;
  show(state: BlockState): void;
  /** 已有数据时再读：右上角转圈，内容不动。 */
  refreshing(on: boolean): void;
}

export function dataBlock(): BlockHandle {
  const root = document.createElement("div");
  root.className = s.block;
  root.setAttribute("aria-live", "polite");
  const body = document.createElement("div");
  body.className = s.blockBody;
  root.appendChild(body);
  let timers: ReturnType<typeof setTimeout>[] = [];
  let busy: HTMLElement | null = null;
  const clear = (): void => {
    for (const t of timers) clearTimeout(t);
    timers = [];
  };
  const retryButton = (run: () => void): HTMLButtonElement => button({ label: copyText("kit.block.retry"), size: "compact", onClick: run });
  const show = (st: BlockState): void => {
    clear();
    root.dataset.state = st.kind;
    root.removeAttribute("title");
    body.replaceChildren();
    switch (st.kind) {
      case "loading": {
        timers.push(setTimeout(() => body.replaceChildren(skeletonRows()), BLOCK_SKELETON_AFTER_MS));
        if (st.doing) {
          const doing = st.doing;
          timers.push(
            setTimeout(() => {
              const d = document.createElement("div");
              d.className = s.blockDoing;
              d.append(spinner(), document.createTextNode(doing));
              body.appendChild(d);
            }, BLOCK_SAY_DOING_AFTER_MS),
          );
        }
        return;
      }
      case "ready":
        body.appendChild(st.content);
        return;
      case "empty":
        body.appendChild(emptyState(st.empty));
        return;
      case "error":
        body.appendChild(banner("error", st.text, [retryButton(st.retry)]));
        return;
      case "stale":
        body.append(banner("warn", st.text, [retryButton(st.retry)]), st.content);
        return;
      case "partial": {
        body.appendChild(st.content);
        for (const m of st.missing) {
          const r = document.createElement("div");
          r.className = s.blockMissing;
          r.textContent = m;
          body.appendChild(r);
        }
        return;
      }
      case "many": {
        const n = document.createElement("div");
        n.className = s.blockNote;
        n.textContent = st.note;
        body.append(st.content, n);
        return;
      }
      case "disabled":
        root.title = st.why;
        if (st.content) body.appendChild(st.content);
        return;
    }
  };
  return {
    root,
    show,
    refreshing(on) {
      if (on && !busy) {
        busy = document.createElement("span");
        busy.className = s.blockBusy;
        busy.appendChild(spinner());
        root.appendChild(busy);
      } else if (!on && busy) {
        busy.remove();
        busy = null;
      }
    },
  };
}
