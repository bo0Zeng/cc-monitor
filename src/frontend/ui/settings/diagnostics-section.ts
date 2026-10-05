/**
 * 设置面板「日志」区（v2.0.0 落地 issue #4； 09-19 改名）。
 *
 * 给用户：
 * - 看到日志文件路径 + 当前大小
 * - 切日志级别（trace/debug/info/warn/error/off），立即生效不用重启
 * - 切错误 toast 开关
 * - 切是否写日志文件（需要重启）
 * - 一键打开日志文件 / 日志目录
 *
 * 之所以独立 section 不混进字体/颜色：它是「出问题才用」的工具，跟外观无关。
 * 参照 cc_integration.ts 的 section 范式。
 */

import { commands } from "../ipc/commands";
import { makeInfoIcon } from "./info-icon";
import { holdSkeletonHeight } from "./skeleton";
import { withPending } from "./pending";
import { toast } from "../kit/toast";
import { formatBytes } from "../format";
import { markRestartNeeded } from "./restart-notice"; // S7：待生效改动的唯一去处
import { openPath } from "@tauri-apps/plugin-opener";

// C04d 批 4：四个类型换成生成物（源 `logging.rs`）。手写版与生成物**逐字等价**
// ——这一批零漂移，价值是防将来漂。
//
// `RestartHint` 是**只有 unit variant 的外部标记枚举**（`rename_all = "snake_case"`，无 `tag`）
// ⇒ 线上就是字符串，生成物给的正是 `"none" | "needs_restart"`，与手写版完全相同。
//
// 三个大整数字段按 C03 策略配了 `ts(type = "number")`，且**两个量纲分开论证**：
// `current_size_bytes`/`size_bytes` 是**字节数**（2^53-1 B ≈ 8 PB）·
// `modified_ms` 是**毫秒时间戳**（≈ 28.5 万年）。混成一条是 C03 明确禁止的。
// **只 import 还真被本文件引用的那个**：`LogFileInfo`/`LogFileEntry`/`RestartHint`
// 现在由包装层的签名提供，调用点不再需要本地标注 —— 这正是包装层该有的效果
// （它们仍被 `ipc/commands.ts` 与生成物之间的 import 链消费，不是死文件）。
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import { copyText } from "../copy-table";

const LOG_LEVELS = ["trace", "debug", "info", "warn", "error", "off"] as const;

export interface DiagnosticsSectionOptions {
  /**
   * v2.x (issue #7)：被 CollapsibleGroup 包起来时传 `headless: true` —— 不渲染
   * 自己的「日志」小标题（用 collapsible header 那一行即可，避免重复）。
   */
  headless?: boolean;
}

/** 给 collapsible header 复用的 i 图标说明文字。headless 模式丢失了内嵌图标 → 让外面挂一下。 */
// 🔴 差项 2/3：原文逐字是「monitor 是 GUI 应用（`windows_subsystem=windows`），
//    没有 stderr 控制台」＋三处 `tracing` / `tracing layer` / `tracing::error!`。
//    前者正是 `§2.4` 那条纪律逐字禁的「**文件路径以外的源码住址**」，
//    后者是那一族的**内部标识符外泄** —— 用户不需要知道我们用的是哪个日志库。
//    ⇒ 说**用户看得见的事实**（日志写到哪、什么时候弹提示），不说我们是怎么实现的。
// 做成函数、用到时才取文（模块顶层不留取文口调用 —— 顶层调用会让 Rollup 把设置面板挪进主窗共享 chunk）。
const DIAGNOSTICS_INFO_TEXT = (): string =>
  copyText("diagnostics.info.logs");

export class DiagnosticsSection {
  private root: HTMLElement;
  private current: DiagnosticsConfig = {
    log_enabled: true,
    log_level: "info",
    error_toast: true,
    max_files: 3,
  };
  private headless: boolean;

