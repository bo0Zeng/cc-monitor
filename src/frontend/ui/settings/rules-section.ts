/**
 * 机器页「轮换」栏：`{machine} 的规则  默认 日常 … [+ 新建规则]` ＋ 一张表（一条规则一行，44 高：勾 · 名称 ［默认］· 摘要 · 在用 · ⋯）。
 *
 * - ⋯：复制 · 改名 · 设为默认 · 复制到 ▸ 别的机器 · 删除（默认那条灰）。「编辑」与点行进编辑器是下一批。
 * - 「在用」可点 ⇒ 行下展开名单（每会话一行，勾上的 / 全部 ⇒ 改用另一条 · 转为本会话；已结束的折着）。
 * - 勾了几条 ⇒ 表头换成批量条：`已选 2 · [复制到 ▸] [删除]`。
 * - 删一条在用的规则 ⇒ 确认框里选那些会话落到哪（转为本会话 · 改为跟随默认）；没人在用 ⇒ 直接删、toast 带撤销。
 *
 * 判定全在那台后端：摘要 · 谁在用 · 名字对不对 · 重名加号 · 版本冲突 · 删默认拒（`rotation-*` 那几条）。会话标题取
 * `history-list`（那台的会话清单）。这里只排版、只认最后一趟回答；那台规则一变推 `changed {rotation_rules}`，这一栏自己重读。
 */
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import {
  deleteRules,
  readRules,
  renameRule,
  saveRule,
  setDefaultRule,
  writeSessionRotation,
  type RuleRow,
  type RuleUserDoing,
  type RulesRead,
  type SessionRotationWrite,
} from "../quota-reads";
import { fetchList } from "../history-list-reads";
import { readRemoteConfig } from "../remote-config";
import { bindEvents } from "../events";
import { confirmDialog, type ConfirmFn, type ConfirmSpec } from "../kit/dialog";
import { button, setDisabled } from "../kit/button";
import { banner } from "../kit/banner";
import { detailOf, failSaid } from "../kit/detail";
import { foldCaret } from "../kit/fold";
import { skeletonRows } from "../kit/skeleton";
import { field } from "../kit/field";
import { openMenu, type MenuItem } from "../kit/menu";
import { closePopover, openPopover } from "../kit/popover";
import { select } from "../kit/select";
import { tag } from "../kit/badge";
import { statusDot, type DotState } from "../kit/status-dot";
import { activityFace } from "../session-status";
import { attachTooltip } from "../kit/tooltip";
import { toast, failToast } from "../kit/toast";
import { copyText } from "../copy-table";
import { machineName } from "../ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import type { CellError } from "../generated/CellError";
import { RuleEditor } from "./rule-editor";
import s from "./rules-section.module.css";
import { MachineTimeline } from "./rules-timeline";

/** 推来的帧合并成一次重读的间隔。 */
const PUSH_COALESCE_MS = 300;
/** 规则多过这么多条 ⇒ 段头下出筛选框。 */
const FILTER_OVER = 12;

/** 一个会话在名单里的标题（`history-list` 那一行）；状态取规则表那一格（`users.doing`，后端判）。 */
interface Who {
  label: string;
}

/** 名单里一个会话的点与那一句（与主窗口同一套：字照抄核心写的，点的颜色按语气、已结束画空心）。 */
function userFace(d: RuleUserDoing | null): { dot: DotState; word: string } {
  // 后端对名单里每个 sid 都给一格（解码时查过）；万一没有 ⇒ 不画状态，不替它判。
  if (d === null) return { dot: "unknown", word: "" };
  return { dot: d.state === "ended" ? "ended" : activityFace(d.tone).dot, word: d.text };
}

/** 别处要这一栏开某条规则的编辑器（面板「编辑规则…」带目的地落过来）：按机器记一条，这一栏读到那台的表时开。 */
const wanted = new Map<Origin, string>();
const wantListeners = new Set<() => void>();

/** 开 `origin` 那台规则 `id` 的编辑器（这一栏还没读到表 ⇒ 读到时开）。 */
export function openRuleEditor(origin: Origin, id: string): void {
  wanted.set(origin, id);
  for (const f of wantListeners) f();
}

export interface RulesSectionOptions {
  /** 注入缝（判据换掉确认框）。 */
  confirm?: ConfirmFn;
}

export class RulesSection {
  /** 规则列表下面那条时间轴（这台全部号）。 */
  private readonly tl = new MachineTimeline();
  readonly element: HTMLElement;
  private readonly titleEl: HTMLElement;
  private readonly subEl: HTMLElement;
  private readonly newBtn: HTMLButtonElement;
  private readonly filterSlot: HTMLElement;
  private readonly body: HTMLElement;
  private readonly confirm: ConfirmFn;
  private origin: Origin = LOCAL_ORIGIN;
  private seq = 0;
  private read: RulesRead | null = null;
  /** 勾着的规则（批量条）。 */
  private readonly picked = new Set<string>();
  /** 「在用」展开着的规则。 */
  private readonly open = new Set<string>();
  /** 展开名单里勾着的会话（按规则分）。 */
  private readonly pickedSids = new Map<string, Set<string>>();
  /** 「已结束 N」拉开着的规则。 */
  private readonly endedOpen = new Set<string>();
  /** 正在行内改名的那一条（`fresh` ＝ 刚复制出来的）与它的报错。 */
  private renaming: { id: string; error: string | null } | null = null;
  private filter = "";
  /** 这台会话的标题（`history-list`；切机器 / 重读清单时作废）。 */
  private who: Promise<Map<string, Who>> | null = null;
  private whoGot: Map<string, Who> | null = null;
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private subscribed = false;
  private loaded = false;
  /** 开着的那条规则的编辑器（同一栏里的子页）。 */
  private editor: RuleEditor | null = null;

