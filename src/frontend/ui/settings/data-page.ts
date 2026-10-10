/**
 * 设置窗「文件与数据」页：两栏 —— 待办（角标）· cc-monitor 放了什么。
 *
 * - 待办：顶上一行总数 ＋［全部重查］；按机器分段（有事的在前），段头 机器名 · 那台的数 ·［打开该机器］；
 *   每件一行：状态点 · 名字 · 类的小标签 · 下一行 文件位置小标签 ＋ 一句现状 · 主按钮。连不上的那台：「{machine} 离线 · 未检查」。
 *   今天的件来自那台后端的 `data-report`（要装 / 要装 · 可选）；单件里每一格都照那份成品画，界面不判。
 * - cc-monitor 放了什么：机器 chip 切换；本机先「Claude 目录」（宿主交进来那一块），再「改过你的文件」（每处 改了什么 · 撤回在哪 ［前往］），
 *   再「cc-monitor 的文件」（本机：宿主交进来那一块）。
 * - 左栏角标 ＝ 各台 `chores` 相加（问不到的那台不算）；主窗口状态栏读同一个数（`data-reads.ts::choresOf`）。
 * 构造零 I/O：宿主在这一页第一次可见时调 [`loadNow`]；之后［全部重查］显式补一刀（切页不重读，不轮询）。
 */
import { tabs } from "../kit/tabs";
import { button } from "../kit/button";
import { countBadge } from "../kit/badge";
import { banner } from "../kit/banner";
import { homeShort } from "../kit/path";
import { copyText } from "../copy-table";
import { writeClipboard } from "../clipboard";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { findHostByOrigin, hostKey, readRemoteConfig } from "../remote-config";
import { noteMachineTmux } from "../resume-defaults";
import { markChore, readDataReport, recallDataReport, type Chore, type ChoreState, type DataReport } from "./data-reads";
import { remoteOwnBlock } from "./remote-own";
import { spanNow } from "../duration-format";
import { choreRow } from "./chore-row";
import { countTags } from "./chore-tags";
import { fold } from "../kit/fold";
import { failToast } from "../kit/toast";
import { revealInFolder } from "../reveal-in-folder";
import { openFileWindow } from "../file-window";
import type { SettingsTarget } from "./open-settings";
import { openUrl } from "@tauri-apps/plugin-opener";

export type DataTab = "chores" | "placed";

/** 从别处带进来时段头高亮多久（与设置窗跳锚点同值）。 */
const FOCUS_HIGHLIGHT_MS = 1500;

export interface DataPageHost {
  /** 本机「Claude 目录」那一块（宿主建、宿主存）。 */
  claudeDir: HTMLElement;
  /** 本机「cc-monitor 的文件」那一块（宿主建）。 */
  ownFiles: HTMLElement;
  /** 跳到设置窗里那一页那一栏那一节。 */
  goTo: (t: SettingsTarget) => void;
  /** 左栏角标（各台进角标的件数相加）。 */
  setBadge: (n: number) => void;
}

/** 一台这一趟的结果：读到 · 读不到（那一句 ＋ 本机后端记着的上次那一份，没有 ⇒ `null`）。 */
type Reading = { ok: true; report: DataReport } | { ok: false; why: string; last: { report: DataReport; atMs: number } | null };

interface Machine {
  origin: Origin;
  name: string;
}

export class DataPage {
  readonly element: HTMLElement;
  private readonly host: DataPageHost;
  private tab: DataTab = "chores";
  private readonly tabStrip: HTMLElement;
  private readonly choresPane: HTMLElement;
  private readonly placedPane: HTMLElement;
  private readonly chips: HTMLElement;
  private readonly changedBox: HTMLElement;
  private readonly offlineBar: HTMLElement;
  /** 只在本机 chip 上出现的两块（Claude 目录 · cc-monitor 的文件，各带段头）。 */
  private readonly localClaude: HTMLElement;
  private readonly localOwn: HTMLElement;
  /** 远端那台的「cc-monitor 的文件」（照那台 `data-report` 的 `own` 排）。 */
  private readonly remoteOwn: HTMLElement;
  private readonly remoteOwnBody: HTMLElement;
  private machines: Machine[] = [{ origin: LOCAL_ORIGIN, name: copyText("remote.cards.local") }];
  private readings = new Map<Origin, Reading>();
  private picked: Origin = LOCAL_ORIGIN;
  /** 点过复制、那台还没认出已做的那几件（界面只多记这一样）。键 ＝ 机器 ＋ 件。 */
  private readonly copied = new Set<string>();
  /** 展开着的那几件（重画不收起）。 */
  private readonly openRows = new Set<string>();
  /** 上一趟每件的态（认出「已做」那一句按它比）。 */
  private readonly lastState = new Map<string, ChoreState>();
  private readonly recognized = new Map<Origin, string[]>();
  /** 带进来的「滚到那台」还没落（那一段还没画出来）。 */
  private pendingFocus: Origin | null = null;
  private generation = 0;

