/**
 * F60：远端 tmux 画面预览（只读快照）。轻量 overlay（照 pf 范式，body-level fixed，
 * 点外关 + Esc + ✕，z-index 200）——invoke `capture_remote_pane` 抓 `tmux capture-pane -p`
 * 的屏幕文本，等宽 `<pre>` 展示；失败弹 toast。**非 attach、不接管终端；只读快照非实时**
 * （「重新抓取」按钮手动刷新，要动态看去 attach）。一次只开一个。
 */
import { showActionFailureToast } from "../error-toast";
import { commands } from "../ipc/commands";

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

/** 打开 [origin] 的 tmux 会话 `target` 的画面预览。 */
export async function openPanePreview(origin: string, target: string): Promise<void> {
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
  title.textContent = `预览画面 · [${origin}] tmux: ${target}`;
  head.appendChild(title);

  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "pane-preview-btn";
  refreshBtn.textContent = "重新抓取";
  head.appendChild(refreshBtn);

  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "pane-preview-btn";
  closeBtn.textContent = "✕";
  closeBtn.title = "关闭";
  closeBtn.addEventListener("click", closePanePreview);
  head.appendChild(closeBtn);

  const pre = document.createElement("pre");
  pre.className = "pane-preview-pre";
  pre.textContent = "抓取中…";

  box.appendChild(head);
  box.appendChild(pre);
  overlay.appendChild(box);
  document.body.appendChild(overlay);
  current = overlay;
  document.addEventListener("keydown", onKey);

  let loaded = false; // 已成功抓过一次（刷新失败时保留旧画面）
  const load = async (): Promise<void> => {
    refreshBtn.disabled = true;
    if (!loaded) pre.textContent = "抓取中…";
    try {
      const text = await commands.capture_remote_pane({ origin, target });
      if (current !== overlay) return; // 抓取途中被关/换
      pre.textContent = text.length > 0 ? text : "（画面为空）";
      loaded = true;
    } catch (e) {
      if (current !== overlay) return;
      showActionFailureToast("预览画面失败", String(e), { level: "info" });
      if (!loaded) closePanePreview(); // 首次失败无内容可留 → 关
    } finally {
      if (current === overlay) refreshBtn.disabled = false; // overlay 已关/换则别碰旧按钮
    }
  };
  refreshBtn.addEventListener("click", () => void load());
  await load();
}
