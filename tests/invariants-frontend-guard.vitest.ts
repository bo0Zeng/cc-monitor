/**
 * `P21` —— **`INVARIANTS.md` 里讲前端那五条（条 12 / 13 / 14 / 21 / 22）的判据。**
 *
 * 量具住 `tests/evidence/P21-frontend-invariants.ts`，**本文件只登记与判**
 * （分家的理由写在那边：`scanning-guard-registry.vitest.ts` 的 `WALKER_CEILING`
 * 不许测试文件再多一个遍历者）。
 *
 * # 🔴 先说这条判据**为什么**存在，以及它订正了 `真相源/105` 的哪一句
 *
 * `真相源/105 §1.4` 的读数是：「条 21 · 22 · 12 · 13 · 14 这五条讲的全是前端，
 * 而 TS 侧 2 129 条判据里**没有一条的断言对象是那几条条文**」。
 * 那个读数**是对的**，但它后面那句推论 ——「所以它们没人守」——**不成立**。
 * `P21` 第一步现打核过（命令与逐条读数写在件里），结论是**三种情况混在一起**：
 *
 * | 条 | 今天到底有没有人守 | 住址 |
 * |---|---|---|
 * | **12** alert 不算错误反馈 | 🔴 **没人守**（生产侧现打 0 处 `alert(`，但**没有任何东西断言那个 0**） | ⇒ 本文件 ① |
 * | **13** portal 必须真挂 body | 🔴 **没人守**（`info-icon.vitest.ts` 断的是 tooltip **零残留**，不是它挂在哪） | ⇒ 本文件 ② |
 * | **14** key 必须前缀 `cc-monitor.` | 🔴 **没人守**（`src/local-storage.ts` 头注引了条 14，全仓无断言读它） | ⇒ 本文件 ③ |
 * | **21.1/21.2** 守卫式 `snap` · 不手动补偿 | 🔴 **TS 侧没人守**（只有 `tests/e2e/f40-suite.sh` 的密度绊线，而它**手动、不进 CI**） | ⇒ 本文件 ④⑤ |
 * | **21.3** 尾部优先门控（F40a/F40b） | ✅ **已有人守** —— `tests/tabs.vitest.ts` 那一组 `F40a 门控矩阵` / `F40b R-1` 等值断言 | **不在这里复制**（`D1`） |
 * | **22.4** 精简模式 CSS 不塌 grid 行 | ✅ **已有人守** —— `tests/app-grid-claims.vitest.ts`，`viewer` 模式的隐式行数钉在 0 | **不在这里复制**（`D1`） |
 * | **22.5** 关窗要 `core:window:allow-close` | ✅ **已有人守** —— `tests/frontend/shell/capability_registry_tests.rs::every_webview_permission_is_registered` 那条 `stale` 断言（`ALLOWED` 里登记过的权限必须还在 `capabilities/default.json` 里）。⚠ 那份文件的头注**逐字说 `§22` 那处讲的是「要加权限，不是不许加」** —— 它当时判的是「本条服务哪条要求」，而它顺带兑现的正是 `§22` 第 5 项 | **不在这里复制**（`D1`；且 `.rs` 不在本轮写区，只能登记） |
 * | **22.3/22.6** listen 先注册再 emit · 独立窗自调 `dispatcher.start()` | 🔴 **没人守** | ⇒ 本文件 ⑥ |
 * | **22.1/22.2** 开窗 IPC 必须 `async` · 定向事件 target-kind 对齐 | ✅ **Rust 那半已有人守**〔S5 · 第四波〕—— `tests/frontend/shell/lib_window_lifecycle_tests.rs::every_window_building_command_is_async`（建窗点集合两向相等 ＋ 每处 `async fn` ＋ 紧挨 `#[tauri::command]`）· `…::every_emit_to_targets_a_webview_window_not_a_bare_label`（`emit_to` 调用点两向相等 ＋ 目标由 `EventTarget::webview_window(` 绑定）。TS 那半（viewer 传 `windowScoped: true`）〔MIG-1 收尾 · V41〕随那个选项删了：`bindEvents` 里已没有 Tauri 监听，定向投递只剩 `src/ipc/chan.ts` 那一处按窗口作用域听 | **不在这里复制**（`D1`） |
 *
 * ⇒ **`105` 那句「所以它们没人守」要改成「五条里有两条半今天真有人守，只是那些判据的
 * 散文里没点出它服务哪条条」。** 后半句才是 `105` 量到的东西（它量的是**指向**）。
 *
 * # 形态（每一格都是恒等或零命中，没有地板）
 *
 * | 格 | 判什么 | 形态 |
 * |---|---|---|
 * | ⓪ | 量具没坏（人群没缩水 · CSS 切规则那一刀在承重） | 地板 ＋ 恒等 |
 * | ① | 生产 TS 里 `alert(` 的调用处 | **零命中** |
 * | ② | 建 fixed 浮层的模块必须碰 `document.body`；不碰的 == 登记表，且宿主机检无包含块属性 | **恒等** |
 * | ③ | 每个存储 key 都带 `cc-monitor.` 前缀；绕过接入层的直写 key == 登记表 | **零命中 ＋ 恒等** |
 * | ④ | `MessageStream.snap()` 的守卫：已在底部 ⇒ 对 `scrollTop` **零次写入** | **恒等**（写入次数） |
 * | ⑤ | `.stream` 家族在 CSS 里不许出现 `overflow-anchor: none` | **零命中** |
 * | ⑥ | viewer 先 `await bindEvents` 再 replay；settings 自调 `applyOverrides` ＋ `start()` | **恒等**（顺序 ＋ 处数） |
 *
 * # 诚实边界
 *
 * - ④ 测的是 `snap()` 的**守卫条件**（`scrollHeight - clientHeight - scrollTop > 1`），
 *   **不是**「用户看不看得见抖动」。后者要真引擎逐帧读 `getBoundingClientRect().top`，
 *   那件事的家是 `tests/e2e/f40-suite.sh`（手动、不进 CI，`src/doc/DEVELOPMENT.md` 已登记）。
 *   ⇒ 本格买到的是「那个守卫还在、边界还是 1px」，买不到「真机不抖」。
 * - ② 是**必要条件**（一个从不碰 `document.body` 的模块不可能把浮层挂到 body 上），
 *   不是充分条件：同一份文件里既挂 body 又把另一个 fixed 元素挂在组件子树里，本格看不见。
 *   换来的是它对本仓实际的两族写法都判得对（理由逐字在量具 `FixedPortal` 头注里）。
 * - **不管条 12 的正面那半**（「关键失败必须有 toast / banner」）：那要判语义。
 *   ①只钉反面（不许用 alert）。正面那半今天由 `tests/error-toast.vitest.ts`（合流）与
 *   `tests/settings/unknown-keys-notice.vitest.ts`（恒亮的告警等于没有告警，报文逐字引 `INVARIANTS §12`）
 *   各守一角，**同样不在这里复制**。
 */
