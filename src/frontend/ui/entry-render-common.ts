/**
 * 主窗口与 viewer 窗共用、**设置窗没有**的那一小撮：外链 ＋ 代码块「复制」的全局 click 代理。
 *
 * 为什么从 `entry-common.ts` 再拆出来：代码块复制按钮只存在于渲染栈画出来的卡片里，
 * 设置窗没有渲染栈。留在三窗共用的那份里，设置窗的模块图就会带着 `.code-copy` /
 * `.code-block` 这两个选择器 —— `tests/frontend/ui/entry-graphs.vitest.ts` 的「每窗 CSS 完整性」会据此
 * 要求设置窗也加载代码块样式，与「设置窗里没有渲染栈」自相矛盾。拆开之后两条同时成立。
 */
import { installExternalLinkDelegation } from "./entry-common";
import { copyText } from "./copy-table";
import { writeClipboard } from "./clipboard";

/**
 * 外链 + 代码块复制的全局 click 代理。主窗口与独立 viewer 窗口共用。
 * - 外链（render.ts 标 data-external）：见 `installExternalLinkDelegation`。
 * - 代码块"复制"按钮：marked 输出的 HTML 无法挂 listener，这里 delegate。
 */
export function installGlobalClickDelegation(): void {
  installExternalLinkDelegation();

  document.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement | null)?.closest?.(".code-copy");
    if (!btn) return;
    const block = btn.closest(".code-block");
    const pre = block?.querySelector("pre");
    const text = pre?.textContent ?? "";
    if (!text) return;
    void writeClipboard(text).then(
      () => {
        btn.classList.add("copied");
        btn.textContent = copyText("render.codeBlock.copied");
        // 调度：一次性 —— 1.2s 后把「已复制」还原成「复制」
        window.setTimeout(() => {
          btn.classList.remove("copied");
          btn.textContent = copyText("render.codeBlock.copy");
        }, 1200);
      },
      () => {
        // 写不进：把那段代码选中（就地全选，按一下复制键就有），按钮说「复制失败」。
        if (pre) window.getSelection()?.selectAllChildren(pre);
        btn.textContent = copyText("render.codeBlock.failed");
        // 调度：一次性 —— 1.2s 后把「失败」还原成「复制」
        window.setTimeout(() => (btn.textContent = copyText("render.codeBlock.copy")), 1200);
      },
    );
  });
}