  constructor(host: DataPageHost) {
    this.host = host;
    const root = document.createElement("div");
    root.className = "data-page";
    this.element = root;

    this.tabStrip = document.createElement("div");
    this.tabStrip.className = "data-tabs";
    root.appendChild(this.tabStrip);

    // ── 待办 ──
    this.choresPane = document.createElement("div");
    this.choresPane.className = "data-pane";
    this.choresPane.dataset.pane = "chores";
    root.appendChild(this.choresPane);

    // ── cc-monitor 放了什么 ──
    this.placedPane = document.createElement("div");
    this.placedPane.className = "data-pane";
    this.placedPane.dataset.pane = "placed";
    this.chips = document.createElement("div");
    this.chips.className = "data-chips";
    this.offlineBar = document.createElement("div");
    this.offlineBar.className = "data-offline";
    this.offlineBar.hidden = true;
    const claudeHead = section(copyText("dataPage.claudeDir.title"), copyText("dataPage.claudeDir.sub"));
    const changedHead = section(copyText("dataPage.changed.title"), copyText("dataPage.changed.sub"));
    this.changedBox = document.createElement("div");
    this.changedBox.className = "data-card";
    const pasted = document.createElement("div");
    pasted.className = "settings-hint";
    pasted.textContent = copyText("dataPage.changed.pasted");
    const ownHead = section(copyText("dataPage.own.title"), copyText("dataPage.own.sub"));
    this.localClaude = document.createElement("div");
    this.localClaude.className = "data-local-only";
    this.localClaude.append(claudeHead, host.claudeDir);
    this.localOwn = document.createElement("div");
    this.localOwn.className = "data-local-only";
    this.localOwn.append(ownHead, host.ownFiles);
    this.remoteOwn = document.createElement("div");
    this.remoteOwn.className = "data-remote-only";
    this.remoteOwnBody = document.createElement("div");
    this.remoteOwn.append(section(copyText("dataPage.own.title"), copyText("dataPage.own.sub")), this.remoteOwnBody);
    this.placedPane.append(this.chips, this.offlineBar, this.localClaude, changedHead, this.changedBox, pasted, this.localOwn, this.remoteOwn);
    root.appendChild(this.placedPane);

    this.paintTabs(0);
    this.showTab();
    this.paintChores();
    this.paintPlaced();
  }

  /** 第一次可见 / 每次切进来：读机器表，再逐台问一次。 */
  loadNow(): void {
    void this.refresh();
  }

  /** 带目的地进来：直达那一栏。 */
  openTab(t: DataTab): void {
    this.tab = t;
    this.showTab();
  }

  /**
   * 带目的地进来：`chores:<origin>` ⇒ 待办那一栏、滚到那台那一段、段头高亮 1.5 秒；`placed:<origin>` ⇒ 放了什么那一栏、选中那台。
   * 那台那一段还没画出来（机器表 / 那台还没读回）⇒ 记着，读回来再落。
   */
  focus(anchor: string): void {
    const [kind, ...rest] = anchor.split(":");
    const origin = rest.join(":");
    if (kind === "placed") {
      this.picked = origin;
      this.openTab("placed");
      this.paintPlaced();
      return;
    }
    if (kind !== "chores") return;
    this.openTab("chores");
    this.pendingFocus = origin;
    this.landFocus();
  }

  private landFocus(): void {
    const origin = this.pendingFocus;
    if (origin === null) return;
    const sec = [...this.choresPane.querySelectorAll<HTMLElement>(".data-machine")].find((e) => e.dataset.machine === origin);
    if (!sec || !this.readings.has(origin)) return;
    this.pendingFocus = null;
    sec.scrollIntoView?.({ block: "start" });
    const head = sec.querySelector<HTMLElement>(".data-machine-head");
    head?.classList.add("settings-highlight");
    // 调度：一次性 —— 段头高亮 1.5 秒后摘掉
    window.setTimeout(() => head?.classList.remove("settings-highlight"), FOCUS_HIGHLIGHT_MS);
  }

