/**
 * 设置 → 机器 →「轮换」栏里一条规则的编辑器（同一栏里的子页，面包屑 `‹ 轮换 / 名字` 回列表；稿 `轮换规则.md` §5.6、截图 06 · 07）。
 *
 * 从上到下：头（名字点即就地改 · 默认标 · 在用 N 会话 · ⋯）＋ 后端那句说明 · 顺序（与面板同一套：拖 / Alt+↑↓ · 勾 · 封顶 · 兜底）·
 * 触发（满 / ≥N%）· 换法三张卡（各带一条示意小轴）· 无号可换 ＋ 兜底前最多等 · 按号封顶表（5h · 7d · 全部窗口 · 单段）· 预览。
 *
 * - 每格改完即存（`rotation-rule-save`，带读到的版本）；不弹 toast，顶上一行 `已存 HH:MM · [撤销上一处]`；头一次改动出影响条（5 秒）。
 * - 判定全在那台后端：名字对不对 · 触发范围 · 封顶时段 · 版本冲突 · 说明那一句 · 「无号可换」作不作数 · 预览怎么走 · 每格此刻取的上限。
 *   这里只排版、只认最后一趟回答。
 * 判据：`tests/frontend/ui/settings/rule-editor.vitest.ts`。
 */
import {
  readPlan,
  readQuota,
  renameRule,
  saveRule,
  type CapAt,
  type PlanRead,
  type RuleRow,
} from "../quota-reads";
import {
  capButton,
  capShort,
  fallbackToggle,
  howOf,
  moved,
  openCapEditor,
  rowsOf,
  stintAll,
  toggled,
  waitControl,
  withHow,
  type How,
  type Row,
} from "../rot-editor";
import { accountAvatarEl, accountColorSlot } from "../account-color";
import {
  accountLabel,
  slotLabel,
  slotValue,
  type QuotaRead,
  type QuotaReadAccount,
} from "../quota-lines";
import { button } from "../kit/button";
import { banner } from "../kit/banner";
import { foldCaret } from "../kit/fold";
import { tag } from "../kit/badge";
import { icon } from "../kit/icon";
import { segmented } from "../kit/tabs";
import { select } from "../kit/select";
import { attachTooltip } from "../kit/tooltip";
import { closePopover, openPopover } from "../kit/popover";
import { copyText } from "../copy-table";
import type { Origin } from "../ipc/origin";
import type { CapValue } from "../generated/CapValue";
import type { Rotation } from "../generated/Rotation";
import type { SwitchWhy } from "../generated/SwitchWhy";
import type { CellError } from "../generated/CellError";
import s from "./rule-editor.module.css";

const ALL = "*";
/** 改了几格后多久重问预览。 */
const PLAN_DEBOUNCE_MS = 300;
/** 头一次改动那条影响条停多久。 */
const IMPACT_MS = 5_000;
type Span = "6h" | "12h" | "24h" | "7d";