  constructor(opts: RulesSectionOptions = {}) {
    this.confirm = opts.confirm ?? confirmDialog;
    const root = document.createElement("div");
    root.className = s.rules;
    root.dataset.rulesPage = "true";
    const head = document.createElement("div");
    head.className = s.rulesHead;
    this.titleEl = document.createElement("h3");
    this.titleEl.className = s.rulesTitle;
    this.subEl = document.createElement("span");
    this.subEl.className = s.rulesSub;
    this.newBtn = button({
      label: copyText("rot.list.new"),
      icon: "plus",
      size: "compact",
      onClick: () => this.openNew(this.newBtn),
    });
    this.newBtn.dataset.rulesNew = "true";
    const right = document.createElement("div");
    right.className = s.rulesActions;
    right.appendChild(this.newBtn);
    head.append(this.titleEl, this.subEl, right);
    this.filterSlot = document.createElement("div");
    this.body = document.createElement("div");
    this.body.className = s.rulesBody;
    root.append(head, this.filterSlot, this.body);
    root.addEventListener("keydown", (ev) => {
      if (
        ev.key === "Escape" &&
        this.renaming &&
        (ev.target as HTMLElement).dataset.rulesRename
      ) {
        ev.stopPropagation();
        this.renaming = null;
        this.paint();
      }
    });
    this.element = root;
    wantListeners.add(() => {
      if (this.element.isConnected) this.takeWanted();
    });
    subscribeMachine((origin) => {
      if (!this.loaded || origin === this.origin) return;
      this.origin = origin;
      this.reset();
      void this.reload();
    });
  }

  /** 宿主在这一栏第一次可见时调（重开设置再调一次）。 */
  loadNow(): void {
    this.loaded = true;
    this.origin = getCurrentMachine();
    this.reset();
    void this.reload();
    void this.subscribe();
  }

  private reset(): void {
    this.closeEditor();
    this.picked.clear();
    this.open.clear();
    this.pickedSids.clear();
    this.endedOpen.clear();
    this.renaming = null;
    this.filter = "";
    this.who = null;
    this.whoGot = null;
  }

  /** 每台订 `changed/rotation_rules` ＋ `changed/rotation`：合并成一次重读（只重读此刻在看的那台）。 */
  private async subscribe(): Promise<void> {
    if (this.subscribed) return;
    this.subscribed = true;
    const hosts = await readRemoteConfig().then(
      (c) => c.hosts.map((h) => h.label),
      () => [] as string[],
    );
    const origins: Origin[] = [LOCAL_ORIGIN, ...hosts];
    // 规则表 / 默认指向变了 · 某个会话的来源变了（`changed/rotation_rules` · `changed/rotation`）：一来就合并成一次重读。
    const pushed = (origin: Origin): void => {
      if (origin !== this.origin) return;
      if (this.pushTimer !== null) clearTimeout(this.pushTimer);
      // 调度：合批 —— 那台推来的规则 / 会话来源变更：300ms 内几帧合成一次重读
      this.pushTimer = setTimeout(() => {
        this.pushTimer = null;
        void this.reload(true);
      }, PUSH_COALESCE_MS);
    };
    try {
      await bindEvents(
        { onLine: () => {}, onSessionEnded: () => {}, onChanged: pushed },
        { changed: origins.flatMap((origin) => (["rotation_rules", "rotation"] as const).map((topic) => ({ origin, topic }))) },
      );
    } catch (e) {
      console.warn("[rules] 订不上规则推送：", e);
    }
  }