  private async refresh(): Promise<void> {
    const gen = ++this.generation;
    try {
      const hosts = (await readRemoteConfig()).hosts.map(hostKey).filter((h) => h.trim() !== "");
      this.machines = [{ origin: LOCAL_ORIGIN, name: copyText("remote.cards.local") }, ...hosts.map((h) => ({ origin: h, name: h }))];
    } catch {
      // 读不到机器表 ⇒ 只问本机（远端那几台这一趟不列，不猜）。
    }
    await Promise.all(
      this.machines.map(async (m) => {
        let r: Reading;
        try {
          r = { ok: true, report: await readDataReport(m.origin) };
        } catch (e) {
          r = { ok: false, why: e instanceof Error ? e.message : String(e), last: await recallDataReport(m.origin) };
        }
        if (gen !== this.generation) return;
        this.readings.set(m.origin, r);
        this.noteRecognized(m.origin, r);
        if (r.ok && r.report.tmux !== null) noteMachineTmux(m.origin, r.report.tmux);
      }),
    );
    if (gen !== this.generation) return;
    const total = this.choresTotal();
    this.host.setBadge(total);
    this.paintTabs(total);
    this.paintChores();
    this.paintPlaced();
    this.landFocus();
  }

  /** 进角标的件数：各台 `chores` 相加（读不到的那台不算）。 */
  private choresTotal(): number {
    let n = 0;
    for (const r of this.readings.values()) if (r.ok) n += r.report.chores;
    return n;
  }

  private paintTabs(total: number): void {
    const strip = tabs<DataTab>({
      items: [
        { key: "chores", label: copyText("dataPage.tab.chores") },
        { key: "placed", label: copyText("dataPage.tab.placed") },
      ],
      current: this.tab,
      label: copyText("dataPage.tab.aria"),
      onChange: (k) => {
        this.tab = k;
        this.showTab();
      },
    });
    const badge = countBadge(total, "warn");
    if (badge) strip.querySelector('[data-key="chores"]')?.appendChild(badge);
    this.tabStrip.replaceChildren(strip);
  }

  private showTab(): void {
    this.choresPane.hidden = this.tab !== "chores";
    this.placedPane.hidden = this.tab !== "placed";
  }

  // ── 待办 ──────────────────────────────────────────────────────────────

  private key(origin: Origin, id: string): string {
    return `${origin}\u0000${id}`;
  }

  /** 这一趟新认出「已做」的那几件（上一趟没做 / 过期，这一趟已做）：段头下一行说一句。 */
  private noteRecognized(origin: Origin, r: Reading): void {
    if (!r.ok) return;
    const names: string[] = [];
    for (const c of r.report.todo) {
      const k = this.key(origin, c.id);
      const was = this.lastState.get(k);
      if (c.state === "done" && (was === "todo" || was === "expired")) names.push(c.name);
      if (c.state !== "todo") this.copied.delete(k);
      this.lastState.set(k, c.state);
    }
    this.recognized.set(origin, names);
  }

  private paintChores(): void {
    const pane = this.choresPane;
    pane.replaceChildren();
    const all = [...this.readings.values()].flatMap((r) => (r.ok ? r.report.todo : []));
    const head = document.createElement("div");
    head.className = "data-summary";
    const lead = document.createElement("span");
    lead.textContent = copyText("dataPage.chores.open", { n: all.filter((c) => c.state !== "done" && c.state !== "declined").length });
    head.appendChild(lead);
    for (const t of countTags(all)) head.appendChild(t);
    const sp = document.createElement("span");
    sp.className = "data-sp";
    head.appendChild(sp);
    head.appendChild(button({ label: copyText("dataPage.chores.recheck"), icon: "refresh", kind: "ghost", size: "compact", onClick: () => void this.refresh() }));
    pane.appendChild(head);
    // 有事的那几台在前（顺序照机器表；同一档内不挪）。
    const order = [...this.machines].sort((a, b) => weight(this.readings.get(b.origin)) - weight(this.readings.get(a.origin)));
    for (const m of order) pane.appendChild(this.machineSection(m));
  }