export interface RuleEditorHost {
  /** 回列表（面包屑 · Esc · 已删除那一形的［回到列表］）。 */
  back(): void;
  /** 写成了一处 / 要载入最新：宿主重读规则表，再 `update` 回来。 */
  reload(): void;
  /** ⋯ 那几项（复制 · 设为默认 · 复制到 · 删除，与列表同一份）。 */
  more(rule: RuleRow, anchor: HTMLElement): void;
  /** 「在用 N 会话」展开的名单（与列表同一份）。 */
  users(rule: RuleRow): HTMLElement;
}

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls: ((e: HTMLElementTagNameMap[K]) => void) | null,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls?.(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 换号点的原因短码（预览轴上方那一小格）。 */
function whyText(w: SwitchWhy | null): string {
  if (w === null) return "";
  if (w === "preempt") return copyText("rot.why.preempt");
  if (typeof w === "string") return "";
  if ("threshold" in w)
    return w.threshold.n === 0
      ? copyText("rot.why.off")
      : copyText("rot.why.trig", { n: w.threshold.n });
  if ("full" in w) return copyText("rot.why.full");
  if ("stint" in w) return copyText("rot.why.stint", { n: w.stint.n });
  if ("held" in w) return copyText("rot.why.held");
  if ("wait" in w)
    return copyText("rot.why.wait", { acct: accountLabel(w.wait.account) });
  return "";
}

function layerText(c: CapAt): string {
  if (c.v === null) return copyText("rot.capT.noCap");
  const layer =
    c.layer === "window"
      ? copyText("rot.capT.layerWindow")
      : c.layer === "all"
        ? copyText("rot.capT.layerAll")
        : copyText("rot.capT.layerTrigger");
  return copyText("rot.capT.effective", {
    v: copyText("rot.cap.fixedShort", { n: c.v }),
    layer,
  });
}

export class RuleEditor {
  readonly element: HTMLElement;
  private rule: RuleRow | null;
  private readonly origin: Origin;
  private readonly host: RuleEditorHost;
  private quota: QuotaRead | null = null;
  private plan: PlanRead | null = null;
  private span: Span = "12h";
  private planSeq = 0;
  private planTimer: ReturnType<typeof setTimeout> | null = null;
  private impactTimer: ReturnType<typeof setTimeout> | null = null;
  /** 上一处改动之前的那一份（撤销上一处）与写成的时刻。 */
  private undo: Rotation | null = null;
  private savedAt: string | null = null;
  private impact = false;
  private touched = false;
  private conflict = false;
  private errors: CellError[] = [];
  private renaming: { error: string | null } | null = null;
  private usersOpen = false;
  private uncheckedOpen = false;
  private writing = Promise.resolve();

  constructor(host: RuleEditorHost, origin: Origin, rule: RuleRow) {
    this.host = host;
    this.origin = origin;
    this.rule = rule;
    this.element = el("div", (e) => (e.className = s.ed));
    this.element.dataset.ruleEditor = rule.id;
    this.element.addEventListener("keydown", (ev) => {
      if (ev.key !== "Escape" || ev.defaultPrevented) return;
      if ((ev.target as HTMLElement).dataset.edRename !== undefined) return;
      ev.stopPropagation();
      this.host.back();
    });
    void readQuota(origin).then(
      (q) => {
        this.quota = q;
        this.paint();
      },
      (e: unknown) => console.warn(`[rules] quota-read [${origin}] 失败：`, e),
    );
    this.paint();
    this.askPlan();
  }

  /** 宿主重读到的这一条（`null` ＝ 在别处被删了）。 */
  update(rule: RuleRow | null): void {
    // 与这边刚写成的那一趟赛跑、晚到的旧表（版本比手上的旧）⇒ 不认。
    if (rule && this.rule && rule.rev < this.rule.rev) return;
    const changed =
      rule === null || this.rule === null || rule.rev !== this.rule.rev;
    this.rule = rule;
    if (rule && changed) this.conflict = false;
    this.paint();
    if (rule && changed) this.askPlan();
  }

  /** 宿主重画这一栏时调（名单勾选之类改的是宿主的状态）。 */
  refresh(): void {
    this.paint();
  }

  get id(): string | null {
    return this.rule?.id ?? null;
  }

  dispose(): void {
    for (const t of [this.planTimer, this.impactTimer])
      if (t !== null) clearTimeout(t);
    this.planSeq++;
  }

  // ───────────────────────────── 写 ─────────────────────────────

  /** 一格改完即存：带读到的版本；写成 ⇒ 记撤销、头一次出影响条、重问预览。 */
  private write(next: Rotation, isUndo = false): Promise<void> {
    this.writing = this.writing.then(() => this.writeNow(next, isUndo));
    return this.writing;
  }

  private async writeNow(next: Rotation, isUndo: boolean): Promise<void> {
    const rule = this.rule;
    if (!rule) return;
    let got;
    try {
      got = await saveRule(this.origin, {
        id: rule.id,
        name: rule.name,
        rotation: next,
        ifRev: rule.rev,
      });
    } catch (e) {
      console.warn("[rules] rotation-rule-save 失败：", e);
      this.errors = [{ cell: "rotation", code: "io" } as CellError];
      this.paint();
      return;
    }
    if (got.state === "conflict") {
      this.conflict = true;
      this.paint();
      return;
    }
    if (got.state === "refused") {
      this.errors = got.errors;
      this.paint();
      return;
    }
    this.errors = [];
    this.undo = isUndo ? null : rule.rotation;
    this.savedAt = new Date().toTimeString().slice(0, 5);
    if (!this.touched) {
      this.touched = true;
      this.impact = true;
      // 调度：一次性 —— 头一次改动那条影响条 5 秒后撤
      this.impactTimer = setTimeout(() => {
        this.impactTimer = null;
        this.impact = false;
        this.paint();
      }, IMPACT_MS);
    }
    this.rule = got.rule;
    this.paint();
    this.askPlan();
    this.host.reload();
  }

  /** 预览：改了几格之后 300ms 防抖重问（只认最后一趟）。 */
  private askPlan(): void {
    if (this.planTimer !== null) clearTimeout(this.planTimer);
    // 调度：合批 —— 连着改几格合成一次，只问最后那一份的预览
    this.planTimer = setTimeout(() => {
      this.planTimer = null;
      const rule = this.rule;
      if (!rule) return;
      const my = ++this.planSeq;
      void readPlan(this.origin, { rule: rule.id, span: this.span }).then(
        (p) => {
          if (my !== this.planSeq) return;
          this.plan = p;
          this.paint();
        },
        (e: unknown) =>
          console.warn(`[rules] rotation-plan [${this.origin}] 失败：`, e),
      );
    }, PLAN_DEBOUNCE_MS);
  }

  // ───────────────────────────── 画 ─────────────────────────────

  private paint(): void {
    const rule = this.rule;
    if (!rule) {
      const gone = el("div", (e) => (e.className = s.edGone));
      gone.dataset.edGone = "true";
      gone.append(
        el(
          "span",
          null,
          copyText("rot.ed.gone", {
            name: this.element.dataset.ruleName ?? "",
          }),
        ),
        button({
          label: copyText("rot.ed.backToList"),
          size: "compact",
          onClick: () => this.host.back(),
        }),
      );
      this.element.replaceChildren(gone);
      return;
    }
    this.element.dataset.ruleName = rule.name;
    const r = rule.rotation;
    const write = (next: Rotation): void => void this.write(next);
    const out: HTMLElement[] = [this.head(rule)];
    if (this.conflict) {
      const re = button({
        label: copyText("rot.ed.reload"),
        size: "compact",
        onClick: () => this.host.reload(),
      });
      const b = banner("error", copyText("rot.fail.conflict"), [re]);
      b.dataset.edConflict = "true";
      out.push(b);
    }
    if (this.impact) {
      const b = el(
        "div",
        (e) => (e.className = s.edImpact),
        rule.isDefault
          ? copyText("rot.ed.impactDefault", { n: rule.users.follow })
          : copyText("rot.ed.impact", { n: rule.users.live }),
      );
      b.dataset.edImpact = "true";
      out.push(b);
    }
    if (rule.missing.length > 0)
      out.push(
        banner("warn", copyText("rot.ed.missing", { n: rule.missing.length })),
      );
    out.push(this.block(copyText("rot.ed.order"), this.order(rule, write)));
    out.push(this.block(copyText("acct.rot.trigger"), this.trigger(r, write)));
    out.push(this.block(copyText("rot.how.label"), this.how(r, write)));
    out.push(this.block(copyText("acct.lim.label"), this.atLimit(rule, write)));
    out.push(this.block(copyText("rot.capT.head"), this.caps(r, write)));
    out.push(this.preview(rule));
    this.element.replaceChildren(...out);
    if (this.renaming) {
      const inp =
        this.element.querySelector<HTMLInputElement>("[data-ed-rename]");
      if (inp && document.activeElement !== inp) {
        inp.focus();
        inp.select();
      }
    }
  }

  private block(label: string, body: HTMLElement): HTMLElement {
    const b = el("div", (e) => (e.className = s.edBlock));
    b.append(
      el("div", (e) => (e.className = s.edLabel), label),
      body,
    );
    return b;
  }

  /** `‹ 轮换 / 名字 [默认]   在用 N 会话 ▾  ⋯`，下一行后端那句说明，再下一行 `已存 HH:MM · [撤销上一处]`。 */
  private head(rule: RuleRow): HTMLElement {
    const box = el("div", (e) => (e.className = s.edHead));
    const line = el("div", (e) => (e.className = s.edCrumbs));
    const back = button({
      label: copyText("rot.ed.back"),
      kind: "ghost",
      size: "compact",
      icon: "caretLeft",
      onClick: () => this.host.back(),
    });
    back.dataset.edBack = "true";
    line.append(
      back,
      el("span", (e) => (e.className = s.edSlash), "/"),
    );
    if (this.renaming) {
      const inp = document.createElement("input");
      inp.type = "text";
      inp.className = s.edNameInput;
      inp.value = rule.name;
      inp.dataset.edRename = "true";
      inp.setAttribute("aria-label", copyText("rot.ed.rename"));
      if (this.renaming.error) inp.dataset.error = "true";
      let done = false;
      const commit = async (): Promise<void> => {
        if (done) return;
        done = true;
        const name = inp.value;
        if (name === rule.name) {
          this.renaming = null;
          this.paint();
          return;
        }
        try {
          const got = await renameRule(this.origin, {
            id: rule.id,
            name,
            ifRev: rule.rev,
          });
          if (got.state === "saved") {
            this.renaming = null;
            this.rule = got.rule;
            this.host.reload();
          } else
            this.renaming = {
              error:
                got.state === "refused"
                  ? nameError(got.errors)
                  : copyText("rot.fail.conflict"),
            };
        } catch (e) {
          console.warn("[rules] rotation-rule-rename 失败：", e);
          this.renaming = { error: copyText("rot.save.failed") };
        }
        this.paint();
      };
      inp.addEventListener("keydown", (ev) => {
        if (ev.isComposing) return;
        if (ev.key === "Enter") {
          ev.preventDefault();
          void commit();
        } else if (ev.key === "Escape") {
          ev.preventDefault();
          ev.stopPropagation();
          done = true;
          this.renaming = null;
          this.paint();
        }
      });
      inp.addEventListener("blur", () => void commit());
      line.appendChild(inp);
      if (this.renaming.error)
        line.appendChild(
          el("span", (e) => (e.className = s.edErr), this.renaming.error),
        );
    } else {
      const name = document.createElement("button");
      name.type = "button";
      name.className = s.edName;
      name.textContent = rule.name;
      name.dataset.edName = "true";
      name.addEventListener("click", () => {
        this.renaming = { error: null };
        this.paint();
      });
      line.appendChild(name);
    }
    if (rule.isDefault) line.appendChild(tag(copyText("rot.src.tagDefault")));
    line.appendChild(el("span", (e) => (e.className = s.edSp)));
    if (rule.users.live + rule.users.ended > 0) {
      const use = button({
        label: copyText("rot.src.inUse", { n: rule.users.live }),
        kind: "ghost",
        size: "compact",
      });
      use.appendChild(foldCaret());
      use.dataset.edUsers = "true";
      use.setAttribute("aria-expanded", String(this.usersOpen));
      use.addEventListener("click", () => {
        this.usersOpen = !this.usersOpen;
        this.paint();
      });
      line.appendChild(use);
    }
    const more = button({
      label: copyText("rot.list.more", { name: rule.name }),
      kind: "icon",
      icon: "more",
      size: "compact",
      hint: copyText("rot.list.more", { name: rule.name }),
    });
    more.dataset.edMore = "true";
    more.addEventListener("click", () => this.host.more(rule, more));
    line.appendChild(more);
    box.appendChild(line);
    if (this.usersOpen) box.appendChild(this.host.users(rule));
    if (rule.explain) {
      const ex = el("div", (e) => (e.className = s.edExplain), rule.explain);
      ex.dataset.edExplain = "true";
      box.appendChild(ex);
    }
    if (this.savedAt !== null) {
      const st = el("div", (e) => (e.className = s.edSaved));
      st.dataset.edSaved = "true";
      st.appendChild(
        el("span", null, copyText("rot.ed.saved", { at: this.savedAt })),
      );
      const prev = this.undo;
      if (prev) {
        const u = button({
          label: copyText("rot.ed.undoLast"),
          kind: "ghost",
          size: "compact",
          onClick: () => void this.write(prev, true),
        });
        u.dataset.edUndo = "true";
        st.appendChild(u);
      }
      box.appendChild(st);
    }
    return box;
  }

  /** 顺序：与面板同一套行（起始账号占位斜体 · 勾 · 头像 · 名 · 封顶 · 兜底 · 此刻用量），拖把手或 Alt+↑↓ 挪位。 */
  private order(rule: RuleRow, write: (r: Rotation) => void): HTMLElement {
    const r = rule.rotation;
    const agent =
      this.quota?.accounts[0]?.agent ?? this.quota?.unseen[0]?.agent ?? "";
    const rows = rowsOf(r, "", this.quota, agent);
    const list = el("div", (e) => (e.className = s.edList));
    list.setAttribute("role", "list");
    const lines: HTMLElement[] = [];
    rows.forEach((row, i) => {
      const line = el("div", (e) => (e.className = s.edRow));
      line.setAttribute("role", "listitem");
      line.dataset.edRow = row.start ? "start" : row.account;
      line.tabIndex = 0;
      const handle = el("span", (e) => (e.className = s.edHandle));
      handle.appendChild(icon("drag", "compact"));
      const label = row.start
        ? copyText("rot.ed.start")
        : accountLabel(row.account);
      handle.setAttribute(
        "aria-label",
        copyText("acct.row.dragAria", { name: label }),
      );
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = row.on;
      box.disabled = row.start;
      box.setAttribute(
        "aria-label",
        copyText("acct.row.checkAria", { name: label }),
      );
      box.addEventListener("change", () =>
        write(toggled(r, row.account, box.checked)),
      );
      line.append(handle, box);
      if (row.start) {
        const av = el(
          "span",
          (e) => (e.className = s.edStartAvatar),
          copyText("acct.home.avatar"),
        );
        av.setAttribute("aria-hidden", "true");
        line.append(
          av,
          el("span", (e) => (e.className = s.edStartName), label),
        );
      } else {
        line.append(
          accountAvatarEl(row.account, { size: 16 }),
          el("span", (e) => (e.className = s.edRowName), label),
        );
        if (rule.missing.includes(row.account)) {
          const t = tag(copyText("rot.ed.missingTag"));
          t.dataset.edMissing = "";
          line.appendChild(t);
        }
        line.appendChild(el("span", (e) => (e.className = s.edSp)));
        line.appendChild(capButton(this.origin, r, row.account, write));
        if (r.order.includes(row.account))
          line.appendChild(fallbackToggle(r, row.account, label, write));
        line.appendChild(this.usage(row));
      }
      line.addEventListener("keydown", (ev) => {
        if (
          ev.isComposing ||
          !ev.altKey ||
          (ev.key !== "ArrowUp" && ev.key !== "ArrowDown")
        )
          return;
        ev.preventDefault();
        const to = ev.key === "ArrowUp" ? i - 1 : i + 1;
        if (to >= 0 && to < rows.length) write(moved(r, rows, i, to));
      });
      handle.addEventListener("pointerdown", (ev) =>
        this.drag(ev, lines, i, (to) => write(moved(r, rows, i, to))),
      );
      lines.push(line);
      list.appendChild(line);
    });
    return list;
  }

  /** 行尾此刻用量（卡着它的那一格）：`5h 63%`；按量号 `无 5h / 7d`。 */
  private usage(row: Row): HTMLElement {
    const q: QuotaReadAccount | undefined = this.quota?.accounts.find(
      (x) => x.account === row.account,
    );
    const u = el("span", (e) => (e.className = s.edUsage));
    if (!q) return u;
    if (q.kind === "api") u.textContent = copyText("acct.val.noLimit");
    else {
      const slot = q.limiting ?? "5h";
      u.textContent = `${slotLabel(slot)} ${slotValue(q, slot)}`;
    }
    return u;
  }

  /** 拖把手：按指针落在哪一行之间定新位置，松手才写。 */
  private drag(
    ev: PointerEvent,
    lines: HTMLElement[],
    from: number,
    done: (to: number) => void,
  ): void {
    if (ev.button !== 0) return;
    ev.preventDefault();
    const src = lines[from];
    src.dataset.lifted = "true";
    let to = from;
    const onMove = (e: PointerEvent): void => {
      to = lines.findIndex((l) => {
        const b = l.getBoundingClientRect();
        return e.clientY < b.top + b.height / 2;
      });
      if (to < 0) to = lines.length - 1;
      else if (to > from) to -= 1;
    };
    const onUp = (): void => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      delete src.dataset.lifted;
      if (to !== from) done(to);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  /** `(•)满 ( )≥[90]%`：范围后端判（1–99），错了框红 ＋ 下一行红字。 */
  private trigger(r: Rotation, write: (r: Rotation) => void): HTMLElement {
    const box = el("div", (e) => (e.className = s.edLine));
    const pct = r.when !== "full";
    const n = r.when === "full" ? 90 : r.when.threshold.n;
    const name = `ed-trigger-${this.rule?.id ?? ""}`;
    const radio = (
      on: boolean,
      label: string,
      pick: () => void,
    ): HTMLLabelElement => {
      const l = el("label", (e) => (e.className = s.edRadio));
      const i = document.createElement("input");
      i.type = "radio";
      i.name = name;
      i.checked = on;
      i.addEventListener("change", pick);
      l.append(i, document.createTextNode(label));
      return l;
    };
    const num = document.createElement("input");
    num.type = "text";
    num.inputMode = "numeric";
    num.className = s.edNum;
    num.value = String(n);
    num.dataset.edPct = "true";
    num.setAttribute("aria-label", copyText("acct.rot.trigPct"));
    const bad = this.errors.some((e) => e.cell.startsWith("when"));
    if (bad) num.dataset.error = "true";
    const commit = (): void => {
      const v = Number(num.value.trim());
      if (pct && v === n) return;
      // 写错的数也交后端（它回 range、框红），这里不另判范围。
      write({
        ...r,
        when: { threshold: { n: Number.isFinite(v) ? Math.trunc(v) : -1 } },
      });
    };
    num.addEventListener("change", commit);
    num.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) num.blur();
    });
    box.append(
      radio(!pct, copyText("acct.rot.trigFull"), () =>
        write({ ...r, when: "full" }),
      ),
      radio(pct, copyText("acct.rot.trigPct"), () =>
        write({ ...r, when: { threshold: { n } } }),
      ),
      num,
      el("span", (e) => (e.className = s.edUnit), copyText("acct.rot.pctUnit")),
    );
    if (bad) {
      const e = el(
        "span",
        (e) => (e.className = s.edErr),
        copyText("rot.ed.pctErr"),
      );
      e.dataset.edPctErr = "true";
      box.appendChild(e);
    }
    return box;
  }

  /** 换法三张单选卡（各一行说明 ＋ 一条示意小轴）；单段预算那张带点数与「同时抢回」。 */
  private how(r: Rotation, write: (r: Rotation) => void): HTMLElement {
    const cur = howOf(r);
    const n = stintAll(r) ?? 10;
    const box = el("div", (e) => (e.className = s.edCards));
    box.setAttribute("role", "radiogroup");
    box.setAttribute("aria-label", copyText("rot.how.label"));
    const card = (
      k: How,
      title: string,
      hint: string,
      axis: HTMLElement,
      extra?: HTMLElement,
    ): HTMLElement => {
      const c = el("div", (e) => (e.className = s.edCard));
      c.dataset.edHow = k;
      c.setAttribute("role", "radio");
      c.setAttribute("aria-checked", String(cur === k));
      c.tabIndex = cur === k ? 0 : -1;
      if (cur === k) c.dataset.on = "true";
      const pick = (): void => {
        if (cur !== k) write(withHow(r, k, n));
      };
      c.addEventListener("click", (ev) => {
        if ((ev.target as HTMLElement).closest("input")) return;
        pick();
      });
      c.addEventListener("keydown", (ev) => {
        if (ev.key === " " || ev.key === "Enter") {
          ev.preventDefault();
          pick();
        }
      });
      const t = el("div", (e) => (e.className = s.edCardTitle));
      const dot = el("span", (e) => (e.className = s.edCardDot));
      t.append(dot, el("span", null, title));
      if (extra) t.appendChild(extra);
      c.append(
        t,
        el("div", (e) => (e.className = s.edCardHint), hint),
        axis,
      );
      return c;
    };
    const axis = (
      parts: { w: number; c: number; mark?: string }[],
    ): HTMLElement => {
      const a = el("div", (e) => (e.className = s.edAxis));
      a.setAttribute("aria-hidden", "true");
      for (const p of parts) {
        const seg = el("span", (e) => (e.className = s.edAxisSeg), p.mark);
        seg.style.flexGrow = String(p.w);
        seg.dataset.c = String(p.c);
        a.appendChild(seg);
      }
      return a;
    };
    const stintN = document.createElement("input");
    stintN.type = "text";
    stintN.inputMode = "numeric";
    stintN.className = s.edNum;
    stintN.value = String(n);
    stintN.dataset.edStint = "true";
    stintN.setAttribute("aria-label", copyText("rot.how.stint"));
    stintN.addEventListener("change", () => {
      const v = Number(stintN.value.trim());
      if (Number.isInteger(v) && v !== n) write(withHow(r, "stint", v));
    });
    const stintBox = el("span", (e) => (e.className = s.edCardExtra));
    stintBox.append(
      stintN,
      el("span", (e) => (e.className = s.edUnit), copyText("rot.how.unit")),
    );
    const stintCard = card(
      "stint",
      copyText("rot.how.stint"),
      copyText("rot.how.stintHint", { n }),
      axis([
        { w: 1, c: 0, mark: String(n) },
        { w: 1, c: 1, mark: String(n) },
        { w: 1, c: 2 },
      ]),
      stintBox,
    );
    if (cur === "stint") {
      const also = el("label", (e) => (e.className = s.edRadio));
      const c = document.createElement("input");
      c.type = "checkbox";
      c.checked = r.preempt ?? false;
      c.dataset.edAlsoPreempt = "true";
      c.addEventListener("change", () =>
        write(withHow(r, "stint", n, c.checked)),
      );
      also.append(c, document.createTextNode(copyText("rot.how.stintPlus")));
      stintCard.appendChild(also);
    }
    box.append(
      card(
        "order",
        copyText("rot.how.order"),
        copyText("rot.how.orderHint"),
        axis([
          { w: 2, c: 0, mark: copyText("rot.why.full") },
          { w: 3, c: 1 },
        ]),
      ),
      card(
        "preempt",
        copyText("rot.how.preempt"),
        copyText("rot.how.preemptHint"),
        axis([
          { w: 2, c: 0, mark: copyText("rot.why.full") },
          { w: 1, c: 1 },
          { w: 2, c: 0, mark: copyText("rot.why.preempt") },
        ]),
      ),
      stintCard,
    );
    return box;
  }

  /** `无号可换 [继续跑 | 停]`（灰与否按后端 `atLimitApplies`）＋ `兜底前最多等 [40] 分`。 */
  private atLimit(rule: RuleRow, write: (r: Rotation) => void): HTMLElement {
    const r = rule.rotation;
    const box = el("div", (e) => (e.className = s.edLine));
    const seg = segmented<"continue" | "stop">({
      items: [
        { key: "continue", label: copyText("acct.lim.go") },
        { key: "stop", label: copyText("acct.lim.stop") },
      ],
      current: r.atLimit,
      label: copyText("acct.lim.aria"),
      onChange: (k) => write({ ...r, atLimit: k }),
    });
    seg.dataset.edAtLimit = "true";
    if (!rule.atLimitApplies) {
      seg.dataset.disabled = "true";
      for (const b of seg.querySelectorAll<HTMLButtonElement>("button"))
        b.disabled = true;
      attachTooltip(seg, copyText("rot.lim.noCap"));
    }
    box.append(seg, waitControl(r, false, write));
    return box;
  }

  /** 按号封顶表：行 ＝ 勾上的号（未勾的折着）；列 `5h · 7d · 全部窗口`（换法是单段预算时多「单段」）。悬停一格 ＝ 此刻实际取的值与来自哪一层。 */
  private caps(r: Rotation, write: (r: Rotation) => void): HTMLElement {
    const box = el("div", (e) => (e.className = s.edCaps));
    box.setAttribute("role", "table");
    const stint = howOf(r) === "stint";
    const cols: { w: string; label: string }[] = [
      { w: "5h", label: slotLabel("5h") },
      { w: "7d", label: slotLabel("7d") },
      { w: ALL, label: copyText("rot.cap.colAll") },
    ];
    const head = el("div", (e) => (e.className = s.edCapRow));
    head.setAttribute("role", "row");
    const h0 = el(
      "span",
      (e) => (e.className = s.edCapHead),
      copyText("rot.capT.colAcct"),
    );
    const info = el("span", (e) => (e.className = s.edInfo));
    info.tabIndex = 0;
    info.setAttribute("aria-label", copyText("rot.capT.layers"));
    info.appendChild(icon("info", "compact"));
    attachTooltip(info, copyText("rot.capT.layers"));
    h0.appendChild(info);
    head.appendChild(h0);
    for (const c of cols)
      head.appendChild(el("span", (e) => (e.className = s.edCapHead), c.label));
    if (stint)
      head.appendChild(
        el(
          "span",
          (e) => (e.className = s.edCapHead),
          copyText("rot.capT.colStint"),
        ),
      );
    box.appendChild(head);
    const named = r.order.filter((x): x is string => typeof x === "string");
    const on = named.filter((a) => r.enabled.includes(a));
    const off = named.filter((a) => !r.enabled.includes(a));
    const line = (a: string): HTMLElement => {
      const row = el("div", (e) => (e.className = s.edCapRow));
      row.setAttribute("role", "row");
      row.dataset.edCapRow = a;
      const who = el("span", (e) => (e.className = s.edCapWho));
      who.append(
        accountAvatarEl(a, { size: 16 }),
        el("span", null, accountLabel(a)),
      );
      row.appendChild(who);
      for (const c of cols) {
        const v: CapValue | undefined = r.cap?.[a]?.[c.w];
        const b = button({
          label: v === undefined ? copyText("rot.list.useNone") : capShort(v),
          kind: "secondary",
          size: "compact",
          onClick: () => openCapEditor(b, this.origin, r, a, c.w, write),
        });
        b.dataset.edCap = `${a}.${c.w}`;
        const eff = this.plan?.effective[a]?.[c.w];
        if (eff) attachTooltip(b, layerText(eff));
        row.appendChild(b);
      }
      if (stint) {
        const v = r.stint?.[a]?.[ALL];
        const b = button({
          label: v === undefined ? copyText("rot.list.useNone") : String(v),
          kind: "secondary",
          size: "compact",
          onClick: () => this.openStint(b, r, a, write),
        });
        b.dataset.edStintCell = a;
        row.appendChild(b);
      }
      return row;
    };
    for (const a of on) box.appendChild(line(a));
    if (off.length > 0) {
      const fold = button({
        label: copyText("rot.capT.unchecked", { n: off.length }),
        kind: "ghost",
        size: "compact",
      });
      fold.prepend(foldCaret());
      fold.dataset.edUnchecked = "true";
      fold.setAttribute("aria-expanded", String(this.uncheckedOpen));
      fold.addEventListener("click", () => {
        this.uncheckedOpen = !this.uncheckedOpen;
        this.paint();
      });
      box.appendChild(fold);
      if (this.uncheckedOpen) for (const a of off) box.appendChild(line(a));
    }
    return box;
  }

  /** 单段那一格：只有「固定」（点数）；空 ＝ 不设（落到所有号那一格）。 */
  private openStint(
    anchor: HTMLElement,
    r: Rotation,
    a: string,
    write: (r: Rotation) => void,
  ): void {
    const root = el("div", (e) => (e.className = s.edPop));
    root.dataset.edStintPop = a;
    root.appendChild(
      el(
        "div",
        (e) => (e.className = s.edPopTitle),
        copyText("rot.capT.stintTitle", { acct: accountLabel(a) }),
      ),
    );
    const inp = document.createElement("input");
    inp.type = "text";
    inp.inputMode = "numeric";
    inp.className = s.edNum;
    inp.value = String(r.stint?.[a]?.[ALL] ?? "");
    inp.setAttribute("aria-label", copyText("rot.capT.colStint"));
    const row = el("div", (e) => (e.className = s.edLine));
    row.append(
      inp,
      el("span", (e) => (e.className = s.edUnit), copyText("rot.how.unit")),
    );
    root.appendChild(row);
    const go = (): void => {
      const t = inp.value.trim();
      const stints = { ...(r.stint ?? {}) };
      if (t === "") delete stints[a];
      else stints[a] = { [ALL]: Number(t) };
      closePopover();
      write({ ...r, stint: stints });
    };
    inp.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        go();
      }
    });
    const foot = el("div", (e) => (e.className = s.edPopFoot));
    foot.append(
      button({
        label: copyText("rot.cap.cancel"),
        kind: "ghost",
        size: "compact",
        onClick: () => closePopover(),
      }),
      button({
        label: copyText("rot.cap.ok"),
        kind: "primary",
        size: "compact",
        onClick: go,
      }),
    );
    root.appendChild(foot);
    openPopover(anchor, root, {
      label: copyText("rot.capT.colStint"),
      align: "start",
    });
    inp.focus();
  }

  /** 预览：`预览 [接下来 12h ▾]`；「这条规则」一条按号着色（换号点上方写原因）＋ 池里每号一条（不能用的段打底纹 · 重置点）＋ 图例。 */
  private preview(rule: RuleRow): HTMLElement {
    const box = el("div", (e) => (e.className = s.edPreview));
    box.dataset.edPreview = rule.id;
    const head = el("div", (e) => (e.className = s.edLine));
    const spans: Span[] = ["6h", "12h", "24h", "7d"];
    const sel = select({
      label: copyText("rot.pv.label"),
      options: spans.map((x) => ({
        value: x,
        label: copyText("rot.pv.span", { span: x }),
      })),
      value: this.span,
      onChange: (v) => {
        this.span = v as Span;
        this.askPlan();
      },
    });
    sel.el.dataset.edSpan = "true";
    const selBox = el("span", (e) => (e.className = s.edSpanSel));
    selBox.appendChild(sel.el);
    head.append(
      el("span", (e) => (e.className = s.edLabel), copyText("rot.pv.label")),
      selBox,
      el("span", (e) => (e.className = s.edHint), copyText("rot.pv.hint")),
    );
    box.appendChild(head);
    const p = this.plan;
    if (!p || p.plan.length === 0) return box;
    const t0 = p.now;
    const len = Math.max(1, p.until - t0);
    const pos = (t: number): string =>
      `${(((t - t0) / len) * 100).toFixed(3)}%`;
    const wid = (a: number, b: number): string =>
      `${(((b - a) / len) * 100).toFixed(3)}%`;
    const grid = el("div", (e) => (e.className = s.edAxisGrid));
    const track = (label: HTMLElement, id: string): HTMLElement => {
      const row = el("div", (e) => (e.className = s.edLane));
      row.dataset.edLane = id;
      const t = el("div", (e) => (e.className = s.edTrack));
      row.append(label, t);
      grid.appendChild(row);
      return t;
    };
    const ruleTrack = track(
      el("span", (e) => (e.className = s.edLaneName), copyText("rot.pv.rule")),
      "rule",
    );
    for (const seg of p.plan) {
      const b = el("span", (e) => (e.className = s.edPlanSeg));
      b.style.left = pos(seg.from);
      b.style.width = wid(seg.from, seg.to);
      b.dataset.edSeg = seg.account ?? "";
      if (seg.account === null) b.dataset.held = "true";
      else b.style.background = `var(--acct-c${accountColorSlot(seg.account)})`;
      attachTooltip(
        b,
        copyText("rot.pv.seg", {
          from: seg.fromText,
          to: seg.toText,
          acct:
            seg.account === null
              ? copyText("rot.pv.held")
              : accountLabel(seg.account),
        }),
      );
      const why = whyText(seg.why);
      if (why) {
        const w = el("span", (e) => (e.className = s.edWhy), why);
        w.dataset.edWhy = "";
        b.appendChild(w);
      }
      ruleTrack.appendChild(b);
    }
    for (const lane of p.lanes) {
      const name = el("span", (e) => (e.className = s.edLaneName));
      name.append(
        accountAvatarEl(lane.account, { size: 14 }),
        el("span", null, accountLabel(lane.account)),
      );
      const t = track(name, lane.account);
      for (const sp of lane.spans) {
        const b = el("span", (e) => (e.className = s.edLaneSpan));
        b.style.left = pos(sp.from);
        b.style.width = wid(sp.from, sp.to);
        b.dataset.state = sp.state;
        t.appendChild(b);
      }
      for (const r of lane.resets) {
        const m = el("span", (e) => (e.className = s.edReset));
        m.style.left = pos(r.at);
        m.dataset.w = r.w;
        attachTooltip(
          m,
          `${slotLabel(r.w)} ${copyText("rot.pv.lgReset")} ${r.atText}`,
        );
        t.appendChild(m);
      }
    }
    const ticks = el("div", (e) => (e.className = s.edTicks));
    ticks.append(
      el("span", null, p.nowText),
      el("span", null, p.plan[p.plan.length - 1].toText),
    );
    grid.appendChild(ticks);
    box.appendChild(grid);
    const lg = el("div", (e) => (e.className = s.edLegend));
    for (const [text, state] of [
      [copyText("rot.pv.lgUse"), ""],
      [copyText("rot.pv.lgRefused"), "refused"],
      [copyText("rot.pv.lgCapped"), "capped"],
      [copyText("rot.pv.lgOff"), "off"],
      [copyText("rot.pv.lgReset"), "reset"],
    ] as const) {
      const it = el("span", (e) => (e.className = s.edLegendItem));
      const sw = el("span", (e) => (e.className = s.edSwatch));
      sw.dataset.state = state;
      it.append(sw, el("span", null, text));
      lg.appendChild(it);
    }
    box.appendChild(lg);
    return box;
  }
}

/** 名称那一格的错 ⇒ 红字（与列表改名同一套）。 */
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
