/**
 * 设置窗「日志」页：复制诊断信息（页头主按钮）· 未识别数据（一行，计入诊断信息）· 日志写入文件（改了重启后生效）·
 * 记多细 · 后台出错弹提示 · 文件（今天的日志 · 本机 cc-monitor 输出）· 远端输出去哪看。
 *
 * 诊断信息整段由壳一个命令出（`diagnostics_report`：各版本 · 各台状态与原因码 · 未识别数据 · 日志位置；不含会话内容与 key），
 * 这一页只排版、只交原料：各台的记录账（经通道问那台 `drift-report`，问不到交 `null`）与认不出的顶层键（键表住界面）。
 * 那一行「未识别数据 · …」读同一份答复里的数。读到之前 / 读不到 ⇒ 复制置灰（悬停说为什么）。
 * 构造零 I/O：宿主在这一页第一次可见时调 [`loadNow`]。
 */

import { commands } from "../ipc/commands";
import { toast } from "../kit/toast";
import { button, setDisabled } from "../kit/button";
import { toggleSwitch } from "../kit/switch";
import { icon } from "../kit/icon";
import { homeShort } from "../kit/path";
import { formatBytes } from "../format";
import { clearRestartNeeded, markRestartNeeded } from "./restart-notice";
import { restartNowButton } from "./restart-now";
import { unknownConfigKeys } from "../config";
import { copyText } from "../copy-table";
import { openPath } from "@tauri-apps/plugin-opener";
import { homeDir } from "@tauri-apps/api/path";
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import type { DiagnosticsReport } from "../generated/DiagnosticsReport";
import { readRecordDrift, type RecordDriftFace } from "../record-reads";
import { hostKey, readRemoteConfig } from "../remote-config";
import { LOCAL_ORIGIN } from "../ipc/origin";
import { detailOf, sayWithDetail } from "../kit/detail";

/** 记多细：界面上的四档（后端那一格的值 ＋ 怎么说）。 */
const LEVELS = (): readonly (readonly [string, string])[] => [
  ["error", copyText("diagnostics.level.error")],
  ["warn", copyText("diagnostics.level.warn")],
  ["info", copyText("diagnostics.level.info")],
  ["debug", copyText("diagnostics.level.debug")],
];

type SwitchHandle = ReturnType<typeof toggleSwitch>;

export class DiagnosticsSection {
  private readonly root: HTMLElement;
  private current: DiagnosticsConfig | null = null;
  /** 这一次运行起来时「写进文件」那一格的值（拨回它 ⇒ 不用重启，两处提示一起消）。 */
  private logEnabledAtStart: boolean | null = null;
  private report: DiagnosticsReport | null = null;

  private readonly readFailLine: HTMLElement;
  private readonly unknownLine: HTMLElement;
  private readonly copyBtns: HTMLButtonElement[] = [];
  private readonly logEnabled: SwitchHandle;
  private readonly restartLine: HTMLElement;
  private readonly levelSelect: HTMLSelectElement;
  private readonly errorToast: SwitchHandle;
  private readonly saveError: HTMLElement;
  private readonly todayRow: HTMLElement;
  private readonly outputRow: HTMLElement;
  private readonly fallback: HTMLElement;