  private logEnabledCheckbox!: HTMLInputElement;
  private levelSelect!: HTMLSelectElement;
  private errorToastCheckbox!: HTMLInputElement;
  private pathSpan!: HTMLSpanElement;
  private sizeSpan!: HTMLSpanElement;
  private openFileBtn!: HTMLButtonElement;
  /** 本机常驻后端（脱离那条载体）的输出：路径 · 大小 · 打开。 */
  private backendSpan!: HTMLSpanElement;
  private openBackendBtn!: HTMLButtonElement;
  private backendPath: string | null = null;
  /** 读不到当前设置时，原因落在这一块上（不再只进 console）。 */
  private readFailLine!: HTMLElement;

  constructor(opts: DiagnosticsSectionOptions = {}) {
    this.headless = opts.headless ?? false;
    this.root = this.build();
    // 🔴 步 2：**构造期不再发 I/O。**
    // 原来这里是 `void this.refresh()`，而它一次发 **2 发** IPC
    // （`get_diagnostics_config` + `get_log_file_info`），且这一块住「应用」页、
    // 落地页是「机器」⇒ 那 2 发在用户还没点进「应用」之前就打出去了。
    // 现在由宿主（`panel.ts`）在该页首次可见时调 `loadNow()`。
  }

  get element(): HTMLElement {
    return this.root;
  }

  /**
   * 步 2：宿主在「这一页首次可见」时调它。
   * ⚠ **幂等由宿主保证**（`panel.ts::pagesLoaded`）——两处都判会挡住 `open()` 的重读。
   */
  loadNow(): void {
    void this.refresh();
  }