import { describe, it, expect } from "vitest";

import {
  alertCallSites,
  anchorToggles,
  containingBlockProps,
  cssDecl,
  cssRules,
  fixedPortals,
  fixedSelectors,
  opaqueKeySites,
  productionCode,
  storageKeys,
  topLevelFnBody,
} from "./evidence/P21-frontend-invariants.ts";

// jsdom 无 `ResizeObserver`（`MessageStream` 构造需要）——空壳即可（同 `record-timeline.vitest.ts`）。
globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

const { MessageStream } = await import("../src/stream.ts");

const SOURCES = productionCode();
const CSS_RULES = cssRules();
const PORTALS = fixedPortals(SOURCES);

// ═══════════════════════════ ⓪ 量具自检 ═══════════════════════════

describe("P21 ⓪ 量具自检（这几条不过，下面六格全是空转）", () => {
  it("生产 TS 人群没缩水（2026-09-22 实测 128 份）", () => {
    expect(
      SOURCES.length,
      `只扫到 ${SOURCES.length} 份生产 .ts（09-22 实测 128）—— 遍历坏了，①③⑥ 会零命中地绿`,
    ).toBeGreaterThan(100);
  });

  it("CSS 切规则那一刀在承重：规则数与 fixed 选择器数都不许塌（09-24 实测 24 个 fixed 选择器）", () => {
    expect(CSS_RULES.length, "一条 CSS 规则都没切出来 —— ②⑤ 此刻无效").toBeGreaterThan(500);
    // 🔴 这个 25 是**等号**，不是地板：本文件第一版用「相邻规则共用 `}`」的 matchAll 形，
    //    这里实测报 16（漏 9 条），而 16 看起来完全正常。等号让那种漏当场可见。
    //    真加了一个 fixed 浮层 ⇒ 这里与下面 ② 的登记表**同时**红，那正是要的摩擦。
    //    〔F7b 09-24〕25 → 24：少的是 `.sftp-overlay`（老 SFTP 面板的遮罩，随面板整段 CSS 退役；
    //    它挂在 `document.body` 上、不在 ② 的登记表里 —— 那张表今天一行都没因此变）。
    expect(
      fixedSelectors().length,
      `CSS 里声明 \`position: fixed\` 的选择器有 ${fixedSelectors().length} 个（09-24 实测 24）。\n` +
        "★ 变多 = 新加了一个浮层 ⇒ 去 ② 的登记表里交代它挂在哪；\n" +
        "★ 变少 = 要么真删了一个，要么**切规则那一刀又漏了**（第一版漏了 9 条，报 16）。",
    ).toBe(24);
  });

  it("🔴 正控：`alert(` 的量具对合成样本判得出「有」与「没有」两种", () => {
    const probe = (code: string) => alertCallSites([{ file: "合成.ts", code }]).length;
    expect(probe(`function f() { alert("失败了"); }`), "真调用没被逮到 —— ① 是死规则").toBe(1);
    expect(probe(`function f() { window.alert("失败了"); }`), "`window.alert` 没被逮到").toBe(1);
    expect(probe(`const x = dialog.alert(1); showAlert(2); const alerts = [];`), "误伤了非调用形").toBe(0);
  });

  it("🔴 正控：包含块属性的量具对本仓两个真选择器判得出相反结论", () => {
    // 条 13 的「为什么」逐字点的就是 `.settings-panel` 的 `transform: translateX(0)`。
    expect(
      containingBlockProps(".settings-panel"),
      "`.settings-panel` 的 `transform` 没被看见 —— ② 的登记表理由就是靠这把尺子机检的",
    ).toContain("transform");
    expect(
      containingBlockProps("#app"),
      "`#app` 被误判成带包含块属性 —— ② 会假红在两个合法的例外上",
    ).toEqual([]);
  });
});