  /** `quiet`：推来的 / 动作之后的那一趟不画骨架（原位换）。 */
  private async reload(quiet = false): Promise<void> {
    const my = ++this.seq;
    const origin = this.origin;
    this.titleEl.textContent = copyText("rot.list.title", {
      machine: machineName(origin),
    });
    if (!quiet || this.read === null) {
      this.read = null;
      this.subEl.textContent = "";
      this.body.replaceChildren(skeletonRows(3));
      this.body.setAttribute("aria-busy", "true");
    }
    let got: RulesRead | null = null;
    let failed: unknown = null;
    try {
      got = await readRules(origin);
    } catch (e) {
      failed = e;
    }
    if (my !== this.seq) return;
    this.body.removeAttribute("aria-busy");
    if (got === null) {
      this.read = null;
      this.subEl.textContent = "";
      setDisabled(
        this.newBtn,
        copyText("rot.list.unreadable", { machine: machineName(origin) }),
      );
      const retry = button({
        label: copyText("rot.list.retry"),
        size: "compact",
        onClick: () => void this.reload(),
      });
      this.body.replaceChildren(
        banner(
          "warn",
          failSaid(copyText("rot.list.unreadable", { machine: machineName(origin) }), failed),
          [retry],
          detailOf(failed),
        ),
      );
      return;
    }
    this.read = got;
    this.tl.load(origin);
    if (this.editor) {
      const id = this.editor.id;
      this.editor.update(got.rules.find((x) => x.id === id) ?? null);
    }
    this.takeWanted();
    // 不在了的规则 / 会话不留勾。
    const ids = new Set(got.rules.map((r) => r.id));
    for (const set of [this.picked, this.open, this.endedOpen])
      for (const id of [...set]) if (!ids.has(id)) set.delete(id);
    if (this.renaming && !ids.has(this.renaming.id)) this.renaming = null;
    // 名单里有没见过的会话 ⇒ 标题重取一次。
    if (
      this.whoGot &&
      got.rules.some((r) =>
        [...r.users.sids, ...r.users.endedSids].some(
          (sid) => !this.whoGot!.has(sid),
        ),
      )
    )
      this.who = null;
    this.paint();
    if (this.open.size > 0) void this.ensureWho();
  }

  /** 会话标题（`history-list` 一次给这台全部会话）；问不到 ⇒ 空表（名单写 sid 前 8 位）。 */
  private ensureWho(): Promise<Map<string, Who>> {
    if (this.who === null) {
      const origin = this.origin;
      this.who = fetchList(isLocalOrigin(origin) ? undefined : origin, {}).then(
        (l) => new Map(l.rows.map((r) => [r.sessionId, { label: r.label }])),
        (e: unknown) => {
          console.warn(`[rules] history-list [${origin}] 失败：`, e);
          return new Map<string, Who>();
        },
      );
      void this.who.then((m) => {
        if (origin !== this.origin) return;
        this.whoGot = m;
        if (this.open.size > 0) this.paint();
      });
    }
    return this.who;
  }

  private whoOf(sid: string): Who {
    return this.whoGot?.get(sid) ?? { label: sid.slice(0, 8) };
  }

  // ───────────────────────────── 编辑器 ─────────────────────────────

  /** 别处要开的那条（这一台、表里有）⇒ 开它的编辑器。 */
  private takeWanted(): void {
    const id = wanted.get(this.origin);
    if (id === undefined || !this.read) return;
    wanted.delete(this.origin);
    if (this.read.rules.some((x) => x.id === id)) this.edit(id);
  }

  /** 进一条规则的编辑器（点行 · ⋯「编辑」· 带目的地）。 */
  private edit(id: string): void {
    const rule = this.read?.rules.find((x) => x.id === id);
    if (!rule) return;
    this.closeEditor();
    this.editor = new RuleEditor(
      {
        back: () => {
          this.closeEditor();
          this.paint();
          this.body.querySelector<HTMLElement>(`[data-rule="${id}"]`)?.focus();
        },
        reload: () => void this.reload(true),
        more: (rule, anchor) => {
          const r = this.read;
          if (r)
            openMenu(
              { el: anchor, align: "end" },
              this.menu(r, rule, anchor).filter((it) => it.id !== "edit"),
            );
        },
        users: (rule) =>
          this.read
            ? this.users(this.read, rule)
            : document.createElement("div"),
      },
      this.origin,
      rule,
    );
    this.paint();
  }

  private closeEditor(): void {
    this.editor?.dispose();
    this.editor = null;
  }

  // ───────────────────────────── 画 ─────────────────────────────

  private paint(): void {
    const r = this.read;
    if (!r) return;
    this.element.dataset.editing = String(this.editor !== null);
    if (this.editor) {
      // 编辑器是同一栏里的子页：段头 · 筛选框收起，主区只放它（名单勾选之类重画它自己）。
      this.filterSlot.replaceChildren();
      this.editor.refresh();
      if (this.body.firstElementChild !== this.editor.element)
        this.body.replaceChildren(this.editor.element);
      return;
    }
    setDisabled(this.newBtn, null);
    const def = r.rules.find((x) => x.id === r.defaultRule) ?? null;
    this.subEl.textContent = def
      ? copyText("rot.list.defaultIs", { name: def.name })
      : "";
    const out: HTMLElement[] = [];
    if (r.state === "unreadable")
      out.push(
        banner(
          "warn",
          r.reason ??
            copyText("rot.list.unreadable", {
              machine: machineName(this.origin),
            }),
          [],
          r.detail ?? "",
        ),
      );
    this.paintFilter(r);
    const q = this.filter.trim().toLowerCase();
    const shown = q
      ? r.rules.filter((x) => x.name.toLowerCase().includes(q))
      : r.rules;
    out.push(this.table(r, shown));
    if (r.rules.length <= 1) {
      const hint = document.createElement("div");
      hint.className = s.rulesEmpty;
      hint.dataset.rulesEmpty = "true";
      const t = document.createElement("span");
      t.textContent = copyText("rot.list.emptyHint");
      const add = button({
        label: copyText("rot.list.new"),
        icon: "plus",
        size: "compact",
        onClick: () => this.openNew(add),
      });
      hint.append(t, add);
      out.push(hint);
    }
    out.push(this.tl.element);
    const focusRename = this.renaming !== null;
    this.body.replaceChildren(...out);
    if (focusRename) {
      const inp = this.body.querySelector<HTMLInputElement>(
        "[data-rules-rename]",
      );
      if (inp && document.activeElement !== inp) {
        inp.focus();
        inp.select();
      }
    }
  }

