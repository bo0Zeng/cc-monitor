/**
 * 🔴 〔`设计/99 §2.5 P12` 2026-09-21〕**「配置里有 app 不认识的键」的那条常驻提示。**
 *
 * # 它治的是什么
 *
 * `P12` 逐字：「真正的毛病不是改名，是**未知键静默忽略** —— 那是本仓的头号病形
 *（『关掉了』与『过了』在终端上一模一样）。」
 *
 * config.json 上这个病长这样：用户手写一个键（打错字 / 抄了过期文档 / 用的是已退役的旧名字）
 * → 各模块只按自己认得的键名取值 → 认不出的那个**一个字都不说**
 * ⇒ 用户看到的是「我明明写了，它没生效」，而这一侧连它存在都不知道。
 *
 * ⇒ 这条提示把它说出来，**指名道姓**（只说「有未知键」而不说是哪个，用户还是得自己去翻文件）。
 *
 * # 为什么是**常驻条**，而不是 toast，也不是 ⓘ
 *
 * `INVARIANTS §12` 立着的规矩：状态性 / 安全性警告不得只活在 hover / 点击之后
 *（「用户看到了但没注意到关键信息」在本仓**已真实发生过一次**）。
 * 「配置里有个键没生效」是个**会一直为真到用户去改那个文件为止**的状态，不是一次性事件
 * ⇒ 按 `restart-notice.ts` 那条同型判例办：**常驻、没有「知道了」按钮**。
 * 一个自动消失 5 秒的 toast 恰恰是这条规矩点名不许的形状。
 *
 * ⚠ `config.ts` 那句 `console.warn` **不算出声** —— 日志里的话用户看不见，
 *   而看不见的告知与没告知在终端上一模一样。出声的是本文件这一条。
 *
 * # 射程：它盖不到什么
 *
 * - **只有设置面板这一面**。主窗口启动时那条（`src/frontend/ui/main.ts`）不在 `P12` 的写区，
 *   今天没有 ⇒ 从没打开过设置的用户看不到它。
 * - **只认顶层键**。`remote.hosts[].typo` 这种子对象里的错字本条看不见
 *   （子对象的 schema 归各自的主人，`config.ts` 那张表刻意不认）。
 * - **不判「值对不对」**。键名认得出但值是垃圾（`showBgSessions: "yes"`）那是各读者
 *   自己的宽容读法在管，本条一个字都不说。
 */
import { loadConfig, unknownConfigKeys, unknownKeysIn } from "../config";
import { copyText } from "../copy-table";

/**
 * 提示条上那句话。**纯函数**，好让判据直接打在这里（而不是只能隔着 DOM 猜）。
 *
 * 空列表 ⇒ `null`（整条不该出现）。
 *
 * ⚠ 文案受 `tests/frontend/ui/settings/ui-copy-discipline.vitest.ts` 那五种形状管：
 *   不许 markdown 标记 / 源码住址 / 内部标识符 / 日志行格式 / 设计论证。
 *   `config.json` 是**用户自己机器上的文件**，那把尺子刻意不把 `.json` 当源码住址。
 */
export function unknownKeysMessage(keys: readonly string[]): string | null {
  if (keys.length === 0) return null;
  return (
    copyText("unknownKeysNotice.unknownKeysMessage.message", { keysCount: keys.length, keys: keys.join(copyText("unknownKeysNotice.unknownKeysMessage.listSep")) })
  );
}

/**
 * 常驻条。**刻意没有关闭按钮** —— 同 `restart-notice.ts`：用户点一下并不会让
 * 「这个键不生效」这件事变成假的。
 *
 * 读两次：① 建条时用 `config.ts` 上一次读盘留下的那格快照（app 启动早就读过了，
 * 立刻就有话说）；② 同时再 `loadConfig()` 一次刷新（用户可能刚在外面改过文件，
 * 而「开设置」正是改完之后的标准动作）。两条路更新的是同一句话。
 */
export function createUnknownKeysBar(): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "settings-unknown-keys-bar";
  bar.setAttribute("role", "status");
  const render = (keys: readonly string[]): void => {
    const msg = unknownKeysMessage(keys);
    if (msg === null) {
      bar.hidden = true;
      bar.textContent = "";
      return;
    }
    bar.hidden = false;
    bar.textContent = msg;
  };
  render(unknownConfigKeys());
  // 刷新失败**不清空**已经显示的那句：读不到盘不等于那些键消失了。
  void loadConfig().then(
    (cfg) => render(unknownKeysIn(cfg)),
    () => {},
  );
  return bar;
}