// ═══════════════════════════ ① 条 12 ═══════════════════════════

describe("P21 ① 条 12：前端 alert 不算错误反馈 —— 生产侧零 `alert(`", () => {
  it("★ 生产 TS 里一处 `alert(` 都没有", () => {
    const sites = alertCallSites(SOURCES);
    expect(
      sites.map((s) => `${s.file}:${s.line}  ${s.text}`),
      "条 12：`alert` 弹窗用户可能没看清就关掉（v1.7.9 那次「Permission denied」事故的成因）。\n" +
        "关键失败要走 `console.error` ＋ 状态栏红色 toast（`src/error-toast.ts`）／严重的加顶部 banner。\n" +
        "⚠ 射程只有 `alert` —— `confirm`/`prompt` 问的是「要不要干」，不在条 12 里。",
    ).toEqual([]);
  });
});

// ═══════════════════════════ ② 条 13 ═══════════════════════════

/**
 * **不把 fixed 浮层挂 `document.body` 的已知例外**，逐条写清它凭什么安全。
 *
 * 🔴 「凭什么安全」不是散文 —— `host` 那一栏由下面那条判据**机检**：
 * 宿主在 CSS 里不许带任何会改包含块的属性（`transform`/`filter`/`perspective`/
 * `will-change`/`contain`/`content-visibility`/`backdrop-filter`）。
 * 只登记「它是例外」是不够的：那条断言在「尺子把所有宿主都判安全」时同样绿。
 */