  private paintFilter(r: RulesRead): void {
    if (r.rules.length <= FILTER_OVER) {
      this.filterSlot.replaceChildren();
      return;
    }
    if (this.filterSlot.querySelector("input")) return;
    const box = document.createElement("input");
    box.type = "search";
    box.className = s.rulesFilter;
    box.placeholder = copyText("rot.list.filter");
    box.setAttribute("aria-label", copyText("rot.list.filter"));
    box.value = this.filter;
    box.addEventListener("input", () => {
      this.filter = box.value;
      this.paint();
    });
    this.filterSlot.replaceChildren(box);
  }

  private table(r: RulesRead, shown: RuleRow[]): HTMLElement {
    const t = document.createElement("div");
    t.className = s.rulesTable;
    t.setAttribute("role", "table");
    t.appendChild(
      this.picked.size > 0 ? this.batchBar(r) : this.headRow(shown),
    );
    for (const rule of shown) {
      t.appendChild(this.row(r, rule));
      if (this.open.has(rule.id)) t.appendChild(this.users(r, rule));
    }
    return t;
  }

  private headRow(shown: RuleRow[]): HTMLElement {
    const h = document.createElement("div");
    h.className = s.rulesHeadRow;
    h.setAttribute("role", "row");
    const all = document.createElement("input");
    all.type = "checkbox";
    all.setAttribute("aria-label", copyText("rot.list.pickAll"));
    all.addEventListener("change", () => {
      for (const x of shown) this.picked.add(x.id);
      this.paint();
    });
    const cell = (text: string): HTMLElement => {
      const c = document.createElement("span");
      c.setAttribute("role", "columnheader");
      c.textContent = text;
      return c;
    };
    h.append(
      all,
      cell(copyText("rot.list.colName")),
      cell(copyText("rot.list.colSum")),
      cell(copyText("rot.list.colUse")),
      document.createElement("span"),
    );
    return h;
  }

  /** 勾了几条：`已选 2 · [复制到 ▸] [删除]`（默认那条在内时删除灰）。 */
  private batchBar(r: RulesRead): HTMLElement {
    const bar = document.createElement("div");
    bar.className = s.rulesBatch;
    bar.dataset.rulesBatch = "true";
    const all = document.createElement("input");
    all.type = "checkbox";
    all.checked = true;
    all.setAttribute("aria-label", copyText("rot.list.pickAll"));
    all.addEventListener("change", () => {
      this.picked.clear();
      this.paint();
    });
    const n = document.createElement("span");
    n.className = s.rulesBatchN;
    n.textContent = copyText("rot.batch.sel", { n: this.picked.size });
    const rules = r.rules.filter((x) => this.picked.has(x.id));
    const copyTo = button({
      label: copyText("rot.batch.copyTo"),
      icon: "caretRight",
      iconAfter: true,
      size: "compact",
    });
    copyTo.addEventListener("click", () => void this.copyToMenu(copyTo, rules));
    const del = button({
      label: copyText("rot.batch.delete"),
      size: "compact",
      onClick: () => void this.remove(r, rules),
    });
    if (rules.some((x) => x.isDefault))
      setDisabled(del, copyText("rot.batch.hasDefault"));
    const sp = document.createElement("span");
    sp.className = s.rulesSp;
    bar.append(all, n, sp, copyTo, del);
    return bar;
  }

