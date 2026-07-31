/**
 * G4/G5（branch-anywhere）：每条消息旁那个「从这一轮分叉」按钮 —— **只有这一份实现**。
 *
 * # G4：历史查看器与实时 tab 共用
 *
 * 钩子 `onCardRendered` 本来就在**共享**的 `render-stream-record.ts` 里，
 * 只是此前**只有 `session-viewer` 传了它** ⇒ 实时会话上没有入口。
 * 把按钮抽出来之后，两边传同一个东西（主计划 §3 账本第 3 行）。
 *
 * # G5：被 ESC 回退掉的那些消息 —— **保留入口，但呈现要区分**
 *
 * 用户拍板：**要给路口**。理由是那段数据只剩这一个出口 —— CC 的 TUI 没有任何路
 * 把废弃分支捞回来，删了就是删了。
 *
 * 但今天 on-main 与 off-main 的按钮**长得一模一样、tooltip 也一样**：
 * 从一段折叠起来的废弃对话里点它，用户不会意识到自己在**复活一条已经被自己否掉的路**。
 * **缺的是信息，不是限制。**
 *
 * ## 怎么判「这条是不是 off-main」：读效果，不重算
 *
 * 账本第 6 行要求「读 `computeMainBranch`，别另算一份主线」。这里做得更彻底 ——
 * **完全不碰主线集合**：`BranchFolder` 已经把 off-main 的连续卡片包进
 * `.branch-fold-wrap` 了，而那个包装**本身就是** `computeMainBranch` 的产物。
 *
 * ⇒ 判据 = `btn.closest(".branch-fold-wrap") !== null`。
 *
 * 三个好处：① 不可能与主线判定漂移（它就是主线判定的结果）；
 * ② **永远是最新的** —— 一条现在 on-main 的消息，被 ESC 回退之后会被重新包进 wrap，
 * 按钮的呈现自动跟着变，不需要任何刷新管线；③ 样式那半可以纯 CSS 后代选择器搞定。
 */

import { commands } from "./ipc/commands";
import { showActionFailureToast } from "./error-toast";
import type { BranchResult } from "./generated/BranchResult";

/** off-main 的卡片被 `BranchFolder` 包进这个容器里。判据的唯一锚点。 */
export const FOLD_WRAP_SELECTOR = ".branch-fold-wrap";

const TITLE_ON_MAIN =
  "从这一轮创建分支（复制到这条为止 → 新会话，原会话不变，可 resume）";
const TITLE_OFF_MAIN =
  "从这一轮创建分支 —— ⚠ 这条属于**被 ESC 回退掉的**分支。\n" +
  "分叉会把那条你当初放弃的对话复活成一个新会话（原会话仍然不变）。";

/** 这张卡此刻是不是在「已被 ESC 回退」的折叠块里。**每次都现查**，见模块头注。 */
export function isOffMainCard(el: Element): boolean {
  return el.closest(FOLD_WRAP_SELECTOR) !== null;
}

export interface BranchButtonOptions {
  /** 分叉点消息的 uuid。 */
  uuid: string;
  /** 源会话 jsonl 的绝对路径。 */
  jsonlPath: string;
  /** 新会话的工作目录（起会话用）。 */
  cwd?: string;
  /** 成功之后干什么（弹 toast / 起会话）——由调用方决定，本组件不管起会话。 */
  onForked: (res: BranchResult) => void;
}

/**
 * 给一张 user/assistant 卡挂分叉按钮。**幂等**：增量重渲会重复调本函数。
 *
 * 不在这里起会话 —— 那是 G3b 的事，且本地/远端两条路不同。本组件只负责
 * 「产出新会话文件」这一步，成功后把结果交给 `onForked`。
 */
export function attachBranchButton(
  cardEl: HTMLElement,
  opts: BranchButtonOptions,
): void {
  if (cardEl.querySelector(":scope > .viewer-branch-btn")) return; // 幂等
  cardEl.classList.add("has-branch-btn");

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "viewer-branch-btn";
  btn.textContent = "⑂";
  btn.title = TITLE_ON_MAIN;

  // tooltip 在**指上去的那一刻**才定 —— 一条消息的主线归属会随后续对话变化
  // （ESC 回退会把原本 on-main 的一段甩成 off-main）。attach 时定死就会说谎。
  const syncTitle = (): void => {
    btn.title = isOffMainCard(btn) ? TITLE_OFF_MAIN : TITLE_ON_MAIN;
  };
  btn.addEventListener("mouseenter", syncTitle);
  btn.addEventListener("focus", syncTitle);

  btn.addEventListener("click", (ev) => {
    ev.stopPropagation();
    if (btn.dataset.busy === "1") return;
    btn.dataset.busy = "1";
    void (async () => {
      try {
        const res = await commands.create_branch_session({
          sourceJsonlPath: opts.jsonlPath,
          messageUuid: opts.uuid,
        });
        btn.textContent = "✓";
        window.setTimeout(() => (btn.textContent = "⑂"), 2000);
        opts.onForked(res);
      } catch (err) {
        showActionFailureToast("创建分支失败", String(err));
      } finally {
        btn.dataset.busy = "0";
      }
    })();
  });

  cardEl.appendChild(btn);
}
