/**
 * 设置窗「文件与数据」页：两栏 —— 要你动手（角标）· cc-monitor 放了什么。
 *
 * - 要你动手：顶上一行总数 ＋［全部重查］；按机器分段（有事的在前），段头 机器名 · 那台的数 ·［打开该机器］；
 *   每件一行：状态点 · 名字 · 类的小标签 · 下一行 文件位置小标签 ＋ 一句现状 · 主按钮。连不上的那台：「{machine} 离线 · 未检查」。
 *   今天的件来自那台后端的 `data-report`（要装 / 要装 · 可选）；单件里每一格都照那份成品画，界面不判。
 * - cc-monitor 放了什么：机器 chip 切换；本机先「Claude 目录」（宿主交进来那一块），再「改过你的文件」（每处 改了什么 · 撤回在哪 ［前往］），
 *   再「cc-monitor 的文件」（本机：宿主交进来那一块）。
 * - 左栏角标 ＝ 各台 `chores` 相加（问不到的那台不算）；主窗口状态栏读同一个数（`data-reads.ts::choresOf`）。
 * 构造零 I/O：宿主在这一页第一次可见时调 [`loadNow`]；之后［全部重查］显式补一刀（切页不重读，不轮询）。
 */
import { tabs } from "../kit/tabs";
import { button } from "../kit/button";
import { tag, countBadge } from "../kit/badge";
import { banner } from "../kit/banner";
import { homeShort } from "../kit/path";
import { copyText } from "../copy-table";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { hostKey, readRemoteConfig } from "../remote-config";
import { noteMachineTmux } from "../resume-defaults";
import { readDataReport, type DataReport, type NeedsInstall } from "./data-reads";
import type { SettingsTarget } from "./open-settings";
import { openUrl } from "@tauri-apps/plugin-opener";

export type DataTab = "chores" | "placed";

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