  private row(r: RulesRead, rule: RuleRow): HTMLElement {
    const row = document.createElement("div");
    row.className = s.rulesRow;
    row.setAttribute("role", "row");
    row.dataset.rule = rule.id;
    row.dataset.anchor = `rule:${rule.id}`;
    const pick = document.createElement("input");
    pick.type = "checkbox";
    pick.checked = this.picked.has(rule.id);
    pick.setAttribute(
      "aria-label",
      copyText("rot.list.pick", { name: rule.name }),
    );
    pick.addEventListener("change", () => {
      if (pick.checked) this.picked.add(rule.id);
      else this.picked.delete(rule.id);
      this.paint();
    });
    const name = document.createElement("span");
    name.className = s.rulesName;
    name.setAttribute("role", "cell");
    if (this.renaming?.id === rule.id) name.appendChild(this.renameBox(rule));
    else {
      const t = document.createElement("span");
      t.className = s.rulesNameText;
      t.textContent = rule.name;
      name.appendChild(t);
      if (rule.isDefault) name.appendChild(tag(copyText("rot.src.tagDefault")));
    }
    const sum = document.createElement("span");
    sum.className = s.rulesSum;
    sum.setAttribute("role", "cell");
    sum.textContent = rule.summary;
    attachTooltip(sum, rule.summary);
    const use = document.createElement("span");
    use.className = s.rulesUse;
    use.setAttribute("role", "cell");
    const total = rule.users.live + rule.users.ended;
    if (total > 0) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = s.rulesUseBtn;
      b.dataset.rulesUse = rule.id;
      b.setAttribute("aria-expanded", String(this.open.has(rule.id)));
      b.textContent =
        rule.users.live > 0
          ? copyText("rot.list.useN", { n: rule.users.live })
          : copyText("rot.list.useNone");
      b.addEventListener("click", () => {
        if (this.open.has(rule.id)) this.open.delete(rule.id);
        else {
          this.open.add(rule.id);
          void this.ensureWho();
        }
        this.paint();
      });
      use.appendChild(b);
    } else use.textContent = copyText("rot.list.useNone");
    const more = button({
      label: copyText("rot.list.more", { name: rule.name }),
      kind: "icon",
      icon: "more",
      size: "compact",
      hint: copyText("rot.list.more", { name: rule.name }),
    });
    more.dataset.rulesMore = rule.id;
    more.addEventListener("click", () =>
      openMenu({ el: more, align: "end" }, this.menu(r, rule, more)),
    );
    row.append(pick, name, sum, use, more);
    // 点行 ＝ 进编辑器（勾 · 改名框 · 在用 · ⋯ 各管各的）；Enter 同。
    row.tabIndex = 0;
    row.addEventListener("click", (ev) => {
      if ((ev.target as HTMLElement).closest("input, button")) return;
      this.edit(rule.id);
    });
    row.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && ev.target === row && !ev.isComposing) {
        ev.preventDefault();
        this.edit(rule.id);
      }
    });
    return row;
  }

  /** ⋯：复制 · 改名 · 设为默认 · 复制到 ▸ · 分隔 · 删除（默认那条灰，第二行说为什么）。 */
  private menu(r: RulesRead, rule: RuleRow, anchor: HTMLElement): MenuItem[] {
    const items: MenuItem[] = [
      {
        id: "edit",
        label: copyText("rot.act.edit"),
        onClick: () => this.edit(rule.id),
      },
      {
        id: "copy",
        label: copyText("rot.act.copy"),
        onClick: () => void this.copy(rule),
      },
      {
        id: "rename",
        label: copyText("rot.act.rename"),
        onClick: () => this.startRename(rule),
      },
      rule.isDefault
        ? {
            id: "setDefault",
            label: copyText("rot.act.setDefault"),
            checked: true,
            enabled: false,
          }
        : {
            id: "setDefault",
            label: copyText("rot.act.setDefault"),
            onClick: () => void this.makeDefault(r, rule),
          },
      {
        id: "copyTo",
        label: copyText("rot.act.copyTo"),
        onClick: () => void this.copyToMenu(anchor, [rule]),
      },
      { label: "", divider: true },
      rule.isDefault
        ? {
            id: "delete",
            label: copyText("rot.act.delete"),
            enabled: false,
            why: copyText("rot.act.defaultNoDelete"),
          }
        : {
            id: "delete",
            label: copyText("rot.act.delete"),
            danger: true,
            onClick: () => void this.remove(r, [rule]),
          },
    ];
    return items;
  }

  /** 行下展开的在用名单：勾 · 点 · 标题 · 状态；已结束的折着；脚上［改用 规则 ▾］［转为本会话］。 */
  private users(r: RulesRead, rule: RuleRow): HTMLElement {
    const box = document.createElement("div");
    box.className = s.rulesUsers;
    box.dataset.rulesUsers = rule.id;
    let picked = this.pickedSids.get(rule.id);
    if (!picked) {
      picked = new Set();
      this.pickedSids.set(rule.id, picked);
    }
    const all = [...rule.users.sids, ...rule.users.endedSids];
    for (const sid of [...picked]) if (!all.includes(sid)) picked.delete(sid);
    const line = (sid: string): HTMLElement => {
      const w = this.whoOf(sid);
      const l = document.createElement("label");
      l.className = s.rulesUser;
      l.dataset.sid = sid;
      const c = document.createElement("input");
      c.type = "checkbox";
      c.checked = picked.has(sid);
      c.addEventListener("change", () => {
        if (c.checked) picked.add(sid);
        else picked.delete(sid);
        this.paint();
      });
      const { dot, word } = userFace(rule.users.doing[sid] ?? null);
      const t = document.createElement("span");
      t.className = s.rulesUserName;
      t.textContent = w.label;
      const st = document.createElement("span");
      st.className = s.rulesUserState;
      st.textContent = word;
      l.append(c, statusDot(dot, word, "compact"), t, st);
      return l;
    };
    for (const sid of rule.users.sids) box.appendChild(line(sid));
    if (rule.users.endedSids.length > 0) {
      const opened = this.endedOpen.has(rule.id);
      const fold = button({
        label: copyText("sessionState.rotUsers.ended", {
          n: rule.users.endedSids.length,
        }),
        kind: "ghost",
        size: "compact",
      });
      fold.prepend(foldCaret());
      fold.setAttribute("aria-expanded", String(opened));
      fold.dataset.rulesEnded = rule.id;
      fold.addEventListener("click", () => {
        if (opened) this.endedOpen.delete(rule.id);
        else this.endedOpen.add(rule.id);
        this.paint();
      });
      box.appendChild(fold);
      if (opened)
        for (const sid of rule.users.endedSids)
          box.appendChild(line(sid));
    }
    const foot = document.createElement("div");
    foot.className = s.rulesUsersFoot;
    const some = picked.size > 0;
    const target = some ? [...picked] : all;
    if (some) {
      const n = document.createElement("span");
      n.textContent = copyText("rot.list.picked", { n: picked.size });
      foot.appendChild(n);
    }
    const move = button({
      label: some
        ? copyText("rot.batch.moveTo")
        : copyText("rot.batch.moveAll"),
      icon: "caretDown",
      iconAfter: true,
      size: "compact",
    });
    move.dataset.rulesMove = rule.id;
    move.addEventListener("click", () => {
      const def = r.rules.find((x) => x.id === r.defaultRule);
      const items: MenuItem[] = [];
      if (!(
        rule.isDefault &&
        rule.users.follow === rule.users.live &&
        rule.users.ended === 0
      )) {
        items.push({
          id: "follow",
          label: copyText("rot.src.follow"),
          detail: def?.name,
          onClick: () =>
            void this.apply(target, "follow", copyText("rot.src.follow")),
        });
      }
      for (const x of r.rules) {
        if (x.id === rule.id) continue;
        items.push({
          id: `rule:${x.id}`,
          label: x.name,
          detail: x.summary,
          onClick: () =>
            void this.apply(
              target,
              { rule: x.id },
              copyText("rot.src.rule", { name: x.name }),
            ),
        });
      }
      if (items.length === 0)
        items.push({ label: copyText("rot.src.noRules"), enabled: false });
      openMenu({ el: move, align: "start" }, items);
    });
    const detach = button({
      label: some
        ? copyText("rot.batch.detach")
        : copyText("rot.batch.detachAll"),
      size: "compact",
      onClick: () =>
        void this.apply(target, "detach", copyText("rot.src.custom")),
    });
    detach.dataset.rulesDetach = rule.id;
    foot.append(move, detach);
    box.appendChild(foot);
    return box;
  }

  // ───────────────────────────── 做 ─────────────────────────────

  /** 一批会话改来源：回逐会话结局，成了几个照实说。 */
  private async apply(
    sids: string[],
    to: SessionRotationWrite,
    src: string,
  ): Promise<void> {
    const origin = this.origin;
    try {
      const got = await writeSessionRotation(origin, sids, to);
      const done = Object.values(got).filter((o) => o.state === "done").length;
      toast(copyText("rot.done.batch", { src, n: done }), "", {
        level: "success",
      });
      this.pickedSids.clear();
    } catch (e) {
      failToast(copyText("rot.fail.apply", { src }), e, { level: "error" });
    }
    await this.reload(true);
  }

  /** 新建：小浮层 `名称 [ ]` · `从 [默认 日常 ▾]`（可选空白）·［取消］［建］。 */
  private openNew(anchor: HTMLElement): void {
    const r = this.read;
    if (!r) return;
    const origin = this.origin;
    const root = document.createElement("div");
    root.className = s.rulesNew;
    root.dataset.rulesNewForm = "true";
    const title = document.createElement("div");
    title.className = s.rulesNewTitle;
    title.textContent = copyText("rot.new.title");
    const name = field({
      label: copyText("rot.save.name"),
      noteOnDemand: true,
    });
    const def = r.rules.find((x) => x.id === r.defaultRule);
    let from = def?.id ?? "blank";
    const fromSel = select({
      label: copyText("rot.new.from"),
      options: [
        ...r.rules.map((x) => ({
          value: x.id,
          label: x.name,
          note: x.isDefault ? copyText("rot.src.tagDefault") : undefined,
        })),
        { value: "blank", label: copyText("rot.new.blank") },
      ],
      value: from,
      onChange: (v) => {
        from = v;
      },
    });
    const fromRow = document.createElement("div");
    fromRow.className = s.rulesNewRow;
    const fromLabel = document.createElement("span");
    fromLabel.textContent = copyText("rot.new.from");
    fromRow.append(fromLabel, fromSel.el);
    const go = async (): Promise<void> => {
      name.setError(null);
      let got;
      try {
        got = await saveRule(origin, { name: name.input.value, from });
      } catch (e) {
        console.warn("[rules] rotation-rule-save 失败：", e);
        name.setError(copyText("rot.save.failed"));
        return;
      }
      if (got.state === "saved") {
        closePopover();
        await this.reload(true);
        return;
      }
      name.setError(
        got.state === "refused"
          ? nameError(got.errors)
          : copyText("rot.fail.conflict"),
      );
    };
    name.input.addEventListener("keydown", (e) => {
      const ev = e as KeyboardEvent;
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        void go();
      }
    });
    const foot = document.createElement("div");
    foot.className = s.rulesNewFoot;
    const ok = button({
      label: copyText("rot.new.ok"),
      kind: "primary",
      size: "compact",
      onClick: () => void go(),
    });
    ok.dataset.rulesNewOk = "true";
    foot.append(
      button({
        label: copyText("rot.cap.cancel"),
        kind: "ghost",
        size: "compact",
        onClick: () => closePopover(),
      }),
      ok,
    );
    root.append(title, name.root, fromRow, foot);
    openPopover(anchor, root, {
      label: copyText("rot.new.title"),
      align: "end",
    });
    name.input.focus();
  }

  /** 复制：直接出一条 `日常 副本`（重名后端加号），名称进入就地编辑态。 */
  private async copy(rule: RuleRow): Promise<void> {
    try {
      const got = await saveRule(this.origin, {
        name: copyText("rot.act.copySuffix", { name: rule.name }),
        from: rule.id,
        dedupe: true,
      });
      if (got.state !== "saved")
        throw new Error(
          got.state === "refused"
            ? nameError(got.errors)
            : copyText("rot.fail.conflict"),
        );
      this.renaming = { id: got.rule.id, error: null };
    } catch (e) {
      failToast(copyText("rot.fail.copy", { name: rule.name }), e, {
        level: "error",
      });
    }
    await this.reload(true);
  }

  private startRename(rule: RuleRow): void {
    this.renaming = { id: rule.id, error: null };
    this.paint();
  }

  /** 行内改名：Enter / 失焦存 · Esc 取消；重名 / 空 / 超长框红 ＋ 红字（后端判）。 */
  private renameBox(rule: RuleRow): HTMLElement {
    const wrap = document.createElement("span");
    wrap.className = s.rulesRename;
    const inp = document.createElement("input");
    inp.type = "text";
    inp.className = s.rulesRenameInput;
    inp.value = rule.name;
    inp.dataset.rulesRename = rule.id;
    inp.setAttribute("aria-label", copyText("rot.act.rename"));
    const err = this.renaming?.error ?? null;
    if (err) {
      inp.dataset.error = "true";
      inp.setAttribute("aria-invalid", "true");
    }
    let done = false;
    const commit = async (): Promise<void> => {
      if (done) return;
      done = true;
      if (inp.value === rule.name) {
        this.renaming = null;
        this.paint();
        return;
      }
      try {
        const got = await renameRule(this.origin, {
          id: rule.id,
          name: inp.value,
          ifRev: rule.rev,
        });
        if (got.state === "saved") this.renaming = null;
        else
          this.renaming = {
            id: rule.id,
            error:
              got.state === "refused"
                ? nameError(got.errors)
                : copyText("rot.fail.conflict"),
          };
      } catch (e) {
        console.warn("[rules] rotation-rule-rename 失败：", e);
        this.renaming = { id: rule.id, error: copyText("rot.save.failed") };
      }
      await this.reload(true);
    };
    inp.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        void commit();
      }
    });
    inp.addEventListener("blur", () => {
      if (this.renaming?.id === rule.id) void commit();
    });
    wrap.appendChild(inp);
    if (err) {
      const e = document.createElement("span");
      e.className = s.rulesRenameErr;
      e.textContent = err;
      wrap.appendChild(e);
    }
    return wrap;
  }

  /** 设为默认：即生效；toast 带撤销（设回原来那条）。 */
  private async makeDefault(r: RulesRead, rule: RuleRow): Promise<void> {
    const was = r.defaultRule;
    const origin = this.origin;
    try {
      const got = await setDefaultRule(origin, rule.id);
      toast(
        copyText("rot.done.setDefault", { name: rule.name, n: got.followers }),
        "",
        {
          level: "success",
          action: {
            label: copyText("kit.toast.undo"),
            run: () =>
              void setDefaultRule(origin, was)
                .catch((e: unknown) =>
                  failToast(
                    copyText("rot.fail.setDefault", { name: rule.name }),
                    e,
                    { level: "error" },
                  ),
                )
                .then(() => this.reload(true)),
          },
        },
      );
    } catch (e) {
      failToast(copyText("rot.fail.setDefault", { name: rule.name }), e, {
        level: "error",
      });
    }
    await this.reload(true);
  }

  /** 复制到 ▸ 别的机器：那台后端存一条同名（重名它加号），回它没有的号。 */
  private async copyToMenu(
    anchor: HTMLElement,
    rules: RuleRow[],
  ): Promise<void> {
    const hosts = await readRemoteConfig().then(
      (c) => c.hosts.map((h) => h.label),
      () => [] as string[],
    );
    const here = this.origin;
    const others = [LOCAL_ORIGIN, ...hosts].filter((o) => o !== here);
    const items: MenuItem[] =
      others.length === 0
        ? [{ label: copyText("rot.act.noMachines"), enabled: false }]
        : others.map((o) => ({
            id: `to:${o}`,
            label: machineName(o),
            onClick: () => void this.copyTo(o, rules),
          }));
    openMenu({ el: anchor, align: "end" }, items);
  }

  private async copyTo(to: Origin, rules: RuleRow[]): Promise<void> {
    const machine = machineName(to);
    for (const rule of rules) {
      try {
        const got = await saveRule(to, {
          name: rule.name,
          rotation: rule.rotation,
          dedupe: true,
        });
        if (got.state !== "saved")
          throw new Error(
            got.state === "refused"
              ? cellsText(got.errors)
              : copyText("rot.fail.conflict"),
          );
        const miss = got.rule.missing;
        toast(
          miss.length > 0
            ? copyText("rot.done.copiedToMissing", {
                machine,
                name: got.rule.name,
                n: miss.length,
                list: miss.join(copyText("kit.text.sep")),
              })
            : copyText("rot.done.copiedTo", { machine, name: got.rule.name }),
          "",
          { level: "success" },
        );
      } catch (e) {
        failToast(
          copyText("rot.fail.copyTo", { machine, name: rule.name }),
          e,
          { level: "error" },
        );
      }
    }
  }

  /**
   * 删：没人在用（活着的 · 已结束的都没有）⇒ 不问、直接删，toast 带撤销（照原样再存一条）；
   * 有人在用 ⇒ 确认框：改动 `在用 N 会话（…）` ＋ 一组单选（转为本会话 · 改为跟随默认）· 之后 `已结束的 N 会话同样处理`。
   */
  private async remove(r: RulesRead, rules: RuleRow[]): Promise<void> {
    if (rules.some((x) => x.isDefault)) return;
    const origin = this.origin;
    const live = rules.flatMap((x) => x.users.sids);
    const ended = rules.reduce((n, x) => n + x.users.endedSids.length, 0);
    let then: "custom" | "follow" = "custom";
    if (live.length + ended > 0) {
      const who = await this.ensureWho();
      const def = r.rules.find((x) => x.id === r.defaultRule);
      const names = live.map((sid) => who.get(sid)?.label ?? sid.slice(0, 8));
      const rows: ConfirmSpec["rows"] = [
        {
          label: copyText("rot.del.change"),
          items: [
            copyText("rot.del.inUse", {
              n: live.length,
              list: names.join(copyText("kit.text.sep")),
            }),
          ],
          choice: {
            name: "rules-delete-then",
            value: then,
            options: [
              { value: "custom", label: copyText("rot.del.toCustom") },
              {
                value: "follow",
                label: copyText("rot.del.toFollow", { name: def?.name ?? "" }),
              },
            ],
            onPick: (v) => {
              then = v === "follow" ? "follow" : "custom";
            },
          },
        },
      ];
      if (ended > 0)
        rows.push({
          label: copyText("rot.del.after"),
          items: [copyText("sessionState.rotDelete.ended", { n: ended })],
        });
      const yes = await this.confirm({
        title:
          rules.length === 1
            ? copyText("rot.del.title", { name: rules[0].name })
            : copyText("rot.del.titleN", { n: rules.length }),
        action: copyText("rot.del.ok"),
        danger: true,
        rows,
      });
      if (!yes) return;
    }
    try {
      await deleteRules(
        origin,
        rules.map((x) => x.id),
        then,
      );
      for (const x of rules) this.picked.delete(x.id);
      const quiet = live.length + ended === 0;
      toast(
        rules.length === 1
          ? copyText("rot.done.deleted", { name: rules[0].name })
          : copyText("rot.done.deletedN", { n: rules.length }),
        "",
        {
          level: "success",
          // 没人在用的才能原样撤回（在用的那些会话已经挪了）。
          ...(quiet
            ? {
                action: {
                  label: copyText("kit.toast.undo"),
                  run: () => {
                    void Promise.all(
                      rules.map((x) =>
                        saveRule(origin, {
                          name: x.name,
                          rotation: x.rotation,
                        }),
                      ),
                    )
                      .catch((e: unknown) =>
                        failToast(
                          copyText("rot.fail.delete", { name: rules[0].name }),
                          e,
                          { level: "error" },
                        ),
                      )
                      .then(() => this.reload(true));
                  },
                },
              }
            : {}),
        },
      );
    } catch (e) {
      failToast(
        copyText("rot.fail.delete", {
          name: rules.map((x) => x.name).join(copyText("kit.text.sep")),
        }),
        e,
        { level: "error" },
      );
    }
    await this.reload(true);
  }
}

/** 名称那一格的错照后端短码写（空 · 重名 · 超长）。 */
function nameError(errors: CellError[]): string {
  const e = errors.find((x) => x.cell === "name");
  return e?.code === "dup"
    ? copyText("rot.save.dup")
    : e?.code === "tooLong"
      ? copyText("rot.save.tooLong")
      : e?.code === "empty"
        ? copyText("rot.save.empty")
        : copyText("rot.save.failed");
}

/** 那台拒了整条（它的账号库不认某格之类）：照名称那格说，别的格写「未保存」。 */
function cellsText(errors: CellError[]): string {
  return errors.some((x) => x.cell === "name")
    ? nameError(errors)
    : copyText("rot.save.failed");
}