const BODY_MOUNT_EXCEPTIONS: ReadonlyArray<{ sel: string; host: string; why: string }> = [
  {
    sel: ".fork-ask-backdrop",
    host: "#app",
    why:
      "`src/fork-ask.ts` 里是 `const host = opts.host ?? document.body` —— **默认就是 body**，" +
      "`opts.host` 只给测试注入宿主用。量具按「文件里有没有 `document.body.<挂载动词>(`」判，" +
      "这一形写成 `host.appendChild(backdrop)` ⇒ 判不出来。登记的宿主取 `#app`（最坏情况：" +
      "调用方传一个应用内容器），机检它无包含块属性。",
  },
  {
    sel: ".tasks-popover",
    host: "#app",
    why:
      "`src/tasks-panel.ts` 与 `src/agents-panel.ts` 的 popover 由 `src/main.ts` 挂进 `#app`" +
      "（`document.getElementById(\"app\")?.appendChild(…)` 两处），`src/agents-panel.ts` 的头注" +
      "逐字写着「挂到 `#app` 里当 fixed popover」。⇒ 这是**有意**的偏离条 13 字面，" +
      "安全性全靠 `#app` 身上没有包含块属性 —— 那一点由下面机检。",
  },
];

describe("P21 ② 条 13：CSS portal 元素必须真挂 body", () => {
  it("★ 建 fixed 浮层的模块必须碰 `document.body`；不碰的 == 登记表", () => {
    const off = PORTALS.filter((p) => !p.mountsBody).map((p) => p.sel);
    expect(
      off.sort(),
      "条 13：祖先有 `transform`/`filter`/`perspective`/`will-change`（或 layout containment）时，\n" +
        "`position: fixed` 后代的包含块从 viewport 重置到那个祖先 ⇒ fixed 失去 viewport 锚定。\n" +
        "本仓 `.settings-panel` 有 `transform: translateX(0)` 做 slide-in、`.stream-content > *` 有\n" +
        "`content-visibility: auto` ⇒ 挂在它们子树里的 fixed 元素会乱跑。挂 body 是唯一可靠路径。\n" +
        "★ 真有正当理由不挂 body ⇒ 写进 `BODY_MOUNT_EXCEPTIONS` 并说清宿主是谁（宿主会被机检）。" +
        `\n当前登记：${BODY_MOUNT_EXCEPTIONS.map((e) => e.sel).join(" · ")}`,
    ).toEqual(BODY_MOUNT_EXCEPTIONS.map((e) => e.sel).sort());
  });

  it("★ 登记表里每条的宿主，CSS 里必须真的不带任何包含块属性（这条才是「凭什么安全」）", () => {
    const bad = BODY_MOUNT_EXCEPTIONS.map((e) => [e.sel, e.host, containingBlockProps(e.host)] as const).filter(
      ([, , props]) => props.length > 0,
    );
    expect(
      bad.map(([sel, host, props]) => `${sel} 挂在 ${host}，而它带 ${props.join("/")}`),
      "登记的宿主身上出现了会改包含块的属性 ⇒ 挂在它里面的 fixed 浮层**此刻就会乱跑**。\n" +
        "要么把那个浮层改挂 `document.body`，要么把宿主那条属性去掉。",
    ).toEqual([]);
  });

  it("每个 fixed 选择器都说得出谁建的（说不出 ⇒ 上面两条在一个空集合上空转）", () => {
    expect(
      PORTALS.filter((p) => p.bornIn.length === 0).map((p) => p.sel),
      "CSS 里有 `position: fixed` 的选择器，TS 侧却找不到任何建元素处 ——\n" +
        "要么是死规则（删掉），要么是 `index.html` 里的静态 DOM（那要在这里登记），\n" +
        "要么量具的建元素匹配形不认这种写法（那要去量具里补一形）。",
    ).toEqual([]);
  });

  it("登记表不许有死条目（修好了就把登记删掉）", () => {
    const known = new Set(PORTALS.map((p) => p.sel));
    const nowOk = new Set(PORTALS.filter((p) => p.mountsBody).map((p) => p.sel));
    expect(
      BODY_MOUNT_EXCEPTIONS.filter((e) => !known.has(e.sel)).map((e) => e.sel),
      "登记表里这些选择器在 CSS 里已经不是 `position: fixed` 了 —— 删掉登记，\n" +
        "留着会让「已登记」看起来还覆盖着，实则那条已经不在了",
    ).toEqual([]);
    expect(
      BODY_MOUNT_EXCEPTIONS.filter((e) => nowOk.has(e.sel)).map((e) => e.sel),
      "登记表里这些已经改成挂 `document.body` 了 —— 把登记删掉（例外表只记今天真的例外）",
    ).toEqual([]);
  });
});

