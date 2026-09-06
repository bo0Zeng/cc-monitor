/**
 * N-F3：**第一次打开说得出下一步** —— 主窗口那一条「还差什么」指路。
 *
 * # 病在哪（现打于 `2afe176`）
 *
 * 「还差什么」这张清单**算得出来**：`settings/readiness.ts` 的 `computeGaps` /
 * `summarizeGaps` / `describeGap` 三个纯函数早就齐了，`N-F2` 之后本机那两格也进去了。
 * 而它的**生产消费者只有一个** —— `settings/remote-section.ts`
 * ⇒ 它只在「设置面板 → 远端那一节」渲染。
 * **一个刚装完、还没打开过设置的人，一个字都看不到。**
 *
 * ⚠ 「只有一个」是在报一个数 ⇒ **同句给分母**：分母 = `src/**\/*.ts` 去掉 `*.vitest.ts`
 * 与 `readiness.ts` 自己（现打 220 个文件），搜那三个名字得 6 处命中、落在 2 个文件上；
 * 其中 `accounts-section.ts` 那一处是**注释**不是调用 ⇒ 真消费者 1 个。
 * 那 6 处在 `2afe176` 上的住址与逐字（行号钉在这个 sha 上，别裸引）：
 *   `remote-section.ts:54`  `import { computeGaps, summarizeGaps, describeGap } from "./readiness";`
 *   `remote-section.ts:311` （头注里提了一句 `computeGaps` 是纯函数）
 *   `remote-section.ts:320` `const gaps = computeGaps({`
 *   `remote-section.ts:327` `const summary = summarizeGaps(gaps);`
 *   `remote-section.ts:347` `li.textContent = describeGap(g);`
 *   `accounts-section.ts:301` （注释：`summarizeGaps` 恒非 null）
 *
 * 本模块只补这一跳：主窗口状态栏上一条**非模态**的指路。
 *
 * # 划死了不做什么（件文件 `§2`，别在这里越界）
 *
 * 1. **不做多步安装向导**（定框 `N3` 逐字排除：「那是在没有数据源的情况下做壳」）。
 * 2. **不把整张清单搬过来** —— 清单今天的住址（设置面板那一节）是对的，这里只加**一条指路**。
 * 3. **不另写一套措辞** —— 数与文案一律来自 `summarizeGaps`，主窗口不给同一份文案开第二个家。
 * 4. **不碰 `readiness.ts` 的算法**（那三条 `notApplicable` 是 Phase G 逮到真 bug 之后钉的）。
 *
 * # 🔴 两个维度 = 两个值，不许压成一个
 *
 * 盘上已经有一个「首次运行」的形状：`main.ts` 的 `cmdkHintSeen`
 * （`LS_KEYS` ＋ `safeGet/safeSet`，头注逐字「见过即不再」）。
 * **那个形状对这里是错的，刻意没有照抄**：
 *
 * - 「教一个快捷键」是**一次性知识**：见过就学会了，永远不再说是对的。
 * - 「你还差三项没配」是**状态**：见过一次就永远不说，等于把一条状态提示做成一次性广告；
 *   而它说的那件事在盘上仍然是真的。
 *
 * ⇒ 这里是**两个独立的值**，`refresh()` 里合成，谁也不写谁：
 *
 * | 维 | 值 | 住哪 | 谁改它 |
 * |---|---|---|---|
 * | **状态维**「还差什么」 | `gapSummary()` 的返回 | **不住任何地方**——每次 `refresh()` 现算 | 账本（用户真去补齐了） |
 * | **意愿维**「别再烦我」 | `dismissedThisRun` | **只在内存里**，一个进程的生命周期 | 用户点那个 × |
 *
 * 两条推论，都是判据钉着的：
 * - 补齐了 ⇒ 指路消失；又缺了 ⇒ **再出现**（意愿维管不着状态维）。
 * - 关掉这一次 ⇒ 这一次不再打扰；**下次启动照说**（意愿维**不落盘**，
 *   这正是与 `cmdkHintSeen` 分野的那一刀 —— 落了盘它就退化成「见过即不再」）。
 *
 * # 为什么值不缓存
 *
 * `gapSummary()` 每次现算而不缓存：一缓存，「补齐了就消失」就变成「补齐了但没人通知它」。
 * `computeGaps` 是纯函数、只读 S3 那本账（不发任何请求），算一次的代价就是几十次对象查表。
 */

import { computeGaps, summarizeGaps } from "./settings/readiness";
import type { MachineStatus } from "./settings/machine-status";
import type { HostOs } from "./settings/host-os";

/** 那条指路的容器 class。判据与样式共用这一个名字，别在第二处写字面量。 */
export const FIRST_RUN_HINT_CLASS = "status-first-run";

