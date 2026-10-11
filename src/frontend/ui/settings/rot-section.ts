/**
 * 设置 → 机器 → 轮换（稿 `轮换-v3.md` §6，图 08 · 09）：页签 `配置 | 策略`。
 *
 * - 配置：左边配置列表（名 · 描述 · 用的哪份策略 · N 会话 · 本机默认 / 已改 · ⋯）＋ 右边配置编辑器。编辑器是同一个文档做法：
 *   打开 ＝ 一份草稿，改了出 `已改`，`保存` 带 `ifRev` 写回（别处先存了 ⇒ 问 覆盖 · 另存为… · 取消），不再「改一格即存」。
 *   号表一行一个号、列 ＝ 这份策略的每号参数、格 ＝ 这个号自己的数；点一行在下面展开那一行的表单；点行首勾多选 ⇒ 「已选 N 个号」那一条。
 *   整份参数照声明一项一格；预览是同一张合图（`rotation-plan` 跑草稿）。
 * - 策略：顶上一行 `lib · 共用小函数`（引擎，只读）；内置 · 本机各一行（读不懂的照实写错在第几行、不能被选）；详情写描述 · 文件 ·
 *   声明的参数 · 用它的配置 · 试跑（`rotation-strategy-try`，不写盘不换号）· 源码。
 * - 判定全在那台后端（描述 · 校验 · 推演 · 试跑都是它的）；这里只排版。那台配置表 / 策略一变推 `changed {rotation_configs}`，这一栏重读。
 * 判据：`tests/frontend/ui/settings/rot-section.vitest.ts`。
 */
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { bindEvents } from "../events";
import { readRemoteConfig } from "../remote-config";
import { readQuota } from "../quota-reads";
import {
  addStrategy,
  deleteConfig,
  deleteStrategy,
  readConfigs,
  readPlan,
  readStrategies,
  renameConfig,
  saveConfig,
  setDefaultConfig,
  tryStrategy,
  type ConfigsShown,
} from "../rot-reads";
import { combinedChart, type ChartView } from "../rot-chart";
import { bulkBar, cellErrorsFor, paramForm } from "../rot-form";
import { accountAvatarEl } from "../account-color";
import { accountLabel, slotLabel } from "../acct-words";
import { banner } from "../kit/banner";
import { button } from "../kit/button";
import { askText, choiceDialog, confirmDialog, type ConfirmFn } from "../kit/dialog";
import { detailOf, failSaid } from "../kit/detail";
import { icon } from "../kit/icon";
import { openMenu, type MenuItem } from "../kit/menu";
import { select } from "../kit/select";
import { skeletonRows } from "../kit/skeleton";
import { tabs } from "../kit/tabs";
import { toast } from "../kit/toast";
import { copyText } from "../copy-table";
import { machineName } from "../ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import type { AcctRow } from "../generated/AcctRow";
import type { CellError } from "../generated/CellError";
import type { ConfigBody } from "../generated/ConfigBody";
import type { ConfigItem } from "../generated/ConfigItem";
import type { DeclShown } from "../generated/DeclShown";
import type { PlanReply } from "../generated/PlanReply";
import type { StrategiesRead } from "../generated/StrategiesRead";
import type { StrategyItem } from "../generated/StrategyItem";
import type { TryReply } from "../generated/TryReply";
import s from "./rot-section.module.css";

/** 推来的帧合并成一次重读的间隔。 */
const PUSH_COALESCE_MS = 300;

type Tab = "configs" | "strategies";

/** 别处要这一栏打开哪一份（面板「管理配置…」「看说明与源码 ›」带目的地落过来）：按机器记一条，读到那台的表时开。 */
const wanted = new Map<Origin, { tab: Tab; id: string }>();
const wantListeners = new Set<() => void>();

/** 开 `origin` 那台的一份配置（`config:<id>`）/ 一份策略（`strategy:<id>`）。 */
export function openRotationAt(origin: Origin, anchor: string): void {
  const m = /^(config|strategy):(.+)$/.exec(anchor);
  if (!m) return;
  wanted.set(origin, { tab: m[1] === "config" ? "configs" : "strategies", id: m[2] });
  for (const f of wantListeners) f();
}

export interface RotationSectionOptions {
  /** 注入缝（判据换掉确认框）。 */
  confirm?: ConfirmFn;
}

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: (e: HTMLElementTagNameMap[K]) => void, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 一份配置的内容（存下的那一份去掉版本那几格）。 */
function bodyOf(c: ConfigItem): ConfigBody {
  return {
    name: c.name,
    strategy: c.strategy,
    accounts: c.accounts.map((r) => ({ ...r, params: { ...r.params } })),
    params: { ...c.params },
    acctInit: { ...c.acctInit },
    parked: c.parked,
    ...(c.tz !== undefined ? { tz: c.tz } : {}),
  };
}

const same = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b);

function rowKey(r: AcctRow): string {
  return r.start ? "@start" : r.rest ? "@rest" : (r.id ?? "");
}

function rowLabel(r: AcctRow): string {
  return r.start ? copyText("rot.ed.rowStart") : r.rest ? copyText("rot.ed.rowRest") : accountLabel(r.id ?? "");
}

/** 号表一格写成的字（照声明的类型；值照存的原样，不判）。 */
function cellText(d: DeclShown, v: unknown): string {
  const val = v === undefined ? d.init : v;
  if (val === null || val === undefined) return d.type === "line" ? copyText("rot.form.full") : copyText("rot.form.empty");
  switch (d.type) {
    case "bool":
      return val === true ? copyText("rot.ed.on") : copyText("rot.ed.off");
    case "choice":
      return d.options?.find((o) => o.v === val)?.label ?? String(val);
    case "line":
      if (typeof val === "number") return copyText("rot.ed.pct", { n: val });
      return (["5h", "7d"] as const)
        .map((w) => copyText("rot.ed.window", { w: slotLabel(w), v: typeof (val as Record<string, unknown>)[w] === "number" ? copyText("rot.ed.pct", { n: (val as Record<string, number>)[w] }) : copyText("rot.form.full") }))
        .join(copyText("kit.text.sep"));
    case "slots": {
      const list = Array.isArray(val) ? (val as { at: string; line: number }[]) : [];
      if (list.length === 0) return copyText("rot.form.slotNone");
      return list.map((x) => (x.line === 0 ? copyText("rot.ed.slotOff", { at: x.at.replace("-", copyText("rot.form.dash")) }) : copyText("rot.ed.slot", { at: x.at.replace("-", copyText("rot.form.dash")), n: x.line }))).join(copyText("kit.text.sep"));
    }
    default:
      return String(val);
  }
}