// ═══════════════════════════ ③ 条 14 ═══════════════════════════

/**
 * **绕过 `src/local-storage.ts` 接入层直写的存储 key**（`LS_KEYS` 里没有它们）。
 *
 * ⚠ 条 14 要的是**前缀**，这两个前缀都对。登记它们的理由是**另一半**：
 * `src/local-storage.ts` 头注逐字「新加 key 必须先在这里注册，否则模块内 grep 不到无法定位」，
 * 而条 14 的附带契约（Batch5-F19）要求「非主窗视图写共享 key 前必须显式隔离」——
 * 那个审视动作**只发生在接入层的 `LS_KEYS` 旁边**。绕过去的 key 没人会想起来审。
 * ⇒ 这一格只许降。
 */
// 〔CFG1 · 4D〕2 → 1：`cc-monitor.tab-bar-w` 进了 `LS_KEYS.tabBarWidth`（从 `main.ts` 直写收进 `tab-bar-width.ts`）。
const OFF_LEDGER_KEYS: readonly string[] = ["cc-monitor.boot-id"];

describe("P21 ③ 条 14：localStorage / IndexedDB key 必须前缀 `cc-monitor.`", () => {
  const KEYS = storageKeys(SOURCES);

  it("分母：真的抽到了 key（09-22 实测 19 处）", () => {
    expect(KEYS.length, `只抽到 ${KEYS.length} 个存储 key（09-22 实测 19）—— 抽取器坏了`).toBeGreaterThan(12);
    expect(KEYS.some((k) => k.registered), "`LS_KEYS` 那一族一个都没抽到 —— 接入层的块切错了").toBe(true);
  });

  it("★ 每一个 key 都以 `cc-monitor.` 开头", () => {
    expect(
      KEYS.filter((k) => !k.key.startsWith("cc-monitor.")).map((k) => `${k.file}:${k.line}  ${JSON.stringify(k.key)}`),
      "条 14：WebView2 的 origin 是 `tauri://localhost`，与其他可能用同一 origin 的 Tauri 应用\n" +
        "共享存储（理论上）。前缀避免冲突，也让数据透明化展示时过滤得出来。",
    ).toEqual([]);
  });

  it("★ 访问器实参解不出静态 key 的那几处 == 零（不许静默跳过）", () => {
    expect(
      opaqueKeySites(SOURCES).map((o) => `${o.file}:${o.line}  arg=${o.arg}`),
      "有存储访问器的 key 实参解不出静态前缀 ⇒ 上面那条断言**看不见它** ——\n" +
        "而「看不见」与「它合规」长得一模一样。要么把 key 收口进 `LS_KEYS`，\n" +
        "要么去量具 `resolveKeyArg` 里补这一形。",
    ).toEqual([]);
  });

  it("★ 绕过接入层直写的 key == 登记表（只许降）", () => {
    expect(
      [...new Set(KEYS.filter((k) => !k.registered).map((k) => k.key))].sort(),
      "新增了一个绕过 `src/local-storage.ts` 的直写 key。\n" +
        "★ 条 14 的附带契约（Batch5-F19：非主窗视图写共享 key 前必须显式隔离）那个审视动作\n" +
        "只发生在 `LS_KEYS` 旁边 —— 绕过去的 key 没人会想起来审。收口进 `LS_KEYS`，别加进这张表。",
    ).toEqual([...OFF_LEDGER_KEYS].sort());
  });
});