/** 一台这一趟的结果：读到 · 读不到（那一句）。 */
type Reading = { ok: true; report: DataReport } | { ok: false; why: string };

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
  private machines: Machine[] = [{ origin: LOCAL_ORIGIN, name: copyText("remote.cards.local") }];
  private readings = new Map<Origin, Reading>();
  private picked: Origin = LOCAL_ORIGIN;
  private generation = 0;

  constructor(host: DataPageHost) {
    this.host = host;
    const root = document.createElement("div");
    root.className = "data-page";
    this.element = root;

    this.tabStrip = document.createElement("div");
    this.tabStrip.className = "data-tabs";
    root.appendChild(this.tabStrip);

    // ── 要你动手 ──
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
    this.placedPane.append(this.chips, this.offlineBar, this.localClaude, changedHead, this.changedBox, pasted, this.localOwn);
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
          r = { ok: false, why: e instanceof Error ? e.message : String(e) };
        }
        if (gen !== this.generation) return;
        this.readings.set(m.origin, r);
        if (r.ok && r.report.tmux !== null) noteMachineTmux(m.origin, r.report.tmux);
      }),
    );
    if (gen !== this.generation) return;
    const total = this.choresTotal();
    this.host.setBadge(total);
    this.paintTabs(total);
    this.paintChores();
    this.paintPlaced();
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

  // ── 要你动手 ──────────────────────────────────────────────────────────────

  private paintChores(): void {
    const pane = this.choresPane;
    pane.replaceChildren();
    const all = [...this.readings.values()].flatMap((r) => (r.ok ? r.report.needsInstall : []));
    const head = document.createElement("div");
    head.className = "data-summary";
    const lead = document.createElement("span");
    lead.textContent = copyText("dataPage.chores.open", { n: all.length });
    head.appendChild(lead);
    const must = all.filter((n) => n.required).length;
    if (must > 0) head.appendChild(tag(copyText("dataPage.chores.countInstall", { n: must })));
    if (all.length - must > 0) head.appendChild(tag(copyText("dataPage.chores.countOptional", { n: all.length - must })));
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
    if (r?.ok) {
      const must = r.report.needsInstall.filter((n) => n.required).length;
      const opt = r.report.needsInstall.length - must;
      if (must > 0) head.appendChild(tag(copyText("dataPage.chores.countInstall", { n: must })));
      if (opt > 0) head.appendChild(tag(copyText("dataPage.chores.countOptional", { n: opt })));
    }
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
    } else if (!r.ok) {
      sec.appendChild(note(copyText("dataPage.chores.offline", { machine: m.name })));
    } else if (r.report.needsInstall.length === 0) {
      sec.appendChild(note(copyText("dataPage.chores.none")));
    } else {
      const card = document.createElement("div");
      card.className = "data-card";
      for (const n of r.report.needsInstall) card.appendChild(this.installRow(n));
      sec.appendChild(card);
    }
    return sec;
  }

  /** 一件「要装」：状态点 · 名字 · 类 · 下一行 位置小标签 ＋ 一句为什么 · ［安装方法］（开那个工具自己的安装说明）。 */
  private installRow(n: NeedsInstall): HTMLElement {
    const row = document.createElement("div");
    row.className = "data-item";
    row.dataset.chore = n.id;
    const dot = document.createElement("span");
    dot.className = "data-dot";
    dot.dataset.tone = n.required ? "warn" : "muted";
    row.appendChild(dot);
    const body = document.createElement("div");
    body.className = "data-item-body";
    const title = document.createElement("div");
    title.className = "data-item-title";
    title.textContent = copyText("dataPage.install.title", { name: n.name });
    title.appendChild(tag(n.required ? copyText("dataPage.install.kind") : copyText("dataPage.install.kindOptional")));
    const sub = document.createElement("div");
    sub.className = "data-item-sub";
    const loc = document.createElement("code");
    loc.className = "data-loc";
    loc.textContent = n.what;
    const why = document.createElement("span");
    why.textContent = n.required ? copyText("dataPage.install.whyRequired") : copyText("dataPage.install.whyOptional");
    sub.append(loc, why);
    body.append(title, sub);
    row.appendChild(body);
    if (n.howUrl) {
      const url = n.howUrl;
      row.appendChild(button({ label: copyText("dataPage.install.how"), size: "compact", onClick: () => void openUrl(url) }));
    }
    return row;
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
    const r = this.readings.get(this.picked);
    const name = this.machines.find((m) => m.origin === this.picked)?.name ?? this.picked;
    this.offlineBar.hidden = !(r && !r.ok);
    if (r && !r.ok) {
      this.offlineBar.replaceChildren(
        banner("warn", copyText("dataPage.placed.offline", { machine: name }), [
          button({ label: copyText("dataPage.placed.retry"), size: "compact", onClick: () => void this.refresh() }),
        ]),
      );
    }
    this.changedBox.replaceChildren();
    if (!r) {
      this.changedBox.appendChild(note(copyText("dataPage.chores.reading")));
      return;
    }
    if (!r.ok) {
      this.changedBox.appendChild(note(copyText("dataPage.placed.unknown")));
      return;
    }
    if (r.report.changedFiles.length === 0) {
      this.changedBox.appendChild(note(copyText("dataPage.changed.none")));
      return;
    }
    for (const c of r.report.changedFiles) {
      const row = document.createElement("div");
      row.className = "data-item";
      const body = document.createElement("div");
      body.className = "data-item-body";
      const path = document.createElement("div");
      path.className = "data-item-path";
      path.textContent = homeShort(c.path.replace(/^~/, r.report.home), r.report.home);
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

/** 有事的分量：进角标的件 ＞ 可选的件 ＞ 读不到 ＞ 没事。 */
function weight(r: Reading | undefined): number {
  if (!r) return 0;
  if (!r.ok) return 1;
  const must = r.report.needsInstall.filter((n) => n.required).length;
  return must * 100 + r.report.needsInstall.length * 2;
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