function declsText(ds: readonly DeclShown[]): string {
  if (ds.length === 0) return copyText("rot.st.none");
  return ds.map((d) => copyText("rot.st.decl", { label: d.label, init: cellText(d, d.init) })).join(copyText("kit.text.sep"));
}

interface Editor {
  id: string;
  base: ConfigItem;
  draft: ConfigBody;
  errors: CellError[];
  /** 号表里展开着表单的那一行（`@start` · `@rest` · `@init` · 号 id）。 */
  openRow: string | null;
  picked: string[];
  bulk: Record<string, unknown>;
  plan: { view: ChartView; reply: PlanReply | null; key: string; seq: number };
}

export class RotationSection {
  readonly element: HTMLElement;
  private readonly body: HTMLElement;
  private readonly confirm: ConfirmFn;
  private origin: Origin = LOCAL_ORIGIN;
  private tab: Tab = "configs";
  private seq = 0;
  private configs: ConfigsShown | null = null;
  private strategies: StrategiesRead | null = null;
  /** 这台账号库里的号与它是不是按量号（`＋ 加号…` 的候选 · 按量号的 line / slots 两格不出）。 */
  private library: { account: string; api: boolean }[] = [];
  private failed: unknown = null;
  private ed: Editor | null = null;
  /** 策略页选中的那一行（`@lib` ＝ 共用小函数）· 试跑的结果。 */
  private strat: string | null = null;
  private tryCfg: string | null = null;
  private tried: TryReply | null = null;
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private subscribed = false;
  private loaded = false;

  constructor(opts: RotationSectionOptions = {}) {
    this.confirm = opts.confirm ?? confirmDialog;
    const root = el("div", (e) => (e.className = s.rot));
    root.dataset.rotSection = "true";
    const strip = tabs<Tab>({
      items: [
        { key: "configs", label: copyText("rot.set.tabConfigs") },
        { key: "strategies", label: copyText("rot.set.tabStrategies") },
      ],
      current: this.tab,
      label: copyText("rot.set.tabs"),
      onChange: (k) => {
        this.tab = k;
        this.paint();
      },
    });
    strip.dataset.rotTabs = "true";
    this.body = el("div", (e) => (e.className = s.rotBody));
    root.append(strip, this.body);
    this.element = root;
    wantListeners.add(() => {
      if (this.element.isConnected) this.takeWanted();
    });
    subscribeMachine((origin) => {
      if (!this.loaded || origin === this.origin) return;
      this.origin = origin;
      this.ed = null;
      this.strat = null;
      void this.reload();
    });
  }

  /** 宿主在这一栏第一次可见时调（重开设置再调一次）。 */
  loadNow(): void {
    this.loaded = true;
    this.origin = getCurrentMachine();
    this.ed = null;
    this.strat = null;
    void this.reload();
    void this.subscribe();
  }

  private async subscribe(): Promise<void> {
    if (this.subscribed) return;
    this.subscribed = true;
    const hosts = await readRemoteConfig().then(
      (c) => c.hosts.map((h) => h.label),
      () => [] as string[],
    );
    const origins: Origin[] = [LOCAL_ORIGIN, ...hosts];
    const pushed = (origin: Origin): void => {
      if (origin !== this.origin) return;
      if (this.pushTimer !== null) clearTimeout(this.pushTimer);
      // 调度：合批 —— 那台推来的配置表 / 策略 / 会话变更：300ms 内几帧合成一次重读
      this.pushTimer = setTimeout(() => {
        this.pushTimer = null;
        void this.reload(true);
      }, PUSH_COALESCE_MS);
    };
    try {
      await bindEvents(
        { onLine: () => {}, onSessionEnded: () => {}, onChanged: pushed },
        { changed: origins.flatMap((origin) => (["rotation_configs", "rotation"] as const).map((topic) => ({ origin, topic }))) },
      );
    } catch (e) {
      console.warn("[rot] 订不上轮换推送：", e);
    }
  }

  private async reload(quiet = false): Promise<void> {
    const my = ++this.seq;
    const origin = this.origin;
    if (!quiet || this.configs === null) {
      this.body.replaceChildren(skeletonRows(3));
      this.body.setAttribute("aria-busy", "true");
    }
    let cs: ConfigsShown | null = null;
    let st: StrategiesRead | null = null;
    let failed: unknown = null;
    try {
      [cs, st] = await Promise.all([readConfigs(origin), readStrategies(origin)]);
    } catch (e) {
      failed = e;
    }
    if (this.library.length === 0)
      this.library = await readQuota(origin).then(
        (q) => [...q.accounts, ...q.unseen].filter((x) => x.account !== "_").map((x) => ({ account: x.account, api: x.kind === "api" })),
        () => [],
      );
    if (my !== this.seq) return;
    this.body.removeAttribute("aria-busy");
    this.configs = cs;
    this.strategies = st;
    this.failed = failed;
    // 编辑器开着：没改过的跟上新版；改过的留着草稿（保存时撞版本再问）。
    if (this.ed && cs) {
      const now = cs.configs.find((c) => c.id === this.ed?.id);
      if (!now) this.ed = null;
      else if (!this.dirty()) this.ed = { ...this.ed, base: now, draft: bodyOf(now) };
      else this.ed.base = { ...now, rev: this.ed.base.rev };
    }
    if (!this.ed && cs) this.openEditor(cs.configs.find((c) => c.id === cs.defaultConfig) ?? cs.configs[0] ?? null, false);
    this.takeWanted(false);
    this.paint();
  }