// ═══════════════════════════ ④ 条 21.1 ═══════════════════════════

/** 给一个元素装上「可编程的滚动几何」＋ 数 `scrollTop` 被写了几次。 */
function riggedScroller(scrollHeight: number, clientHeight: number, scrollTop: number) {
  const el = document.createElement("div");
  const writes: number[] = [];
  let top = scrollTop;
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => scrollHeight });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => clientHeight });
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => top,
    set: (v: number) => {
      writes.push(v);
      top = v;
    },
  });
  document.body.appendChild(el);
  return { el, writes };
}

describe("P21 ④ 条 21.1：`snap()` 必须是守卫式的 —— 已在底部就一次都不许写 `scrollTop`", () => {
  it("★ 已经贴在底部（差 0px）⇒ `scrollToBottom()` 对 `scrollTop` **零次写入**", () => {
    const { el, writes } = riggedScroller(1000, 400, 600); // 1000-400-600 = 0
    new MessageStream(el).scrollToBottom();
    expect(
      writes,
      "条 21.1：每帧无脑 `scrollTop = scrollHeight` 会在 HiDPI 分数像素布局下因整数\n" +
        "`scrollHeight` 与分数布局的舍入误差**每帧不同** ⇒ 整块内容 ±0.5px 高频重绘\n" +
        "（启动重放期「最新消息整行上下微抖」的根因，已实测定位）。",
    ).toEqual([]);
  });

  it("★ 差**恰好 1px** ⇒ 仍然零次写入（守卫的边界就是 `> 1`，不是 `> 0`）", () => {
    const { el, writes } = riggedScroller(1000, 400, 599); // 1000-400-599 = 1
    new MessageStream(el).scrollToBottom();
    expect(writes, "边界被放宽成 `> 0` ⇒ 亚像素舍入摆动会让每帧都重钉，抖动回归").toEqual([]);
  });

  it("★ 反向对照：真落后底部 42px ⇒ **恰一次**写入，且值 == `scrollHeight`", () => {
    const { el, writes } = riggedScroller(1000, 400, 558); // 1000-400-558 = 42
    new MessageStream(el).scrollToBottom();
    expect(writes, "落后底部却不贴 —— 那是把守卫写成了死规则，贴底功能整个没了").toEqual([1000]);
  });

  it("★ 条 21.2：内容插到「视口上方」（anchor ≠ null）而已在底部 ⇒ 零次写入（不手动补偿）", () => {
    const { el, writes } = riggedScroller(1000, 400, 600);
    const stream = new MessageStream(el);
    const first = document.createElement("div");
    stream.insertNode(first, null); // 先放一张，作后面的 anchor
    const above = document.createElement("div");
    stream.insertNode(above, first); // 插到它**上方**
    expect(
      writes,
      "条 21.2：视口上方插入交给浏览器原生 `overflow-anchor` 维持视觉稳定。\n" +
        "手动补偿 `scrollTop` ＋ 原生 anchoring 叠加会 double-shift。",
    ).toEqual([]);
    expect(above.nextElementSibling, "插入位置错了 —— 上面那个零写入是在一条没发生的插入上空转").toBe(first);
  });
});

// ═══════════════════════════ ⑤ 条 21.2（CSS 那半）═══════════════════════════

