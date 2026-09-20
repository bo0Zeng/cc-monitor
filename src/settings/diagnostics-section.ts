/**
 * 设置面板「日志」区（v2.0.0 落地 issue #4；`70 §10.3` 09-19 改名）。
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
import { showActionFailureToast } from "../error-toast";
import { formatBytes } from "../format";
import { markRestartNeeded } from "./restart-notice"; // S7：待生效改动的唯一去处

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

const LOG_LEVELS = ["trace", "debug", "info", "warn", "error", "off"] as const;

export interface DiagnosticsSectionOptions {
  /**
   * v2.x (issue #7)：被 CollapsibleGroup 包起来时传 `headless: true` —— 不渲染
   * 自己的「日志」小标题（用 collapsible header 那一行即可，避免重复）。
   */
  headless?: boolean;
}

/** 给 collapsible header 复用的 i 图标说明文字。headless 模式丢失了内嵌图标 → 让外面挂一下。 */
// 🔴 `70 §10.3` 差项 2/3：原文逐字是「monitor 是 GUI 应用（`windows_subsystem=windows`），
//    没有 stderr 控制台」＋三处 `tracing` / `tracing layer` / `tracing::error!`。
//    前者正是 `§2.4` 那条纪律逐字禁的「**文件路径以外的源码住址**」，
//    后者是 `91 §2.1` 那一族的**内部标识符外泄** —— 用户不需要知道我们用的是哪个日志库。
//    ⇒ 说**用户看得见的事实**（日志写到哪、什么时候弹提示），不说我们是怎么实现的。
const DIAGNOSTICS_INFO_TEXT =
  "monitor 没有可以看的控制台窗口，所以后端的输出都写进日志文件：\n" +
  "~/.claude/work/logs/monitor.YYYY-MM-DD.log。\n" +
  "出错时同时在右下角弹一条提示，点它直接打开日志文件。";

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

  constructor(opts: DiagnosticsSectionOptions = {}) {
    this.headless = opts.headless ?? false;
    this.root = this.build();
    // 🔴 步 2（`70 §1.3 B` · `§10.4`）：**构造期不再发 I/O。**
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
      // 🔴 `70 §10.3`「名字」：**「诊断」→「日志」**。
      // 理由是**重名**，不是 R5：`§5.3` 把机器列表页那块「还差什么（诊断汇总）」
      // 改名成「诊断」，那一落地设置面板里就会同时有两个「诊断」——
      // 一个是「这台机器还缺什么」，一个是「monitor 的日志开关」，两者毫无关系。
      // ⇒ 这一块让名。它的全部内容（写不写日志文件 / 级别 / 错误提示 / 路径 / 大小 / 打开）
      //   都是日志的事。
      // ⚠ `§10.3` 逐字要求这次改名与 `§5.3` 那个改名**同拍**，怕的是中间有一段时间
      //   两个「诊断」并存。**本拍先改这一个**：先让名不会造出那一档，后改反而会
      //   —— 顺序上这是安全的那一半，另一半（`§5.3` 那个改名）还没落地。
      heading.textContent = "日志";
      heading.appendChild(makeInfoIcon(DIAGNOSTICS_INFO_TEXT));
      group.appendChild(heading);
    }

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
    // `70 §10.3` 差项 4（中英混写）＋ 那条「R5 规矩文字命中、检法抓不到」的建议：
    // 「启用 log 文件」是**动宾**且中英混写 ⇒ 改成名词短语「日志文件」，
    // 「启用不启用」由复选框这个控件本身表达。
    // ⚠ **「复选框标签要不要给 R5 开豁免」这件事本篇判不了**（`§10.5` #3：规矩禁祈使、
    //   检法只扫问号与口语词，两者不一致，是 `91 §4` 的洞）—— 这里只按建议改措辞，
    //   **不动 R5 的检法**，也不声称这一格已决。
    logLabel.textContent = "日志文件";
    logRow.appendChild(logLabel);
    logRow.appendChild(
      makeInfoIcon(
        "按天滚动写入 monitor.YYYY-MM-DD.log，保留最近 3 天。\n" +
          "关闭后已存在的日志文件不会被删除，但不再写新内容。\n" +
          "改这一项要重启 monitor 才生效。",
      ),
    );
    group.appendChild(logRow);

    // 2. 日志级别 select
    const levelRow = document.createElement("div");
    levelRow.className = "settings-row";
    const levelLabel = document.createElement("span");
    levelLabel.className = "settings-label";
    levelLabel.textContent = "日志级别";
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
        "info（默认）：每个 IPC / watcher 关键步骤都记一行。\n" +
          "debug：加细节，约 10× 体积。查疑难问题时短期开启用。\n" +
          "warn / error：只记问题。\n" +
          "off：完全不记。\n" +
          "✓ 切换立即生效，无需重启。",
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
    toastLabel.textContent = "错误提示";
    toastRow.appendChild(toastLabel);
    toastRow.appendChild(
      makeInfoIcon(
        "勾选后：后端报错时右下角弹一条红色提示，6 秒自动消失，点它直接打开日志文件。\n" +
          "限频 60 秒内最多 20 条，避免错误风暴时屏幕被刷满。\n" +
          "改这一项立即生效，不用重启。",
      ),
    );
    group.appendChild(toastRow);

    // 4. log 路径 + 大小 + 操作按钮
    const pathRow = document.createElement("div");
    pathRow.className = "settings-row settings-row-stack";
    const pathLabel = document.createElement("span");
    pathLabel.className = "settings-label";
    pathLabel.textContent = "文件位置";
    pathRow.appendChild(pathLabel);
    this.pathSpan = document.createElement("span");
    this.pathSpan.className = "settings-cc-autolaunch-path-value";
    this.pathSpan.style.fontFamily = "var(--font-mono, monospace)";
    this.pathSpan.style.fontSize = "11px";
    this.pathSpan.style.wordBreak = "break-all";
    // 步 1（`70 §10.3` 差项 5）：**这一块骨架基本不欠** —— 结构在 `build()` 里就搭齐、
    // 刷新只改文本。唯一会长高的是这条 `word-break: break-all` 的路径：
    // 从 `—` 变成一条可换行的长路径 ⇒ 它下面的东西往下掉。
    // ⇒ 只钉这一行的高度，别的不动（不欠的地方不假装补）。
    holdSkeletonHeight(pathRow, "logs");
    this.pathSpan.textContent = "—";
    pathRow.appendChild(this.pathSpan);
    group.appendChild(pathRow);

    const sizeRow = document.createElement("div");
    sizeRow.className = "settings-row";
    const sizeLabel = document.createElement("span");
    sizeLabel.className = "settings-label";
    sizeLabel.textContent = "当前文件大小";
    sizeRow.appendChild(sizeLabel);
    this.sizeSpan = document.createElement("span");
    this.sizeSpan.className = "settings-cc-stat-value";
    this.sizeSpan.textContent = "—";
    sizeRow.appendChild(this.sizeSpan);
    group.appendChild(sizeRow);

    const btnRow = document.createElement("div");
    btnRow.className = "settings-cc-profile-buttons";
    this.openFileBtn = document.createElement("button");
    this.openFileBtn.type = "button";
    this.openFileBtn.className = "settings-btn settings-btn-secondary";
    this.openFileBtn.textContent = "打开日志文件";
    this.openFileBtn.addEventListener("click", () => void this.openFile());
    btnRow.appendChild(this.openFileBtn);

    const openDirBtn = document.createElement("button");
    openDirBtn.type = "button";
    openDirBtn.className = "settings-btn settings-btn-secondary";
    openDirBtn.textContent = "打开日志目录";
    openDirBtn.addEventListener("click", () => void this.openDir());
    btnRow.appendChild(openDirBtn);

    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "settings-btn settings-btn-secondary";
    refreshBtn.textContent = "刷新信息";
    refreshBtn.title = "重新读日志目录看当前文件大小";
    refreshBtn.addEventListener("click", () => void this.refresh());
    btnRow.appendChild(refreshBtn);
    group.appendChild(btnRow);

    return group;
  }

  /** 从后端拉当前配置 + log 文件信息，刷新 UI */
  private async refresh(): Promise<void> {
    try {
      const cfg = await commands.get_diagnostics_config();
      this.current = cfg;
      this.logEnabledCheckbox.checked = cfg.log_enabled;
      this.levelSelect.value = cfg.log_level;
      this.errorToastCheckbox.checked = cfg.error_toast;
    } catch (e) {
      console.warn("get_diagnostics_config failed:", e);
    }
    try {
      const info = await commands.get_log_file_info();
      if (info.current_file) {
        this.pathSpan.textContent = info.current_file;
        this.pathSpan.title = info.current_file;
        this.sizeSpan.textContent = formatBytes(info.current_size_bytes);
        this.openFileBtn.disabled = false;
      } else {
        this.pathSpan.textContent = `（日志目录：${info.dir} —— 还没产生日志文件）`;
        this.sizeSpan.textContent = "—";
        this.openFileBtn.disabled = true;
      }
    } catch (e) {
      console.warn("get_log_file_info failed:", e);
      this.pathSpan.textContent = `（读不到日志目录：${String(e)}）`;
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
        markRestartNeeded("日志文件开关");
        showActionFailureToast(
          "设置已保存",
          "改「日志文件」这一项要重启 monitor 才生效。",
          { level: "info", durationMs: 6000 },
        );
      }
      await this.refresh();
    } catch (e) {
      showActionFailureToast("保存诊断配置失败", String(e));
      // 失败 → 回退到当前实际值
      await this.refresh();
    }
  }

  private async openFile(): Promise<void> {
    try {
      await commands.open_log_file();
    } catch (e) {
      showActionFailureToast("打开 log 文件失败", String(e));
    }
  }

  private async openDir(): Promise<void> {
    try {
      await commands.open_log_dir();
    } catch (e) {
      showActionFailureToast("打开 log 目录失败", String(e));
    }
  }
}