/**
 * 依赖**注入而不是直接 import** —— 与 `readiness.ts` 同一个理由（它的 `statusOf` /
 * `isDaemonless` / `hostOs` 当初就是为这个注入的）：注入了才测得动「补齐 ⇒ 消失」。
 */
export interface FirstRunHintDeps {
  /** 要算哪几台机器（本机用 `LOCAL_MACHINE_KEY`）。顺序即 `computeGaps` 的呈现顺序。 */
  origins: () => string[];
  /** 读 S3 那本账。 */
  statusOf: (origin: string) => MachineStatus;
  /** daemonless 是用户显式选的降级，不是缺件。 */
  isDaemonless?: (origin: string) => boolean;
  /** S9：本机 OS 决定哪些组件适用（Windows 本机没有 `ccm`）。 */
  hostOs: () => HostOs;
  /** 点它 ⇒ 打开那张清单住的地方。 */
  openList: () => void;
}

/**
 * 主窗口状态栏上的那条指路。
 *
 * 生命周期：`refresh()` 决定挂不挂。**没有可说的就一个节点都不建**
 * （不是建一个空的再藏起来）—— 照 `remote-section.ts:330` 那条既有设计逐字：
 * 「全绿就整块不出现 —— 老用户不该天天看见一个空清单」。
 */
export class FirstRunHint {
  private el: HTMLElement | null = null;

  /**
   * 🔴 **意愿维**：这一次不再打扰。
   *
   * **只活在内存里，故意不落 `localStorage`。** 落盘的那一版就是 `cmdkHintSeen`，
   * 而那个语义（见过即不再）对一条状态提示是错的。下次启动重新算、重新说 ——
   * 直到用户真把那几项补齐，状态维自己让它闭嘴。
   */
  private dismissedThisRun = false;

  constructor(
    private readonly host: HTMLElement,
    private readonly deps: FirstRunHintDeps,
  ) {}

  /**
   * 🔴 **状态维**：还差什么。`null` = 一项都不缺。
   *
   * **现算，不缓存**，且**复用 `summarizeGaps`** —— 主窗口不自己数、不自己造词。
   */
  gapSummary(): string | null {
    return summarizeGaps(
      computeGaps({
        origins: this.deps.origins(),
        statusOf: this.deps.statusOf,
        isDaemonless: this.deps.isDaemonless,
        hostOs: this.deps.hostOs(),
      }),
    );
  }

  /** 🔴 **意愿维**的读法。与 `gapSummary()` 是两个值，谁也不派生自谁。 */
  isDismissedThisRun(): boolean {
    return this.dismissedThisRun;
  }

  /**
   * 按**两个值**决定挂不挂。合成规则只有这一处：
   * 有话说（状态维）**并且**没被关掉（意愿维）⇒ 挂；否则整块摘掉。
   */
  refresh(): void {
    const summary = this.gapSummary(); // 值① 状态维
    const dismissed = this.dismissedThisRun; // 值② 意愿维
    if (summary === null || dismissed) {
      this.unmount();
      return;
    }
    this.mount(summary);
  }

  /** 用户主动关掉这一次。**不写盘**，见 `dismissedThisRun` 的头注。 */
  dismissThisRun(): void {
    this.dismissedThisRun = true;
    this.refresh();
  }

  /** 整块摘掉 —— **移除节点**，不是渲染成空、也不是 `display:none`。 */
  private unmount(): void {
    this.el?.remove();
    this.el = null;
  }

  private mount(summary: string): void {
    if (!this.el) {
      const box = document.createElement("span");
      box.className = FIRST_RUN_HINT_CLASS;

      const open = document.createElement("button");
      open.type = "button";
      open.className = `${FIRST_RUN_HINT_CLASS}-open`;
      open.title =
        "打开设置 → 远端：那里逐条列着还差什么、缺了有什么后果、点哪里补齐。";
      open.addEventListener("click", () => this.deps.openList());
      box.appendChild(open);

      const close = document.createElement("button");
      close.type = "button";
      close.className = `${FIRST_RUN_HINT_CLASS}-dismiss`;
      close.textContent = "×";
      // 措辞刻意写「这次」：它下次还会来，直到那几项真被补齐。
      close.title = "这次先不看（下次启动还会提示，直到补齐）";
      close.setAttribute("aria-label", "关掉这次的提示");
      close.addEventListener("click", () => this.dismissThisRun());
      box.appendChild(close);

      this.host.appendChild(box);
      this.el = box;
    }
    const open = this.el.querySelector(`.${FIRST_RUN_HINT_CLASS}-open`);
    // 文案与数**全部**来自 `summarizeGaps`；这里只加一个前缀词，与
    // `remote-section.ts` 那个标题逐字同形（同一句话不许有两种说法）。
    if (open) open.textContent = `还差什么：${summary}`;
  }
}