describe("P21 ⑤ 条 21.2：`.stream` 家族在 CSS 里不许出现 `overflow-anchor: none`", () => {
  it("★ 一处都没有", () => {
    const hits = CSS_RULES.filter((r) => /\.stream\b/.test(r.selector))
      .filter((r) => /(?:^|;)\s*overflow-anchor\s*:\s*none/.test(r.body))
      .map((r) => r.selector);
    expect(
      hits,
      "条 21.2 逐字：**禁止**给 `.stream` 设 `overflow-anchor: none` ——\n" +
        "补批在**同一同步任务内**的临时关闭（`tabs.fillAbove` / `session-viewer.maybeFillAbove`，\n" +
        "两处都在 `finally` 里还原）是唯一豁免类，而那是 JS 行内样式，**不许写进 CSS**：\n" +
        "写进 CSS 就是永久关闭，启动重放期的上方插入会立刻失去原生锚定。",
    ).toEqual([]);
  });

  it("★ JS 侧那个唯一豁免类：每处临时 `overflow-anchor:none` 都有还原，且还原在 `finally` 里", () => {
    const toggles = anchorToggles(SOURCES);
    // 分母：09-22 实测两处（`tabs.fillAbove` F40b · `session-viewer.maybeFillAbove` F39）。
    // 〔`设计/10` 骨架 · 子步 4〕+2，同一豁免类（同步任务内临时关、`finally` 还原）：
    // `tabs.attachSkeleton`（视口上方插一块高占位时按 ΔscrollHeight 补偿）·
    // `skeleton-view.materializeRanges`（物化可见区时钉住视口里那张已渲染卡）。
    expect(
      toggles.length,
      "找不到任何临时关闭处 —— 下面两条在空集合上绿。要么补批路都删了（那这一格一起删），要么量具坏了",
    ).toBe(6); // 〔U3b〕+1 同一豁免类：`skeleton-view.attachGaps`（查看器接骨架时钉住视口那张卡） // 〔RENDER2〕+1 同一豁免类：`skeleton-view.applyRefined`（第二级精算改占位高时钉住视口）
    expect(
      toggles.filter((t) => t.onLine === null).map((t) => `${t.file}:${t.offLine}`),
      "临时关掉了 `overflow-anchor` 却找不到还原处 ⇒ 那个 tab / viewer 会话**永久**失去原生锚定",
    ).toEqual([]);
    expect(
      toggles.filter((t) => !t.restoredInFinally).map((t) => `${t.file}:${t.onLine}`),
      "条 21.2 逐字：**还原必须在 `finally`** —— 渲染内核抛出时留下 `overflow-anchor:none`\n" +
        "= 该 tab 永久失去原生锚定，**且无自愈**（D 审计两家共识）。\n" +
        "⚠ 本条是近似判法（往前回看 600 字符找 `finally`），嵌套 try 会让它假红 —— 那是叫，不是漏。",
    ).toEqual([]);
  });

  it("对照组：`.stream` 真的被扫到了（否则上面那条是在空集合上绿）", () => {
    expect(
      CSS_RULES.filter((r) => /\.stream\b/.test(r.selector)).length,
      "`.stream` 家族一条规则都没扫到 —— 上面那条零命中此刻无效",
    ).toBeGreaterThan(3);
    // 常驻自检：那条「保留原生 overflow-anchor（默认 auto）」是**靠不写**实现的 ⇒
    // `.stream` 身上压根不该有这个属性。写了任何值都要有人看一眼。
    expect(
      cssDecl(".stream", "overflow-anchor"),
      "`.stream` 上出现了 `overflow-anchor` 声明。条 21.2 要的是**保留浏览器默认 auto**，\n" +
        "也就是**一个字都不写**。写了 `auto` 也要有人看一眼（说明有人动过这一块）。",
    ).toBeNull();
  });
});

// ═══════════════════════════ ⑥ 条 22.2 / 22.3 / 22.6 ═══════════════════════════

