/**
 * 远端 tmux 画面预览（只读快照）：轻量浮层（body 级 fixed，点外关 ＋ Esc ＋ ✕），经 `terminal-reads.ts::previewShot`
 * 问那台后端抓挂着这个会话的那个终端那一屏（`terminal-preview`，带颜色段），按 `terminal-screen.ts` 的画法展示；失败弹 toast。
 * 不 attach、不接管终端、不实时（「重新抓取」手动刷新）。一次只开一个。对外文案全在文案表 `panePreview.*`。
 */
import { copyText } from "../copy-table";
import { toast } from "../kit/toast";
import { saidOfControl } from "../control-said";
import { previewShot, type TerminalTarget } from "../terminal-reads";
import { renderScreen, screenPre } from "../terminal-screen";
import { detailOf } from "../kit/detail";

let current: HTMLElement | null = null;

function onKey(e: KeyboardEvent): void {
  if (e.key === "Escape") closePanePreview();
}

/** 关掉当前预览 overlay（无则 no-op）。 */
export function closePanePreview(): void {
  if (current) {
    current.remove();
    current = null;
    document.removeEventListener("keydown", onKey);
  }
}

/** 打开 [origin] 上 `which` 那个终端的画面预览（`target` 是标题里那个 tmux 会话名）。 */
export async function openPanePreview(origin: string, target: string, which: TerminalTarget): Promise<void> {
  closePanePreview(); // 一次只一个

  const overlay = document.createElement("div");
  overlay.className = "pane-preview-overlay";
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) closePanePreview();
  });

  const box = document.createElement("div");
  box.className = "pane-preview-box";

  const head = document.createElement("div");
  head.className = "pane-preview-head";
  const title = document.createElement("span");
  title.className = "pane-preview-title";
  title.textContent = copyText("panePreview.head.title", { origin, target });
  head.appendChild(title);

  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "pane-preview-btn";
  refreshBtn.textContent = copyText("panePreview.head.refresh");
  head.appendChild(refreshBtn);

  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "pane-preview-btn";
  closeBtn.textContent = copyText("panePreview.head.closeIcon");
  closeBtn.title = copyText("panePreview.head.close");
  closeBtn.addEventListener("click", closePanePreview);
  head.appendChild(closeBtn);

  const scroller = document.createElement("div");
  scroller.className = "pane-preview-pre";
  const pre = screenPre();
  scroller.appendChild(pre);
  pre.textContent = copyText("panePreview.body.loading");

  box.appendChild(head);
  box.appendChild(scroller);
  overlay.appendChild(box);
  document.body.appendChild(overlay);
  current = overlay;
  document.addEventListener("keydown", onKey);

  let loaded = false; // 已成功抓过一次（刷新失败时保留旧画面）
  const load = async (): Promise<void> => {
    refreshBtn.disabled = true;
    if (!loaded) pre.textContent = copyText("panePreview.body.loading");
    try {
      const shot = await previewShot(origin, which, target);
      if (current !== overlay) return; // 抓取途中被关/换
      if (shot.text.length > 0) renderScreen(pre, shot.lines);
      else pre.textContent = copyText("panePreview.body.empty");
      loaded = true;
    } catch (e) {
      if (current !== overlay) return;
      // 那台写好的那一句不带对象（它不知道这里怎么称呼这个窗格）⇒ 句子作标题、对象放灰字。
      toast(saidOfControl(e), target, { detail: detailOf(e), level: "info" });
      if (!loaded) closePanePreview(); // 首次失败无内容可留 → 关
    } finally {
      if (current === overlay) refreshBtn.disabled = false; // overlay 已关/换则别碰旧按钮
    }
  };
  refreshBtn.addEventListener("click", () => void load());
  await load();
}