  private machineSection(m: Machine): HTMLElement {
    const sec = document.createElement("section");
    sec.className = "data-machine";
    sec.dataset.machine = m.origin;
    const head = document.createElement("div");
    head.className = "data-machine-head";
    const name = document.createElement("span");
    name.className = "data-machine-name";
    name.textContent = m.name;
    head.appendChild(name);
    const r = this.readings.get(m.origin);
    if (r?.ok) for (const t of countTags(r.report.todo)) head.appendChild(t);
    const sp = document.createElement("span");
    sp.className = "data-sp";
    head.appendChild(sp);
    head.appendChild(
      button({
        label: copyText("dataPage.chores.openMachine"),
        kind: "ghost",
        size: "compact",
        onClick: () => this.host.goTo({ machine: m.origin }),
      }),
    );
    sec.appendChild(head);
    if (!r) {
      sec.appendChild(note(copyText("dataPage.chores.reading")));
      return sec;
    }
    if (!r.ok) {
      sec.appendChild(note(copyText("dataPage.chores.offline", { machine: m.name })));
      return sec;
    }
    const seen = this.recognized.get(m.origin) ?? [];
    if (seen.length) {
      const ok = document.createElement("div");
      ok.className = "data-recognized";
      ok.textContent = copyText("dataPage.chores.recognized", { names: seen.join(copyText("dataPage.chores.sep")) });
      sec.appendChild(ok);
    }
    const active = r.report.todo.filter((c) => c.state !== "done" && c.state !== "declined");
    const done = r.report.todo.filter((c) => c.state === "done");
    const declined = r.report.todo.filter((c) => c.state === "declined");
    const card = document.createElement("div");
    card.className = "data-card";
    if (active.length === 0) card.appendChild(note(copyText("dataPage.chores.noneOn", { machine: m.name })));
    for (const c of active) card.appendChild(this.row(m.origin, c));
    for (const [list, which, title] of [
      [done, "done", copyText("dataPage.chores.doneN", { n: done.length })],
      [declined, "declined", copyText("dataPage.chores.declinedN", { n: declined.length })],
    ] as const) {
      if (!list.length) continue;
      const k = this.key(m.origin, `\u0000${which}`);
      const inner = document.createElement("div");
      for (const c of list) inner.appendChild(this.row(m.origin, c));
      card.appendChild(fold({ title, open: this.openRows.has(k), body: inner, bare: true, onToggle: (o) => (o ? this.openRows.add(k) : this.openRows.delete(k)) }));
    }
    sec.appendChild(card);
    return sec;
  }

  private row(origin: Origin, c: Chore): HTMLElement {
    const k = this.key(origin, c.id);
    return choreRow(c, {
      open: this.openRows.has(k),
      copied: this.copied.has(k),
      onToggle: (o) => {
        if (o) this.openRows.add(k);
        else this.openRows.delete(k);
        this.paintChores();
      },
      onCopied: (text, ids) => void this.copyFor(origin, text, ids),
      onGo: (x) => this.go(origin, x),
      onOpenFile: (path) => void this.openFile(origin, path),
      onDecline: (x, yes) => void this.decline(origin, x, yes),
      onRecheck: () => void this.refresh(),
    });
  }

  private async copyFor(origin: Origin, text: string, ids: string[]): Promise<void> {
    try {
      await writeClipboard(text);
    } catch (e) {
      failToast(copyText("dataPage.chore.copyFailed"), e, { level: "error" });
      return;
    }
    for (const id of ids) this.copied.add(this.key(origin, id));
    this.paintChores();
  }

  private go(origin: Origin, c: Chore): void {
    if (c.action === "how" && c.howUrl) {
      void openUrl(c.howUrl);
      return;
    }
    const g = c.go;
    if (!g) return;
    this.host.goTo(g.page === "machine" ? { machine: origin, tab: g.tab, anchor: g.anchor } : { page: g.page, anchor: g.anchor });
  }

  private async openFile(origin: Origin, path: string): Promise<void> {
    try {
      if (isLocalOrigin(origin)) {
        await revealInFolder(path);
        return;
      }
      const host = findHostByOrigin((await readRemoteConfig()).hosts, origin);
      if (host) await openFileWindow(host, { revealFile: path });
    } catch (e) {
      failToast(copyText("dataPage.chore.openFailed"), e, { level: "error" });
    }
  }

  private async decline(origin: Origin, c: Chore, yes: boolean): Promise<void> {
    try {
      await markChore(origin, { op: yes ? "decline" : "undecline", id: c.id });
    } catch (e) {
      failToast(copyText("dataPage.chore.markFailed"), e, { level: "error" });
      return;
    }
    await this.refresh();
  }