  private build(): HTMLElement {
    const group = document.createElement("div");
    // headless 模式：不挂 .settings-group 边距，直接作为 collapsible body 内容
    group.className = this.headless ? "settings-headless" : "settings-group";

    if (!this.headless) {
      // 标题 + 信息图标
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      // 🔴 「名字」：**「诊断」→「日志」**。
      // 理由是**重名**，不是 R5：`§5.3` 把机器列表页那块「还差什么（诊断汇总）」
      // 改名成「诊断」，那一落地设置面板里就会同时有两个「诊断」——
      // 一个是「这台机器还缺什么」，一个是「monitor 的日志开关」，两者毫无关系。
      // ⇒ 这一块让名。它的全部内容（写不写日志文件 / 级别 / 错误提示 / 路径 / 大小 / 打开）
      //   都是日志的事。
      // ⚠ `§10.3` 逐字要求这次改名与 `§5.3` 那个改名**同拍**，怕的是中间有一段时间
      //   两个「诊断」并存。这一个先改了；`§5.3` 那一半也落了
      //   （`remote-section.ts::renderGaps` 的块标题）。判据 `settings-unique-names.vitest.ts`（`§8 #11`）。
      heading.textContent = copyText("diagnostics.build.title");
      heading.appendChild(makeInfoIcon(DIAGNOSTICS_INFO_TEXT()));
      group.appendChild(heading);
    }

    // 🔴 那处真缺陷 ＋ `§8` 判据 #2（「没有复选框在加载后自己改状态」）：
    //    读失败原先只 `console.warn` ⇒ 三个控件**静默显示构造期默认值**，用户一点就把
    //    假状态写回去 —— 与 `§1` 那个「启用远端模式」自己从 ☐ 跳到 ☑ 是**同一种伤**。
    //    ⇒ 失败落在这一块上，并且三个控件在**读回来之前不可交互**（见 `setControlsReady`）。
    this.readFailLine = document.createElement("div");
    this.readFailLine.className = "settings-banner";
    group.appendChild(this.readFailLine);

    // 1. 启用 log 文件 toggle
    const logRow = document.createElement("label");
    logRow.className = "settings-row settings-row-checkbox";
    this.logEnabledCheckbox = document.createElement("input");
    this.logEnabledCheckbox.type = "checkbox";
    this.logEnabledCheckbox.className = "settings-checkbox";
    this.logEnabledCheckbox.addEventListener("change", () => void this.save());
    logRow.appendChild(this.logEnabledCheckbox);
    const logLabel = document.createElement("span");
    logLabel.className = "settings-checkbox-label";
    // 差项 4（中英混写）＋ 那条「R5 规矩文字命中、检法抓不到」的建议：
    // 「启用 log 文件」是**动宾**且中英混写 ⇒ 改成名词短语「日志文件」，
    // 「启用不启用」由复选框这个控件本身表达。
    // ⚠ **「复选框标签要不要给 R5 开豁免」这件事本篇判不了**（`§10.5` #3：规矩禁祈使、
    //   检法只扫问号与口语词，两者不一致，是个洞）—— 这里只按建议改措辞，
    //   **不动 R5 的检法**，也不声称这一格已决。
    logLabel.textContent = copyText("diagnostics.file.enable");
    logRow.appendChild(logLabel);
    logRow.appendChild(
      makeInfoIcon(
        copyText("diagnostics.file.enableHint"),
      ),
    );
    group.appendChild(logRow);

    // 2. 日志级别 select
    const levelRow = document.createElement("div");
    levelRow.className = "settings-row";
    const levelLabel = document.createElement("span");
    levelLabel.className = "settings-label";
    levelLabel.textContent = copyText("diagnostics.build.level");
    levelRow.appendChild(levelLabel);
    this.levelSelect = document.createElement("select");
    this.levelSelect.className = "settings-input";
    for (const lv of LOG_LEVELS) {
      const opt = document.createElement("option");
      opt.value = lv;
      opt.textContent = lv;
      this.levelSelect.appendChild(opt);
    }
    this.levelSelect.addEventListener("change", () => void this.save());
    levelRow.appendChild(this.levelSelect);
    levelRow.appendChild(
      makeInfoIcon(
        copyText("diagnostics.level.hint"),
      ),
    );
    group.appendChild(levelRow);

    // 3. error toast toggle
    const toastRow = document.createElement("label");
    toastRow.className = "settings-row settings-row-checkbox";
    this.errorToastCheckbox = document.createElement("input");
    this.errorToastCheckbox.type = "checkbox";
    this.errorToastCheckbox.className = "settings-checkbox";
    this.errorToastCheckbox.addEventListener("change", () => void this.save());
    toastRow.appendChild(this.errorToastCheckbox);
    const toastLabel = document.createElement("span");
    toastLabel.className = "settings-checkbox-label";
    toastLabel.textContent = copyText("diagnostics.toast.enable");
    toastRow.appendChild(toastLabel);
    toastRow.appendChild(
      makeInfoIcon(
        copyText("diagnostics.toast.enableHint"),
      ),
    );
    group.appendChild(toastRow);

    // 4. log 路径 + 大小 + 操作按钮
    const pathRow = document.createElement("div");
    pathRow.className = "settings-row settings-row-stack";
    const pathLabel = document.createElement("span");
    pathLabel.className = "settings-label";
    pathLabel.textContent = copyText("diagnostics.file.location");
    pathRow.appendChild(pathLabel);
    this.pathSpan = document.createElement("span");
    this.pathSpan.className = "settings-cc-autolaunch-path-value";
    this.pathSpan.style.fontFamily = "var(--font-mono, monospace)";
    this.pathSpan.style.fontSize = "11px";
    this.pathSpan.style.wordBreak = "break-all";
    // 步 1（差项 5）：**这一块骨架基本不欠** —— 结构在 `build()` 里就搭齐、
    // 刷新只改文本。唯一会长高的是这条 `word-break: break-all` 的路径：
    // 从 `—` 变成一条可换行的长路径 ⇒ 它下面的东西往下掉。
    // ⇒ 只钉这一行的高度，别的不动（不欠的地方不假装补）。
    holdSkeletonHeight(pathRow, "logs");
    this.pathSpan.textContent = copyText("diagnostics.build.empty");
    pathRow.appendChild(this.pathSpan);
    group.appendChild(pathRow);

    const sizeRow = document.createElement("div");
    sizeRow.className = "settings-row";
    const sizeLabel = document.createElement("span");
    sizeLabel.className = "settings-label";
    sizeLabel.textContent = copyText("diagnostics.file.size");
    sizeRow.appendChild(sizeLabel);
    this.sizeSpan = document.createElement("span");
    this.sizeSpan.className = "settings-cc-stat-value";
    this.sizeSpan.textContent = copyText("diagnostics.build.empty");
    sizeRow.appendChild(this.sizeSpan);
    group.appendChild(sizeRow);

    // 5. 本机常驻后端的输出。它脱离 monitor 常驻时没有别的地方可说话
    //    （拨号的 host key 警告、中转起不来的原因都在它那里）⇒ 它自己落一份有上限、滚动的文件，这里看得到。
    const backendRow = document.createElement("div");
    backendRow.className = "settings-row settings-row-stack";
    const backendLabel = document.createElement("span");
    backendLabel.className = "settings-label";
    backendLabel.textContent = copyText("diagnostics.backend.label");
    backendRow.appendChild(backendLabel);
    this.backendSpan = document.createElement("span");
    this.backendSpan.className = "settings-cc-autolaunch-path-value";
    this.backendSpan.style.fontFamily = "var(--font-mono, monospace)";
    this.backendSpan.style.fontSize = "11px";
    this.backendSpan.style.wordBreak = "break-all";
    this.backendSpan.textContent = copyText("diagnostics.build.empty");
    backendRow.appendChild(this.backendSpan);
    this.openBackendBtn = document.createElement("button");
    this.openBackendBtn.type = "button";
    this.openBackendBtn.className = "settings-btn";
    this.openBackendBtn.textContent = copyText("diagnostics.backend.open");
    this.openBackendBtn.disabled = true;
    this.openBackendBtn.addEventListener("click", () =>
      void withPending(this.openBackendBtn, copyText("diagnostics.build.opening"), () => this.openBackendFile()),
    );
    backendRow.appendChild(this.openBackendBtn);
    group.appendChild(backendRow);

    const btnRow = document.createElement("div");
    btnRow.className = "settings-cc-profile-buttons";
    this.openFileBtn = document.createElement("button");
    this.openFileBtn.type = "button";
    this.openFileBtn.className = "settings-btn";
    this.openFileBtn.textContent = copyText("diagnostics.build.openFile");
    // 步 4·E：这三个都会走一次 IPC，期间按住对应的按钮。
    this.openFileBtn.addEventListener("click", () =>
      void withPending(this.openFileBtn, copyText("diagnostics.build.opening"), () => this.openFile()),
    );
    btnRow.appendChild(this.openFileBtn);

    const openDirBtn = document.createElement("button");
    openDirBtn.type = "button";
    openDirBtn.className = "settings-btn";
    openDirBtn.textContent = copyText("diagnostics.build.openDir");
    openDirBtn.addEventListener("click", () =>
      void withPending(openDirBtn, copyText("diagnostics.build.opening"), () => this.openDir()),
    );
    btnRow.appendChild(openDirBtn);

    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "settings-btn";
    refreshBtn.textContent = copyText("diagnostics.build.refresh");
    refreshBtn.title = copyText("diagnostics.build.refreshHint");
    refreshBtn.addEventListener("click", () =>
      void withPending(refreshBtn, copyText("diagnostics.build.reading"), () => this.refresh()),
    );
    btnRow.appendChild(refreshBtn);
    group.appendChild(btnRow);

    this.setControlsReady(false);
    return group;
  }