  constructor() {
    const root = document.createElement("div");
    root.className = "settings-headless diag-page";
    this.root = root;

    this.readFailLine = document.createElement("div");
    this.readFailLine.className = "settings-row-error";
    this.readFailLine.hidden = true;
    root.appendChild(this.readFailLine);

    // ── 未识别数据（一行；数读诊断信息那一份）──
    const unknown = this.line(copyText("diagnostics.unknown.label"));
    this.unknownLine = unknown.help;
    this.unknownLine.textContent = copyText("diagnostics.unknown.reading");
    unknown.row.appendChild(this.copyButton("compact"));
    root.appendChild(unknown.row);

    // ── 日志写入文件（改了重启后生效）──
    this.restartLine = document.createElement("div");
    this.restartLine.className = "diag-restart";
    this.restartLine.hidden = true;
    this.restartLine.append(icon("warning", "compact"), document.createTextNode(copyText("diagnostics.file.restart")), restartNowButton());
    this.logEnabled = toggleSwitch({
      label: copyText("diagnostics.file.enable"),
      on: false,
      onChange: (on) => this.save({ log_enabled: on }),
    });
    this.logEnabled.root.querySelector("span")?.appendChild(this.restartLine);
    root.appendChild(this.wrap(this.logEnabled.root));

    // ── 记多细 ──
    const level = this.line(copyText("diagnostics.build.level"));
    this.levelSelect = document.createElement("select");
    this.levelSelect.className = "settings-input";
    for (const [v, said] of LEVELS()) {
      const opt = document.createElement("option");
      opt.value = v;
      opt.textContent = said;
      this.levelSelect.appendChild(opt);
    }
    this.levelSelect.addEventListener("change", () => void this.save({ log_level: this.levelSelect.value }));
    level.row.appendChild(this.levelSelect);
    root.appendChild(level.row);

    // ── 后台出错 · 弹提示 ──
    this.errorToast = toggleSwitch({
      label: copyText("diagnostics.toast.enable"),
      on: false,
      onChange: (on) => this.save({ error_toast: on }),
    });
    root.appendChild(this.wrap(this.errorToast.root));
    this.saveError = document.createElement("div");
    this.saveError.className = "settings-row-error";
    this.saveError.hidden = true;
    root.appendChild(this.saveError);

    // ── 文件 ──
    const head = document.createElement("div");
    head.className = "diag-files-title";
    head.textContent = copyText("diagnostics.files.title");
    root.appendChild(head);
    const card = document.createElement("div");
    card.className = "diag-files";
    this.todayRow = document.createElement("div");
    this.todayRow.className = "diag-file";
    this.todayRow.dataset.role = "today-log";
    this.outputRow = document.createElement("div");
    this.outputRow.className = "diag-file";
    this.outputRow.dataset.role = "local-output";
    card.append(this.todayRow, this.outputRow);
    root.appendChild(card);
    const remote = document.createElement("div");
    remote.className = "settings-hint";
    remote.textContent = copyText("diagnostics.files.remote");
    root.appendChild(remote);

    // 剪贴板不可用时：只读文本框、全选好。
    this.fallback = document.createElement("div");
    this.fallback.className = "diag-fallback";
    this.fallback.hidden = true;
    root.appendChild(this.fallback);

    this.setControlsReady(false);
    this.paintCopy();
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 页头那颗主按钮（与「未识别数据」那一行的是同一件事、同一份文本）。 */
  headButton(): HTMLButtonElement {
    return this.copyButton("regular");
  }

  /** 宿主在这一页第一次可见时调（幂等由宿主保证）。 */
  loadNow(): void {
    void this.refresh();
  }

  private copyButton(size: "compact" | "regular"): HTMLButtonElement {
    const b = button({
      label: copyText("diagnostics.copy.button"),
      kind: size === "regular" ? "primary" : "secondary",
      icon: size === "regular" ? "copy" : undefined,
      size,
      onClick: () => void this.copy(),
    });
    b.dataset.role = "copy-diagnostics";
    this.copyBtns.push(b);
    this.paintCopy();
    return b;
  }

  /** 一行：左边名字 ＋ 下一行小字，右边控件。 */
  private line(label: string): { row: HTMLElement; help: HTMLElement } {
    const row = document.createElement("div");
    row.className = "settings-row settings-switch-row";
    const col = document.createElement("span");
    col.className = "settings-label-col";
    const name = document.createElement("span");
    name.className = "settings-label";
    name.textContent = label;
    const help = document.createElement("span");
    help.className = "settings-hint";
    col.append(name, help);
    row.appendChild(col);
    return { row, help };
  }

  private wrap(el: HTMLElement): HTMLElement {
    const row = document.createElement("div");
    row.className = "settings-switch-row";
    row.appendChild(el);
    return row;
  }

  /** 会写回壳的那三格：读回来之前一律不可交互（构造期的值不是真状态）。 */
  private setControlsReady(ready: boolean): void {
    this.levelSelect.disabled = !ready;
    for (const sw of [this.logEnabled, this.errorToast]) sw.input.setAttribute("aria-disabled", String(!ready));
  }

  /** 复制：读到诊断信息之前置灰，悬停说为什么。 */
  private paintCopy(): void {
    for (const b of this.copyBtns) setDisabled(b, this.report ? null : copyText("diagnostics.copy.notYet"));
  }

  private async refresh(): Promise<void> {
    try {
      const cfg = await commands.get_diagnostics_config();
      this.current = cfg;
      if (this.logEnabledAtStart === null) this.logEnabledAtStart = cfg.log_enabled;
      this.paintConfig(cfg);
      this.readFailLine.hidden = true;
      this.setControlsReady(true);
    } catch (e) {
      this.setControlsReady(false);
      sayWithDetail(this.readFailLine, copyText("diagnostics.refresh.settingsUnreadable"), detailOf(e));
      this.readFailLine.hidden = false;
    }
    await Promise.all([this.paintFiles(), this.readReport()]);
  }

  private paintConfig(cfg: DiagnosticsConfig): void {
    this.logEnabled.set(cfg.log_enabled);
    this.errorToast.set(cfg.error_toast);
    if (![...this.levelSelect.options].some((o) => o.value === cfg.log_level)) {
      // 盘上是这四档以外的值（手改过）：照原样摆出来，不悄悄换成别的。
      const opt = document.createElement("option");
      opt.value = cfg.log_level;
      opt.textContent = cfg.log_level;
      this.levelSelect.appendChild(opt);
    }
    this.levelSelect.value = cfg.log_level;
    this.restartLine.hidden = this.logEnabledAtStart === null || cfg.log_enabled === this.logEnabledAtStart;
  }

  /** 诊断信息那一份：那一行的数 ＋ 复制用的整段。 */
  private async readReport(): Promise<void> {
    try {
      this.report = await commands.diagnostics_report({ configUnknown: unknownConfigKeys(), drift: await driftOfAll() });
      this.unknownLine.textContent = unknownSaid(this.report);
    } catch (e) {
      this.report = null;
      sayWithDetail(this.unknownLine, copyText("diagnostics.unknown.unread", { e: String(e) }), detailOf(e));
    }
    this.paintCopy();
  }

  private async copy(): Promise<void> {
    const r = this.report;
    if (!r) return;
    this.fallback.hidden = true;
    try {
      await navigator.clipboard.writeText(r.text);
      toast(copyText("diagnostics.copy.done"), "", { level: "info" });
    } catch {
      const box = document.createElement("textarea");
      box.className = "settings-input diag-fallback-text";
      box.readOnly = true;
      box.rows = 8;
      box.value = r.text;
      const said = document.createElement("div");
      said.className = "settings-hint";
      said.textContent = copyText("diagnostics.copy.fallback");
      this.fallback.replaceChildren(said, box);
      this.fallback.hidden = false;
      box.focus();
      box.select();
    }
  }

  /** 改一格：交壳；那一格要重启才生效 ⇒ 行内一句 ＋ 顶上那条（拨回原值 ⇒ 两处一起消）。存失败 ⇒ 拇指退回、行下一句。 */
  private async save(patch: Partial<DiagnosticsConfig>): Promise<boolean> {
    const cur = this.current;
    if (!cur) return false;
    const cfg: DiagnosticsConfig = { ...cur, ...patch };
    this.saveError.hidden = true;
    try {
      await commands.set_diagnostics_config({ cfg });
    } catch (e) {
      sayWithDetail(this.saveError, copyText("diagnostics.save.failedLine", { e: String(e) }), detailOf(e));
      this.saveError.hidden = false;
      this.levelSelect.value = cur.log_level;
      return false;
    }
    this.current = cfg;
    const reason = copyText("diagnostics.save.fileSwitch");
    if (this.logEnabledAtStart !== null && cfg.log_enabled !== this.logEnabledAtStart) markRestartNeeded(reason);
    else clearRestartNeeded(reason);
    this.paintConfig(cfg);
    return true;
  }

  private async paintFiles(): Promise<void> {
    let home: string | null = null;
    try {
      home = await homeDir();
    } catch {
      home = null;
    }
    try {
      const info = await commands.get_log_file_info();
      const today = info.current_file;
      this.todayRow.replaceChildren(
        fileText(copyText("diagnostics.files.today"), today ? homeShort(today, home) : copyText("diagnostics.files.noFile", { dir: homeShort(info.dir, home) })),
      );
      if (today) {
        const open = button({ label: copyText("diagnostics.files.open"), size: "compact", onClick: () => void openFile(today) });
        const dir = button({ label: copyText("diagnostics.files.openDir"), kind: "icon", icon: "folder", size: "compact", onClick: () => void openDir() });
        this.todayRow.append(sizeOf(info.current_size_bytes), open, dir);
      }
      const latest = info.backend_stderr[0];
      if (latest) {
        const open = button({ label: copyText("diagnostics.files.open"), size: "compact", onClick: () => void openFile(latest.path) });
        this.outputRow.replaceChildren(fileText(copyText("diagnostics.files.output"), homeShort(latest.path, home)), sizeOf(latest.size_bytes), open);
      } else {
        // 没有就只说没有，不摆按钮。
        this.outputRow.replaceChildren(fileText(copyText("diagnostics.files.output"), copyText("diagnostics.files.outputNone"), false));
      }
    } catch (e) {
      this.todayRow.replaceChildren(fileText(copyText("diagnostics.files.today"), copyText("diagnostics.refresh.dirUnreadable", { e: String(e) }), false));
    }
  }
}

/** 「未识别数据」那一行：有数的几台 · config.json 几项 · 已计入诊断信息；都没有 ⇒ 无；读不到的那台照实说。 */
export function unknownSaid(r: DiagnosticsReport): string {
  const parts: string[] = [];
  for (const m of r.unknown) {
    if (m.records === null) parts.push(copyText("diagnostics.unknown.machineUnread", { machine: m.machine }));
    else if (m.records > 0) parts.push(copyText("diagnostics.unknown.machine", { machine: m.machine, n: m.records }));
  }
  if (r.configUnknown > 0) parts.push(copyText("diagnostics.unknown.config", { n: r.configUnknown }));
  if (parts.length === 0) return copyText("diagnostics.unknown.none");
  return copyText("diagnostics.unknown.line", { parts: parts.join(copyText("kit.text.sep")) });
}

/** 各台的记录账（本机 ＋ 机器表里每一台；问不到 ⇒ `null`，壳照实写「读不到」）。 */
async function driftOfAll(): Promise<{ origin: string; report: { faces: RecordDriftFace[] } | null }[]> {
  let hosts: string[] = [];
  try {
    hosts = (await readRemoteConfig()).hosts.map(hostKey).filter((h) => h.trim() !== "");
  } catch {
    hosts = [];
  }
  return Promise.all(
    [LOCAL_ORIGIN, ...hosts].map(async (origin) => {
      try {
        return { origin, report: { faces: await readRecordDrift(origin) } };
      } catch {
        return { origin, report: null };
      }
    }),
  );
}

function sizeOf(bytes: number): HTMLElement {
  const size = document.createElement("span");
  size.className = "diag-file-size";
  size.textContent = formatBytes(bytes);
  return size;
}

function fileText(label: string, detail: string, path = true): HTMLElement {
  const col = document.createElement("span");
  col.className = "settings-label-col";
  const name = document.createElement("span");
  name.className = "settings-label";
  name.textContent = label;
  const sub = document.createElement("span");
  sub.className = path ? "diag-file-path" : "settings-hint";
  sub.textContent = detail;
  col.append(name, sub);
  return col;
}

async function openFile(path: string): Promise<void> {
  try {
    await openPath(path);
  } catch (e) {
    toast(copyText("diagnostics.openFile.failed"), String(e), { detail: detailOf(e) });
  }
}

async function openDir(): Promise<void> {
  try {
    await commands.open_log_dir();
  } catch (e) {
    toast(copyText("diagnostics.openDir.failed"), String(e), { detail: detailOf(e) });
  }
}