  // ── cc-monitor 放了什么 ────────────────────────────────────────────────────

  private paintPlaced(): void {
    this.chips.replaceChildren(
      ...this.machines.map((m) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "data-chip";
        b.textContent = m.name;
        b.setAttribute("aria-pressed", String(m.origin === this.picked));
        b.addEventListener("click", () => {
          this.picked = m.origin;
          this.paintPlaced();
        });
        return b;
      }),
    );
    const local = isLocalOrigin(this.picked);
    this.localClaude.hidden = !local;
    this.localOwn.hidden = !local;
    this.remoteOwn.hidden = true;
    const r = this.readings.get(this.picked);
    const name = this.machines.find((m) => m.origin === this.picked)?.name ?? this.picked;
    this.offlineBar.hidden = !(r && !r.ok);
    if (r && !r.ok) {
      const said = r.last ? copyText("dataPage.placed.stale", { machine: name, ago: spanNow(r.last.atMs, Date.now()) }) : copyText("dataPage.placed.offline", { machine: name });
      this.offlineBar.replaceChildren(
        banner("warn", said, [button({ label: copyText("dataPage.placed.retry"), size: "compact", onClick: () => void this.refresh() })]),
      );
    }
    this.changedBox.replaceChildren();
    if (!r) {
      this.changedBox.appendChild(note(copyText("dataPage.chores.reading")));
      return;
    }
    // 连不上的那台：有上次的就照上次的画（警告条说多旧），没有就照实说读不到。
    const report = r.ok ? r.report : r.last?.report;
    if (!report) {
      this.changedBox.appendChild(note(copyText("dataPage.placed.unknown")));
      return;
    }
    if (!local) {
      this.remoteOwn.hidden = false;
      const origin = this.picked;
      this.remoteOwnBody.replaceChildren(remoteOwnBlock(report.own, report.home, r.ok ? (abs) => void this.openFile(origin, abs) : null));
    }
    if (report.changedFiles.length === 0) {
      this.changedBox.appendChild(note(copyText("dataPage.changed.none")));
      return;
    }
    for (const c of report.changedFiles) {
      const row = document.createElement("div");
      row.className = "data-item";
      const body = document.createElement("div");
      body.className = "data-item-body";
      const path = document.createElement("div");
      path.className = "data-item-path";
      path.textContent = homeShort(c.path.replace(/^~/, report.home), report.home);
      const what = document.createElement("div");
      what.className = "data-item-sub";
      what.textContent = c.undo ? copyText("dataPage.changed.line", { what: c.what, where: undoWhere(c.undo) }) : c.what;
      body.append(path, what);
      row.appendChild(body);
      const undo = c.undo;
      if (undo) {
        row.appendChild(
          button({
            label: copyText("dataPage.changed.go"),
            size: "compact",
            onClick: () => this.host.goTo(undo.page === "machine" ? { machine: this.picked, tab: undo.tab, anchor: undo.anchor } : { page: undo.page, anchor: undo.anchor }),
          }),
        );
      }
      this.changedBox.appendChild(row);
    }
  }
}

/** 撤回在哪（按后端给的页 · 栏取那一句）。 */
function undoWhere(u: { page: string; tab?: string }): string {
  if (u.page === "ext") return copyText("dataPage.undo.ext");
  if (u.page === "machine" && u.tab === "config") return copyText("dataPage.undo.config");
  return u.page;
}

/** 有事的分量：进角标的件 ＞ 别的没做完的件 ＞ 读不到 ＞ 没事。 */
function weight(r: Reading | undefined): number {
  if (!r) return 0;
  if (!r.ok) return 1;
  const open = r.report.todo.filter((c) => c.state !== "done" && c.state !== "declined").length;
  return r.report.chores * 100 + open * 2;
}

function section(title: string, sub: string): HTMLElement {
  const h = document.createElement("div");
  h.className = "data-section-head";
  const t = document.createElement("span");
  t.className = "data-section-title";
  t.textContent = title;
  const s = document.createElement("span");
  s.className = "settings-hint";
  s.textContent = sub;
  h.append(t, s);
  return h;
}

function note(text: string): HTMLElement {
  const d = document.createElement("div");
  d.className = "settings-hint data-note";
  d.textContent = text;
  return d;
}