  /** 三个会写回后端的控件：读回来之前一律不可交互（判据 #2 的形状）。 */
  private setControlsReady(ready: boolean): void {
    for (const c of [this.logEnabledCheckbox, this.levelSelect, this.errorToastCheckbox]) {
      c.disabled = !ready;
    }
  }

  /** 从后端拉当前配置 + log 文件信息，刷新 UI */
  private async refresh(): Promise<void> {
    try {
      const cfg = await commands.get_diagnostics_config();
      this.current = cfg;
      this.logEnabledCheckbox.checked = cfg.log_enabled;
      this.levelSelect.value = cfg.log_level;
      this.errorToastCheckbox.checked = cfg.error_toast;
      this.readFailLine.textContent = "";
      this.readFailLine.classList.remove("settings-banner-show");
      this.setControlsReady(true);
    } catch (e) {
      // 这一路本来就会失败（`INVARIANTS §15`：日志子系统失败不许挡启动）⇒ 界面必须答得出「读不到」。
      this.setControlsReady(false);
      this.readFailLine.textContent = copyText("diagnostics.refresh.settingsUnreadable", { e: String(e) });
      this.readFailLine.classList.add("settings-banner-show");
    }
    try {
      const info = await commands.get_log_file_info();
      if (info.current_file) {
        this.pathSpan.textContent = info.current_file;
        this.pathSpan.title = info.current_file;
        this.sizeSpan.textContent = formatBytes(info.current_size_bytes);
        this.openFileBtn.disabled = false;
      } else {
        this.pathSpan.textContent = copyText("diagnostics.refresh.noFileYet", { dir: info.dir });
        this.sizeSpan.textContent = copyText("diagnostics.refresh.empty");
        this.openFileBtn.disabled = true;
      }
      // 本机后端那一份（新在前；有旧的一份也不列 —— 打开目录就看得见）。
      const latest = info.backend_stderr[0];
      if (latest) {
        this.backendPath = latest.path;
        this.backendSpan.textContent = `${latest.path}（${formatBytes(latest.size_bytes)}）`;
        this.backendSpan.title = latest.path;
        this.openBackendBtn.disabled = false;
      } else {
        this.backendPath = null;
        this.backendSpan.textContent = copyText("diagnostics.backend.none");
        this.openBackendBtn.disabled = true;
      }
    } catch (e) {
      console.warn("get_log_file_info failed:", e);
      this.pathSpan.textContent = copyText("diagnostics.refresh.dirUnreadable", { e: String(e) });
    }
  }