  private takeWanted(paint = true): void {
    const w = wanted.get(this.origin);
    if (!w || !this.configs) return;
    wanted.delete(this.origin);
    this.tab = w.tab;
    this.element.querySelector<HTMLElement>(`[data-rot-tabs] [data-key="${w.tab}"]`)?.click();
    if (w.tab === "configs") {
      const c = this.configs.configs.find((x) => x.id === w.id);
      if (c) this.openEditor(c, false);
    } else this.strat = w.id;
    if (paint) this.paint();
  }

  private openEditor(c: ConfigItem | null, paint = true): void {
    if (!c) {
      this.ed = null;
      return;
    }
    this.ed = { id: c.id, base: c, draft: bodyOf(c), errors: [], openRow: null, picked: [], bulk: {}, plan: { view: "12h", reply: null, key: "", seq: 0 } };
    if (paint) this.paint();
  }

  private dirty(): boolean {
    return this.ed !== null && !same(this.ed.draft, bodyOf(this.ed.base));
  }

  private strategy(id: string): StrategyItem | null {
    return this.strategies?.strategies.find((x) => x.id === id) ?? null;
  }

  private paint(): void {
    if (this.configs === null || this.strategies === null) {
      const retry = button({ label: copyText("acct.now.retry"), size: "compact", onClick: () => void this.reload() });
      this.body.replaceChildren(banner("warn", failSaid(copyText("rot.set.unreadable", { machine: machineName(this.origin) }), this.failed), [retry], detailOf(this.failed)));
      return;
    }
    const cols = el("div", (e) => (e.className = s.rotCols));
    if (this.tab === "configs") cols.append(this.configList(this.configs), this.editor());
    else cols.append(this.strategyList(this.strategies), this.strategyDetail(this.strategies));
    this.body.replaceChildren(cols);
  }

  // ─────────────────────────────── 配置列表