describe("P21 ⑥ 条 22：独立窗口契约里 TS 这一侧的三项", () => {
  // 〔三入口拆分 · 住址搬家〕两个精简 bootstrap 从 `main.ts` 搬到了各自的入口模块
  // （`viewer.html → entry-viewer.ts`、`settings.html → entry-settings.ts`，`设计/01 §1.2`）。
  // 切函数体的尺子与下面四条断言一字未改，只是换了文件读。
  const codeOf = (file: string): string => SOURCES.find((s) => s.file === file)?.code ?? "";
  const VIEWER = topLevelFnBody(codeOf("src/entry-viewer.ts"), "bootstrapViewer");
  const SETTINGS = topLevelFnBody(codeOf("src/entry-settings.ts"), "bootstrapSettings");

  it("分母：两个 bootstrap 的函数体都切出来了", () => {
    expect(VIEWER, "`bootstrapViewer` 的函数体切不出来 —— 下面三条全是空转").not.toBeNull();
    expect(SETTINGS, "`bootstrapSettings` 的函数体切不出来 —— 下面那条是空转").not.toBeNull();
    expect((VIEWER ?? "").length, "`bootstrapViewer` 切得太短，八成停在了第一个换行缩进的 `}`").toBeGreaterThan(2000);
    expect((SETTINGS ?? "").length, "`bootstrapSettings` 切得太短").toBeGreaterThan(300);
  });

  // 〔CF2 · 第四波 4B〕viewer 不再单独调定向重放命令（`replay_session_to_window` 退役）：会话流经 `bindEvents` 的
  //   `streams` 选项订（`session-lines/<sid>`，留存由那条订阅当场交）。「先注册再触发」这一条的两半因此换了住址：
  //   ① viewer 这一侧：订阅**只**经 `await bindEvents(…, { streams })`；② `events.ts` 那一侧：所有 listen 注册完
  //   （`await Promise.all(registrations)`）之后才 `chan.subscribe(`（订阅一登记，句柄就可能开始交格）。
  it("★ 22.3：viewer 的会话流**只**经 `await bindEvents(…, { streams })` 订；`bindEvents` 里先注册完 listen 再订", () => {
    const body = VIEWER ?? "";
    const bind = body.indexOf("await bindEvents(");
    expect(bind, "`bootstrapViewer` 里找不到 `await bindEvents(` —— 要么改名了，要么 `await` 被摘了").toBeGreaterThan(-1);
    expect(/streams:\s*\[/.test(body.slice(bind)), "`bootstrapViewer` 的 `bindEvents` 没带 `streams` —— 独立窗口收不到会话内容").toBe(true);
    expect(body.includes("replay_session_to_window"), "退役的定向重放命令又回来了").toBe(false);
    // 〔合并 MIG-1 × 主线 eebf51de〕`events.ts` 那一半换判法：`bindEvents` 里最后几条 Tauri 监听两边各自退役（MIG-1 会话起停并进会话流 · MIG-3b `task-update`），
    //   「先注册完 listen 再订」没有可排的序了 ⇒ 判「那里一条异步注册的 `listen` 都没有」—— 谁长回来一条，这里先红，逼人把「等注册完」那一格补回来。
    const ev = codeOf("src/events.ts");
    expect(ev.indexOf("chan.subscribe("), "`events.ts` 里找不到 `chan.subscribe(`").toBeGreaterThan(-1);
    expect(
      /\blisten\s*(<[^>]*>)?\s*\(|@tauri-apps\/api\/event/.test(ev),
      "条 22.3：`listen()` 是**异步注册**，注册完成前 emit 的事件会**静默丢**（实测症状：viewer 白屏只剩状态栏）。\n" +
        "⇒ `bindEvents` 里若再有 Tauri 监听，必须 `await` 注册完才订会话流（订阅一登记，句柄就可能开始交格）。",
    ).toBe(false);
  });

  // 〔MIG-1 收尾 · V41〕「22.2（TS 那半）：viewer 的 `bindEvents` 必须带 `windowScoped: true`」那一条随那个选项删了（主会话裁）。

  it("★ 22.6：settings 窗必须自调 `dispatcher.applyOverrides` ＋ `dispatcher.start()`，各恰一处", () => {
    const body = SETTINGS ?? "";
    expect(
      (body.match(/dispatcher\.applyOverrides\(/g) ?? []).length,
      "条 22.6：设置窗有**自己的** dispatcher 实例，不 `applyOverrides` ⇒ 窗内看到的是默认键位",
    ).toBe(1);
    expect(
      (body.match(/dispatcher\.start\(\)/g) ?? []).length,
      "条 22.6：`dispatcher.start()` 是唯一挂 window keydown 的地方 —— 不调则\n" +
        "① 快捷键编辑器录制收不到键；② Esc 无法经 overlay LIFO 逐层关。\n" +
        "⚠ **别改成手搓 window 级 Esc 监听**：它会与栈内 overlay 的 Esc 双触发（既关 overlay 又关整窗）。",
    ).toBe(1);
  });
});
