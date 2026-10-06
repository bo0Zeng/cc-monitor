/**
 * 竖直 tab 栏的**宽度**：右缘拖拽把手 ＋ 记忆（Batch11-F33）。
 *
 * 从 `main.ts` 整段搬来（D §D5：拖宽把手的 DOM、拖拽逻辑和宽度读写都住在 `main.ts`，
 * 直调 `localStorage.getItem/setItem`、键不在 `LS_KEYS` 里 —— 绕过了存储接入层）。
 * 今天：键是 `LS_KEYS.tabBarWidth`，读写走 `safeGet` / `safeSet`（私密模式 / 配额满不抛）。
 *
 * 住址仍是 localStorage：宽度算「UI 偏好」还是「用户手写的真相」（那条要住 `config.json`），
 * 设计没裁（D §D5 原话「住址请 30 裁」）—— 本模块不替它裁，只把它收进 tab 栏自己这一族、进登记。
 * 画面契约：「显式宽度 = `--tab-bar-w`」。
 */
import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { copyText } from "./copy-table";

/** 宽度夹在这两个值之间（px）。 */
const MIN_W = 200;
const MAX_W = 340;

const clampW = (w: number): number => Math.min(MAX_W, Math.max(MIN_W, w));

/**
 * 把手挂到 `#app` 上，并把记住的宽度应用上去。只在主窗调一次；没有 `#app` ⇒ 什么都不做。
 *
 * ⚠ `#app` 在这里现取（`const appEl = document.getElementById("app")`），不收参数：
 * `tests/frontend/ui/app-grid-claims.vitest.ts` 的尺 A 按这个形状从源码认「往 `#app` 插节点」的调用点，
 * 收参数的写法它认不出 ⇒ 这个插入点会掉出人群（「看不见」与「合规」长得一样）。
 */
export function mountTabBarResizer(): void {
  const appEl = document.getElementById("app");
  if (!appEl) return;
  const saved = Number(safeGet(LS_KEYS.tabBarWidth));
  if (Number.isFinite(saved) && saved > 0) {
    appEl.style.setProperty("--tab-bar-w", `${clampW(saved)}px`);
  }
  const resizer = document.createElement("div");
  resizer.id = "tab-bar-resizer";
  resizer.title = copyText("main.tabBar.resizeHint");
  // 拖动期间**不能**实时改 --tab-bar-w：网格列宽一变，消息区整棵布局树重排，
  // 而切 tab 零卡顿方案让所有 tab 的 DOM 都 visibility 保活在布局树里——每次
  // mousemove 全量重排 = 拖动巨卡。改为拖动时只画 fixed 参考线（repaint-only），
  // 松手一次性提交宽度。
  resizer.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const barLeft = document.getElementById("tab-bar")?.getBoundingClientRect().left ?? 0;
    const guide = document.createElement("div");
    guide.className = "tab-bar-resize-guide";
    const applyGuide = (clientX: number): number => {
      const w = clampW(clientX - barLeft);
      guide.style.left = `${barLeft + w}px`;
      return w;
    };
    let lastW = applyGuide(e.clientX);
    document.body.appendChild(guide);
    resizer.classList.add("resizing");
    const finish = (commit: boolean): void => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      window.removeEventListener("blur", onBlur);
      guide.remove();
      resizer.classList.remove("resizing");
      if (commit) {
        appEl.style.setProperty("--tab-bar-w", `${lastW}px`);
        safeSet(LS_KEYS.tabBarWidth, String(lastW));
      }
    };
    const onMove = (ev: MouseEvent): void => {
      // 主键已松开（窗外释放 / 切走时 mouseup 丢失）→ 取消收尾。否则 document
      // 级 mousemove 监听永久泄漏，之后选中文字都在触发它（同 tab 撕离的容错）。
      if ((ev.buttons & 1) === 0) {
        finish(false);
        return;
      }
      lastW = applyGuide(ev.clientX);
    };
    const onUp = (): void => finish(true);
    // alt-tab / 点别的窗口切走 → 落点不可信，直接取消（回来不会带着幽灵拖拽）。
    const onBlur = (): void => finish(false);
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    window.addEventListener("blur", onBlur);
  });
  appEl.appendChild(resizer);
  // 窄窗折叠（内容列 780px + 栏 + 呼吸空间放不下 → 图标条 44px）现在**整条在 CSS 里**：
  // `styles.css` 的 `@media (width < 980px)`（S24）。
  // 这里原本是一个 `resize` 监听往 body 上挂 `.tabbar-collapsed`，而那个类
  // **只被写、从没被读**（唯一读者就是那几条 CSS 规则）⇒ 纯视觉断点绕一圈 JS，
  // 白付一次「窄窗启动先闪一下宽栏」。删掉监听不留等价物，别再加回来。
}