  private configList(cs: ConfigsShown): HTMLElement {
    const box = el("div", (e) => (e.className = s.rotListCol));
    const list = el("div", (e) => (e.className = s.rotList));
    list.setAttribute("role", "list");
    for (const c of cs.configs) {
      const row = el("div", (e) => (e.className = s.rotItem));
      row.setAttribute("role", "listitem");
      row.dataset.rotConfig = c.id;
      if (this.ed?.id === c.id) row.dataset.on = "true";
      row.tabIndex = 0;
      const main = el("div", (e) => (e.className = s.rotItemMain));
      const l1 = el("div", (e) => (e.className = s.rotItemL1));
      l1.appendChild(el("span", (e) => (e.className = s.rotItemName), c.name));
      if (c.isDefault) l1.appendChild(el("span", (e) => (e.className = s.rotTag), copyText("rot.cfg.tagDefault")));
      if (this.ed?.id === c.id && this.dirty()) l1.appendChild(el("span", (e) => (e.className = s.rotDirty), copyText("rot.doc.dirty")));
      const desc = el("div", (e) => (e.className = s.rotItemDesc), c.text);
      if (this.strategy(c.strategy)?.error) desc.dataset.error = "true";
      main.append(l1, desc);
      const side = el("span", (e) => (e.className = s.rotItemSide), c.strategyName);
      const used = el("span", (e) => (e.className = s.rotItemSide), c.inUse.length > 0 ? copyText("rot.cfg.inUse", { n: c.inUse.length }) : "");
      const more = el("button", (e) => (e.className = s.rotMore));
      more.type = "button";
      more.setAttribute("aria-label", copyText("rot.set.more", { name: c.name }));
      more.appendChild(icon("more", "compact"));
      more.addEventListener("click", (ev) => {
        ev.stopPropagation();
        openMenu({ el: more, align: "end" }, this.configMenu(c));
      });
      row.append(main, side, used, more);
      const go = (): void => void this.switchTo(c);
      row.addEventListener("click", go);
      row.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") go();
      });
      list.appendChild(row);
    }
    const add = button({ label: copyText("rot.set.new"), icon: "plus", size: "compact", onClick: () => void this.create() });
    add.dataset.rotNew = "true";
    box.append(list, add);
    return box;
  }

  private configMenu(c: ConfigItem): MenuItem[] {
    return [
      { label: copyText("rot.set.rename"), onClick: () => void this.rename(c) },
      { label: copyText("rot.doc.saveAs"), onClick: () => void this.saveAs(bodyOf(c)) },
      { label: copyText("rot.set.makeDefault"), enabled: !c.isDefault, onClick: () => void this.run(() => setDefaultConfig(this.origin, c.id)) },
      { label: copyText("rot.set.delete"), danger: true, onClick: () => void this.remove(c) },
    ];
  }

  /** 换开另一份：草稿改过 ⇒ 先问 保存 · 丢掉 · 取消。 */
  private async switchTo(c: ConfigItem): Promise<void> {
    if (this.ed?.id === c.id) return;
    if (this.dirty() && this.ed) {
      const pick = await choiceDialog({
        title: copyText("rot.doc.askTitle", { name: c.name }),
        action: { key: "keep", label: copyText("rot.doc.keep") },
        others: [{ key: "discard", label: copyText("rot.doc.discard") }],
        blocks: [{ lead: copyText("rot.doc.askLead", { cfg: this.ed.base.name }), lines: [] }],
      });
      if (pick === null) return;
      if (pick === "keep" && !(await this.save())) return;
    }
    this.openEditor(c);
  }

  private async run(f: () => Promise<unknown>): Promise<void> {
    try {
      await f();
    } catch (e) {
      toast(copyText("rot.set.failed", { why: failSaid(copyText("acct.reason.unknown"), e) }), "", { level: "error" });
    }
    await this.reload(true);
  }

  private async create(): Promise<void> {
    const name = await askText({ title: copyText("rot.set.newTitle"), action: copyText("rot.set.newGo"), label: copyText("rot.ed.name") });
    if (name === null || !this.configs) return;
    const from = this.configs.configs.find((c) => c.id === this.configs?.defaultConfig) ?? this.configs.configs[0];
    if (!from) return;
    let got;
    try {
      got = await saveConfig(this.origin, { config: { ...bodyOf(from), name: name.trim() } });
    } catch (e) {
      toast(copyText("rot.set.failed", { why: failSaid(copyText("acct.reason.unknown"), e) }), "", { level: "error" });
      return;
    }
    if (got.state === "refused") toast(copyText("rot.doc.refused", { why: got.errors.map((e) => e.said).join(copyText("kit.text.sep")) }), "", { level: "error" });
    if (got.state === "saved") wanted.set(this.origin, { tab: "configs", id: got.id });
    await this.reload(true);
  }

  private async rename(c: ConfigItem): Promise<void> {
    const name = await askText({ title: copyText("rot.set.renameTitle", { cfg: c.name }), action: copyText("rot.set.renameGo"), label: copyText("rot.ed.name"), initial: c.name });
    if (name === null) return;
    await this.run(async () => {
      const got = await renameConfig(this.origin, { id: c.id, name: name.trim(), ifRev: c.rev });
      if (got.state === "refused") toast(copyText("rot.doc.refused", { why: got.errors.map((e) => e.said).join(copyText("kit.text.sep")) }), "", { level: "error" });
    });
  }

  private async remove(c: ConfigItem): Promise<void> {
    const go = await this.confirm({ title: copyText("rot.set.deleteTitle", { name: c.name }), action: copyText("rot.set.deleteGo"), danger: true, body: copyText("rot.set.deleteBody") });
    if (!go) return;
    await this.run(async () => {
      const got = await deleteConfig(this.origin, c.id);
      if (got.state === "refused") toast(got.said, "", { level: "error" });
      else if (this.ed?.id === c.id) this.ed = null;
    });
  }

  // ─────────────────────────────── 配置编辑器

  /** `保存`：带打开时的版本写回；撞上别处存的 ⇒ 问 覆盖 · 另存为… · 取消；逐格错 ⇒ 落在那一格。 */
  private async save(force = false): Promise<boolean> {
    const ed = this.ed;
    if (!ed) return false;
    let got;
    try {
      got = await saveConfig(this.origin, { id: ed.id, config: ed.draft, ifRev: ed.base.rev, ...(force ? { force: true } : {}) });
    } catch (e) {
      toast(copyText("rot.set.failed", { why: failSaid(copyText("acct.reason.unknown"), e) }), "", { level: "error" });
      return false;
    }
    if (got.state === "saved") {
      ed.errors = [];
      toast(copyText("rot.doc.saved", { cfg: ed.draft.name }), "", { level: "success" });
      await this.reload(true);
      if (this.ed?.id === ed.id && this.configs) {
        const now = this.configs.configs.find((c) => c.id === ed.id);
        if (now) this.openEditor(now);
      }
      return true;
    }
    if (got.state === "refused") {
      ed.errors = got.errors;
      this.paint();
      return false;
    }
    const pick = await choiceDialog({
      title: copyText("rot.doc.clashTitle", { cfg: ed.base.name, at: "" }),
      action: { key: "overwrite", label: copyText("rot.doc.overwrite") },
      others: [{ key: "saveAs", label: copyText("rot.doc.saveAs") }],
      blocks: [{ lead: copyText("rot.doc.clashTheirs"), lines: got.theirs.map((d) => d.text) }],
    });
    if (pick === "overwrite") return this.save(true);
    if (pick === "saveAs") return this.saveAs(ed.draft);
    return false;
  }

  private async saveAs(body: ConfigBody): Promise<boolean> {
    const name = await askText({ title: copyText("rot.doc.saveAsTitle"), action: copyText("rot.doc.saveAsGo"), label: copyText("rot.doc.saveAsName") });
    if (name === null) return false;
    try {
      const got = await saveConfig(this.origin, { config: { ...body, name: name.trim() } });
      if (got.state === "saved") {
        wanted.set(this.origin, { tab: "configs", id: got.id });
        this.ed = null;
        await this.reload(true);
        return true;
      }
      if (got.state === "refused") toast(copyText("rot.doc.refused", { why: got.errors.map((e) => e.said).join(copyText("kit.text.sep")) }), "", { level: "error" });
    } catch (e) {
      toast(copyText("rot.set.failed", { why: failSaid(copyText("acct.reason.unknown"), e) }), "", { level: "error" });
    }
    return false;
  }

  /** 改草稿里的一格：`who` 缺 ＝ 整份参数；`@init` ＝ 新加的号缺省；别的 ＝ 号表那一行。 */
  private setCell(who: string | undefined, k: string, v: unknown): void {
    const ed = this.ed;
    if (!ed) return;
    if (who === undefined) ed.draft.params = { ...ed.draft.params, [k]: v };
    else if (who === "@init") ed.draft.acctInit = { ...ed.draft.acctInit, [k]: v };
    else ed.draft.accounts = ed.draft.accounts.map((r) => (rowKey(r) === who ? { ...r, params: { ...r.params, [k]: v } } : r));
    this.paint();
  }

  private editor(): HTMLElement {
    const box = el("div", (e) => (e.className = s.rotEditor));
    const ed = this.ed;
    if (!ed) return box;
    box.dataset.rotEditor = ed.id;
    const d = ed.draft;
    const st = this.strategy(d.strategy);
    // 头：名字 · 已改 · 保存 另存为… 还原
    const head = el("div", (e) => (e.className = s.rotEdHead));
    const nameLabel = el("label", (e) => (e.className = s.rotEdLabel), copyText("rot.ed.name"));
    const name = el("input", (e) => (e.className = s.rotEdName));
    name.value = d.name;
    name.dataset.rotName = "true";
    name.setAttribute("aria-label", copyText("rot.ed.name"));
    name.addEventListener("change", () => {
      d.name = name.value;
      this.paint();
    });
    nameLabel.appendChild(name);
    head.appendChild(nameLabel);
    if (this.dirty()) {
      head.appendChild(el("span", (e) => (e.className = s.rotDirty), copyText("rot.doc.dirty")));
      head.appendChild(el("span", (e) => (e.className = s.rotSpacer)));
      const save = button({ label: copyText("rot.doc.save"), kind: "primary", size: "compact", onClick: () => void this.save() });
      save.dataset.rotSave = "true";
      head.append(
        save,
        button({ label: copyText("rot.doc.saveAs"), size: "compact", onClick: () => void this.saveAs(d) }),
        button({
          label: copyText("rot.doc.revert"),
          kind: "ghost",
          size: "compact",
          onClick: () => {
            ed.draft = bodyOf(ed.base);
            ed.errors = [];
            this.paint();
          },
        }),
      );
    }
    box.appendChild(head);
    const nameErr = ed.errors.filter((e) => e.cell === "name");
    for (const e of nameErr) box.appendChild(el("div", (e) => (e.className = s.rotErr), e.said));
    // 策略下拉（名 ＋ 描述；读不懂的不能被选）· 看说明与源码 ›
    const sline = el("div", (e) => (e.className = s.rotEdLine));
    sline.appendChild(el("span", (e) => (e.className = s.rotEdLabel), copyText("rot.ed.strategy")));
    const sel = select({
      label: copyText("rot.ed.strategy"),
      options: (this.strategies?.strategies ?? []).map((x) => ({ value: x.id, label: x.name, detail: x.error ? x.error.text : x.text, enabled: x.error === undefined, why: x.error?.text })),
      value: d.strategy,
      menuWidth: 360,
      onChange: (id) => {
        d.strategy = id;
        this.paint();
      },
    });
    sel.el.dataset.rotEdStrategy = "true";
    const src = button({
      label: copyText("rot.par.source"),
      kind: "ghost",
      size: "compact",
      onClick: () => {
        this.strat = d.strategy;
        this.tab = "strategies";
        this.element.querySelector<HTMLElement>('[data-rot-tabs] [data-key="strategies"]')?.click();
      },
    });
    sline.append(sel.el, src);
    box.appendChild(sline);
    if (st?.text) box.appendChild(el("div", (e) => (e.className = s.rotNote), st.text));
    if (!st) return box;
    box.append(this.acctTable(ed, st), this.whole(ed, st), this.preview(ed));
    return box;
  }

  /** 号表：一行一个号 · 列 ＝ 每号参数 · 格 ＝ 这个号自己的数；最底一行「新加的号缺省」。 */
  private acctTable(ed: Editor, st: StrategyItem): HTMLElement {
    const sec = el("div", (e) => (e.className = s.rotSec));
    const h = el("div", (e) => (e.className = s.rotSecHead));
    h.append(el("h4", (e) => (e.className = s.rotSecTitle), copyText("rot.ed.accts")), el("span", (e) => (e.className = s.rotNote), st.ordered ? copyText("rot.ed.acctsNoteOrdered") : copyText("rot.ed.acctsNote")));
    sec.appendChild(h);
    const decls = st.acctParams;
    const table = el("div", (e) => (e.className = s.rotTable));
    table.style.setProperty("--rot-cols", String(decls.length));
    const headRow = el("div", (e) => (e.className = s.rotTr));
    headRow.dataset.head = "true";
    headRow.append(el("span", (e) => (e.className = s.rotTd)), el("span", (e) => (e.className = s.rotTd)), ...decls.map((x) => el("span", (e) => (e.className = s.rotTd), x.label)), el("span", (e) => (e.className = s.rotTd)));
    table.appendChild(headRow);
    const rows = ed.draft.accounts;
    const rowEls: HTMLElement[] = [];
    rows.forEach((r, i) => {
      const key = rowKey(r);
      const tr = el("div", (e) => (e.className = s.rotTr));
      tr.dataset.rotRow = key;
      if (ed.picked.includes(key)) tr.dataset.ticked = "true";
      const lead = el("span", (e) => (e.className = s.rotTd));
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = ed.picked.includes(key);
      box.setAttribute("aria-label", copyText("rot.ed.rowCheck", { name: rowLabel(r) }));
      box.addEventListener("click", (ev) => ev.stopPropagation());
      box.addEventListener("change", () => {
        ed.picked = box.checked ? [...ed.picked, key] : ed.picked.filter((x) => x !== key);
        if (ed.picked.length < 2) ed.bulk = {};
        this.paint();
      });
      lead.appendChild(box);
      if (st.ordered) {
        const handle = el("span", (e) => (e.className = s.rotHandle));
        handle.dataset.rotHandle = key;
        handle.appendChild(icon("drag", "compact"));
        handle.addEventListener("pointerdown", (ev) => this.drag(ev, ed, rowEls, i));
        lead.appendChild(handle);
      }
      const who = el("span", (e) => (e.className = s.rotTd));
      if (r.id) who.append(accountAvatarEl(r.id, { size: 16 }), el("span", (e) => (e.className = s.rotWho), rowLabel(r)));
      else {
        who.appendChild(el("span", (e) => (e.className = s.rotWhoGroup), rowLabel(r)));
        if (r.start) who.appendChild(el("span", (e) => (e.className = s.rotNote), copyText("rot.ed.startNote")));
      }
      const errs = cellErrorsFor(ed.errors, `accounts[${i}]`);
      const cells = decls.map((x) => {
        const td = el("span", (e) => (e.className = s.rotTd), cellText(x, r.params[x.k]));
        if (errs.some((e) => e.k === x.k)) td.dataset.error = "true";
        const orig = ed.base.accounts.find((b) => rowKey(b) === key);
        if (!orig || !same(orig.params[x.k], r.params[x.k])) td.dataset.changed = "true";
        return td;
      });
      const more = el("button", (e) => (e.className = s.rotMore));
      more.type = "button";
      more.setAttribute("aria-label", copyText("rot.set.more", { name: rowLabel(r) }));
      more.appendChild(icon("more", "compact"));
      more.addEventListener("click", (ev) => {
        ev.stopPropagation();
        openMenu({ el: more, align: "end" }, [
          {
            label: copyText("rot.row.leave"),
            enabled: !r.start,
            onClick: () => {
              ed.draft.accounts = ed.draft.accounts.filter((x) => rowKey(x) !== key);
              this.paint();
            },
          },
        ]);
      });
      tr.append(lead, who, ...cells, more);
      tr.addEventListener("click", () => {
        ed.openRow = ed.openRow === key ? null : key;
        this.paint();
      });
      rowEls.push(tr);
      table.appendChild(tr);
      if (ed.openRow === key || errs.length > 0) table.appendChild(this.rowForm(ed, st, key, r.params, errs, r.id !== undefined && this.isApi(r.id)));
    });
    if (ed.picked.length >= 2) {
      table.appendChild(
        bulkBar(
          decls,
          ed.picked.length,
          ed.bulk,
          (k, v) => {
            if (v === undefined) delete ed.bulk[k];
            else ed.bulk[k] = v;
          },
          () => {
            ed.draft.accounts = ed.draft.accounts.map((r) => (ed.picked.includes(rowKey(r)) ? { ...r, params: { ...r.params, ...ed.bulk } } : r));
            ed.bulk = {};
            this.paint();
          },
        ),
      );
    }
    // 新加的号缺省
    const init = el("div", (e) => (e.className = s.rotTr));
    init.dataset.rotRow = "@init";
    init.dataset.init = "true";
    init.append(el("span", (e) => (e.className = s.rotTd)), el("span", (e) => (e.className = s.rotTd), copyText("rot.par.init")), ...decls.map((x) => el("span", (e) => (e.className = s.rotTd), x.type === "slots" ? "" : cellText(x, ed.draft.acctInit[x.k]))), el("span", (e) => (e.className = s.rotTd)));
    init.addEventListener("click", () => {
      ed.openRow = ed.openRow === "@init" ? null : "@init";
      this.paint();
    });
    table.appendChild(init);
    const initErrs = cellErrorsFor(ed.errors, "acctInit");
    if (ed.openRow === "@init" || initErrs.length > 0) table.appendChild(this.rowForm(ed, st, "@init", ed.draft.acctInit, initErrs, false, ["slots"]));
    sec.appendChild(table);
    // ＋ 加号…（账号库里还没在表里的号）
    const named = new Set(ed.draft.accounts.map((r) => r.id).filter((x): x is string => x !== undefined));
    const cand = this.library.map((x) => x.account).filter((a) => !named.has(a));
    const add = button({
      label: copyText("rot.row.addAcct"),
      kind: "ghost",
      size: "compact",
      onClick: () => {
        const items: MenuItem[] =
          cand.length === 0
            ? [{ label: copyText("rot.add.none"), enabled: false }]
            : cand.map((a) => ({
                label: accountLabel(a),
                avatar: accountAvatarEl(a, { size: 16 }),
                onClick: () => {
                  ed.draft.accounts = [...ed.draft.accounts, { id: a, params: { ...ed.draft.acctInit } }];
                  this.paint();
                },
              }));
        openMenu({ el: add, align: "start" }, items);
      },
    });
    add.dataset.rotAdd = "true";
    sec.appendChild(add);
    return sec;
  }

  private isApi(account: string): boolean {
    return this.library.some((x) => x.account === account && x.api);
  }

  private rowForm(ed: Editor, st: StrategyItem, key: string, values: Record<string, unknown>, errors: ReturnType<typeof cellErrorsFor>, metered: boolean, skip: string[] = []): HTMLElement {
    const wrap = el("div", (e) => (e.className = s.rotRowForm));
    wrap.dataset.rotRowForm = key;
    const orig = key === "@init" ? ed.base.acctInit : ed.base.accounts.find((b) => rowKey(b) === key)?.params;
    const changed = new Set(st.acctParams.filter((x) => !orig || !same(orig[x.k], values[x.k])).map((x) => x.k));
    wrap.appendChild(
      paramForm({
        decls: st.acctParams,
        values,
        name: `rot-ed-${ed.id}-${key}`,
        onSet: (k, v) => this.setCell(key, k, v),
        changed,
        was: orig,
        errors,
        metered,
        tz: ed.draft.tz,
        skip,
      }),
    );
    return wrap;
  }

  /** 行首把手拖（只在 `ordered` 的策略）：改配置里号的先后。 */
  private drag(ev: PointerEvent, ed: Editor, rows: HTMLElement[], from: number): void {
    ev.preventDefault();
    ev.stopPropagation();
    let to = from;
    rows[from].dataset.lifted = "true";
    const move = (e: PointerEvent): void => {
      for (const r of rows) delete r.dataset.drop;
      let k = rows.findIndex((r) => e.clientY < r.getBoundingClientRect().top + r.getBoundingClientRect().height / 2);
      if (k < 0) k = rows.length;
      to = k > from ? k - 1 : k;
      if (k < rows.length) rows[k].dataset.drop = "before";
    };
    const up = (): void => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      if (to !== from) {
        const next = [...ed.draft.accounts];
        const [x] = next.splice(from, 1);
        next.splice(to, 0, x);
        ed.draft.accounts = next;
      }
      this.paint();
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  }

  private whole(ed: Editor, st: StrategyItem): HTMLElement {
    const sec = el("div", (e) => (e.className = s.rotSec));
    const h = el("div", (e) => (e.className = s.rotSecHead));
    h.append(el("h4", (e) => (e.className = s.rotSecTitle), copyText("rot.ed.whole")), el("span", (e) => (e.className = s.rotNote), copyText("rot.ed.wholeNote", { strategy: st.name })));
    sec.appendChild(h);
    const changed = new Set(st.params.filter((x) => !same(ed.base.params[x.k], ed.draft.params[x.k])).map((x) => x.k));
    const form = paramForm({
      decls: st.params,
      values: ed.draft.params,
      name: `rot-ed-${ed.id}-whole`,
      onSet: (k, v) => this.setCell(undefined, k, v),
      changed,
      was: ed.base.params,
      errors: cellErrorsFor(ed.errors, "params"),
    });
    form.dataset.columns = "2";
    sec.appendChild(form);
    return sec;
  }

  /** 预览：同一张合图（推演跑的是草稿）。草稿 · 视窗变了才重问。 */
  private preview(ed: Editor): HTMLElement {
    const sec = el("div", (e) => (e.className = s.rotSec));
    const h = el("div", (e) => (e.className = s.rotSecHead));
    h.append(el("h4", (e) => (e.className = s.rotSecTitle), copyText("rot.ed.preview")), el("span", (e) => (e.className = s.rotNote), copyText("rot.ed.previewNote")));
    sec.appendChild(h);
    const p = ed.plan;
    const key = `${p.view}|${JSON.stringify(ed.draft)}`;
    if (p.key !== key) {
      p.key = key;
      const seq = ++p.seq;
      readPlan(this.origin, { draft: ed.draft, view: p.view }).then(
        (r) => {
          if (p.seq !== seq || this.ed !== ed) return;
          p.reply = r;
          this.paint();
        },
        (e: unknown) => console.warn("[rot] 预览推演失败：", e),
      );
    }
    const rows = (p.reply?.rows ?? []).map((r) => {
      const head = el("div", (e) => (e.className = s.rotChartHead));
      head.append(accountAvatarEl(r.account, { size: 16 }), el("span", (e) => (e.className = s.rotWho), accountLabel(r.account)));
      return { account: r.account, head, why: "", alsoUsed: 0 };
    });
    sec.appendChild(
      combinedChart({
        plan: p.reply,
        view: p.view,
        onView: (v) => {
          p.view = v;
          this.paint();
        },
        rows,
      }).root,
    );
    return sec;
  }

  // ─────────────────────────────── 策略

  private strategyList(sr: StrategiesRead): HTMLElement {
    const box = el("div", (e) => (e.className = s.rotListCol));
    const list = el("div", (e) => (e.className = s.rotList));
    list.setAttribute("role", "list");
    const item = (id: string, name: string, tagText: string, desc: string, bad: boolean, side: string, menu: MenuItem[] | null): HTMLElement => {
      const row = el("div", (e) => (e.className = s.rotItem));
      row.setAttribute("role", "listitem");
      row.dataset.rotStrategy = id;
      row.tabIndex = 0;
      const sel = this.strat ?? sr.strategies[0]?.id ?? "@lib";
      if (sel === id) row.dataset.on = "true";
      const main = el("div", (e) => (e.className = s.rotItemMain));
      const l1 = el("div", (e) => (e.className = s.rotItemL1));
      l1.append(el("span", (e) => (e.className = s.rotItemName), name), el("span", (e) => (e.className = s.rotTag), tagText));
      const d = el("div", (e) => (e.className = s.rotItemDesc), desc);
      if (bad) d.dataset.error = "true";
      main.append(l1, d);
      row.append(main, el("span", (e) => (e.className = s.rotItemSide), side));
      if (menu) {
        const more = el("button", (e) => (e.className = s.rotMore));
        more.type = "button";
        more.setAttribute("aria-label", copyText("rot.set.more", { name }));
        more.appendChild(icon("more", "compact"));
        more.addEventListener("click", (ev) => {
          ev.stopPropagation();
          openMenu({ el: more, align: "end" }, menu);
        });
        row.appendChild(more);
      }
      row.addEventListener("click", () => {
        this.strat = id;
        this.tried = null;
        this.paint();
      });
      return row;
    };
    list.appendChild(item("@lib", copyText("rot.st.lib"), copyText("rot.st.libKind"), "", false, "", null));
    for (const x of sr.strategies) {
      list.appendChild(
        item(
          x.id,
          x.name,
          x.builtin ? copyText("rot.st.builtin") : copyText("rot.st.local"),
          x.error ? x.error.text : x.text,
          x.error !== undefined,
          x.usedBy.length > 0 ? copyText("rot.st.usedBy", { n: x.usedBy.length }) : "",
          x.builtin ? null : [{ label: copyText("rot.set.delete"), danger: true, onClick: () => void this.dropStrategy(x) }],
        ),
      );
    }
    const file = document.createElement("input");
    file.className = s.rotFile;
    file.type = "file";
    file.accept = ".rhai";
    file.addEventListener("change", () => void this.addFile(file));
    const add = button({ label: copyText("rot.st.addFile"), size: "compact", onClick: () => file.click() });
    add.dataset.rotAddStrategy = "true";
    box.append(list, add, file, el("div", (e) => (e.className = s.rotNote), copyText("rot.st.addHint")));
    return box;
  }

  private async addFile(input: HTMLInputElement): Promise<void> {
    const f = input.files?.[0];
    input.value = "";
    if (!f) return;
    const id = f.name.replace(/\.rhai$/, "");
    const source = await f.text();
    try {
      const got = await addStrategy(this.origin, id, source);
      if (got.state === "done") {
        toast(copyText("rot.st.added", { id: got.id }), "", { level: "success" });
        this.strat = got.id;
      } else toast(got.error?.text ?? got.said, "", { level: "error" });
    } catch (e) {
      toast(copyText("rot.set.failed", { why: failSaid(copyText("acct.reason.unknown"), e) }), "", { level: "error" });
    }
    await this.reload(true);
  }

  private async dropStrategy(x: StrategyItem): Promise<void> {
    const go = await this.confirm({ title: copyText("rot.st.deleteTitle", { name: x.name }), action: copyText("rot.set.deleteGo"), danger: true, body: copyText("rot.set.deleteBody") });
    if (!go) return;
    await this.run(async () => {
      const got = await deleteStrategy(this.origin, x.id);
      if (got.state === "refused") toast(got.said, "", { level: "error" });
    });
  }

  private strategyDetail(sr: StrategiesRead): HTMLElement {
    const box = el("div", (e) => (e.className = s.rotEditor));
    const id = this.strat ?? sr.strategies[0]?.id ?? "@lib";
    box.dataset.rotStrategyDetail = id;
    if (id === "@lib") {
      const head = el("div", (e) => (e.className = s.rotEdHead));
      head.append(el("span", (e) => (e.className = s.rotDetTitle), copyText("rot.st.lib")), el("span", (e) => (e.className = s.rotTag), copyText("rot.st.libKind")));
      box.append(head, this.source(sr.lib.source, null));
      return box;
    }
    const x = sr.strategies.find((y) => y.id === id);
    if (!x) return box;
    const head = el("div", (e) => (e.className = s.rotEdHead));
    head.append(el("span", (e) => (e.className = s.rotDetTitle), x.name), el("span", (e) => (e.className = s.rotTag), x.builtin ? copyText("rot.st.builtin") : copyText("rot.st.local")), el("span", (e) => (e.className = s.rotSpacer)));
    const tryBtn = button({ label: copyText("rot.st.tryGo"), size: "compact", onClick: () => void this.tryRun(x) });
    tryBtn.dataset.rotTry = "true";
    if (x.error) tryBtn.disabled = true;
    head.appendChild(tryBtn);
    box.appendChild(head);
    const facts = el("div", (e) => (e.className = s.rotFacts));
    const fact = (k: string, v: string, bad = false): void => {
      const dt = el("span", (e) => (e.className = s.rotFactK), k);
      const dd = el("span", (e) => (e.className = s.rotFactV), v);
      if (bad) dd.dataset.error = "true";
      facts.append(dt, dd);
    };
    fact(copyText("rot.st.desc"), x.error ? x.error.text : x.text, x.error !== undefined);
    fact(copyText("rot.st.file"), x.builtin ? copyText("rot.st.fileBuiltin") : (x.path ?? ""));
    fact(copyText("rot.st.params"), declsText(x.params));
    fact(copyText("rot.st.acctParams"), declsText(x.acctParams));
    const names = x.usedBy.map((u) => this.configs?.configs.find((c) => c.id === u)?.name ?? u);
    fact(copyText("rot.st.usedByLabel"), names.length > 0 ? names.join(copyText("kit.text.sep")) : copyText("rot.st.none"));
    box.appendChild(facts);
    // 试跑：选一份配置、按此刻的事实跑一次（不写盘不换号）。
    const tsec = el("div", (e) => (e.className = s.rotSec));
    const th = el("div", (e) => (e.className = s.rotSecHead));
    th.append(el("h4", (e) => (e.className = s.rotSecTitle), copyText("rot.st.tryTitle")), el("span", (e) => (e.className = s.rotNote), copyText("rot.st.tryNote")));
    const cs = this.configs?.configs ?? [];
    const cfg = this.tryCfg ?? x.usedBy[0] ?? this.configs?.defaultConfig ?? "";
    const csel = select({
      label: copyText("rot.st.tryCfg"),
      options: cs.map((c) => ({ value: c.id, label: c.name })),
      value: cfg,
      onChange: (v) => {
        this.tryCfg = v;
        this.tried = null;
        this.paint();
      },
    });
    th.appendChild(csel.el);
    tsec.appendChild(th);
    if (this.tried) tsec.appendChild(this.tryTable(this.tried));
    box.appendChild(tsec);
    box.appendChild(this.source(x.source, x.error?.line ?? null));
    return box;
  }

  private async tryRun(x: StrategyItem): Promise<void> {
    const cfg = this.tryCfg ?? x.usedBy[0] ?? this.configs?.defaultConfig ?? "";
    try {
      this.tried = await tryStrategy(this.origin, { config: cfg, id: x.id });
    } catch (e) {
      toast(copyText("rot.set.failed", { why: failSaid(copyText("acct.reason.unknown"), e) }), "", { level: "error" });
    }
    this.paint();
  }

  private tryTable(t: TryReply): HTMLElement {
    const box = el("div", (e) => (e.className = s.rotTry));
    box.dataset.rotTried = "true";
    if (t.strategyError) box.appendChild(banner("error", t.strategyError.text, [], t.strategyError.msg));
    t.rows.forEach((r, i) => {
      const row = el("div", (e) => (e.className = s.rotTryRow));
      row.dataset.rotTryRow = r.account;
      if (r.p === null) row.dataset.skip = "true";
      row.append(
        el("span", (e) => (e.className = s.rotTryN), r.p === null ? copyText("rot.form.empty") : String(i + 1)),
        accountAvatarEl(r.account, { size: 16 }),
        el("span", (e) => (e.className = s.rotWho), accountLabel(r.account)),
        el("span", (e) => (e.className = s.rotTryWhy), r.p === null ? copyText("rot.row.skip", { why: r.why }) : r.why),
        el("span", (e) => (e.className = s.rotTryP), r.p === null ? "()" : String(r.p)),
      );
      box.appendChild(row);
    });
    const ops = Number(t.ops);
    const max = Number(t.maxOps);
    box.appendChild(el("div", (e) => (e.className = s.rotNote), t.pick ? copyText("rot.st.tryPick", { acct: accountLabel(t.pick), ops, max }) : copyText("rot.st.tryHeld", { ops, max })));
    return box;
  }

  /** 源码（只读、带行号；出错那一行标红）。 */
  private source(text: string, bad: number | null): HTMLElement {
    const sec = el("div", (e) => (e.className = s.rotSec));
    const lines = text.split("\n");
    const h = el("div", (e) => (e.className = s.rotSecHead));
    h.append(el("h4", (e) => (e.className = s.rotSecTitle), copyText("rot.st.source")), el("span", (e) => (e.className = s.rotNote), copyText("rot.st.sourceNote", { n: lines.length })));
    sec.appendChild(h);
    const pre = el("pre", (e) => (e.className = s.rotSource));
    pre.dataset.rotSource = "true";
    lines.forEach((t, i) => {
      const line = el("span", (e) => (e.className = s.rotSrcLine));
      if (bad === i + 1) line.dataset.error = "true";
      line.append(el("span", (e) => (e.className = s.rotSrcN), String(i + 1)), document.createTextNode(`${t}\n`));
      pre.appendChild(line);
    });
    sec.appendChild(pre);
    return sec;
  }
}