  private async openBackendFile(): Promise<void> {
    if (!this.backendPath) return;
    try {
      await openPath(this.backendPath);
    } catch (e) {
      toast(copyText("diagnostics.backend.openFailed"), String(e));
    }
  }

  /** 收到任一控件变化 → 组装 DiagnosticsConfig → invoke set */
  private async save(): Promise<void> {
    const cfg: DiagnosticsConfig = {
      log_enabled: this.logEnabledCheckbox.checked,
      log_level: this.levelSelect.value,
      error_toast: this.errorToastCheckbox.checked,
      max_files: this.current.max_files, // UI 不暴露，保持原值
    };
    try {
      const hint = await commands.set_diagnostics_config({ cfg });
      this.current = cfg;
      if (hint === "needs_restart") {
        // S7 收尾（Phase G 核账逮出来的）：**光弹 toast 不够**。
        //
        // 底部那条常驻条是「现在有什么改动还没生效」的唯一去处，而它此前只有
        // 远端配置一个供给方 —— 于是这里改了 log 开关、6 秒后 toast 消失，
        // 用户再看那条条子是空的，会读成「没有待生效的改动」。
        // **条子的存在本身让它显得权威**，漏一个供给方比没有条子更误导。
        //
        // 两者并存是刻意的：toast 是「刚刚这一下的回执」（事件），
        // 条子是「还欠着没生效」（状态）—— S7 的判据表分的正是这两类。
        markRestartNeeded(copyText("diagnostics.save.fileSwitch"));
        toast(
          copyText("diagnostics.save.done"),
          copyText("diagnostics.save.restartNeeded"),
          { level: "info" },
        );
      }
      await this.refresh();
    } catch (e) {
      toast(copyText("diagnostics.save.failed"), String(e));
      // 失败 → 回退到当前实际值
      await this.refresh();
    }
  }

  private async openFile(): Promise<void> {
    try {
      await commands.open_log_file();
    } catch (e) {
      toast(copyText("diagnostics.openFile.failed"), String(e));
    }
  }

  private async openDir(): Promise<void> {
    try {
      await commands.open_log_dir();
    } catch (e) {
      toast(copyText("diagnostics.openDir.failed"), String(e));
    }
  }
}

