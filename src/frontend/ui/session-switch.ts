/**
 * **会话头上的两颗：换模型 · 切权限档**（会话头那一排、状态一句之后）。
 *
 * 全照那台后端给的：字（会话事实 `modelText` · `permissionModeText`；没有 ⇒ 写这颗是什么）· 清单（`session-open-facts` 那一家的
 * `modelDefault` ＋ `models` ＋ `modelNote` · `permissionModes`）· 此刻能不能换（会话帧 `switch`；那台握手说做不到这条命令 ⇒ 那一句）·
 * 换的结局（`session-model` · `session-permission` 回包的 `result` · 读回的字 · `said`）。界面只管「正在换」这一态（发起的这一端才知道）：
 * 两颗都不可点、换的那颗写选的那一项、原位转圈；读回 `done` 才换字，没成 ⇒ 回原字 ＋ toast 写那台那一句。
 * 读回的字只盖到下一份会话事实里那一格变了为止（之后照后端那一格）。
 */
import type { Tab } from "./tab-model";
import type { AgentFacts } from "./generated/AgentFacts";
import type { AgentPick } from "./generated/AgentPick";
import type { OpenFacts } from "./generated/OpenFacts";
import { copyText } from "./copy-table";
import { icon } from "./kit/icon";
import { spinner } from "./kit/progress";
import { attachTooltip } from "./kit/tooltip";
import { openMenu, type MenuItem } from "./kit/menu";
import s from "./session-head.module.css";

export type SwitchKind = "model" | "permission";

/** 一趟换的结局：成没成 · 读回的字 · 那台那一句（读成同一形，回包与没回来的都是）。 */
export interface Switched {
  done: boolean;
  text: string | null;
  said: string | null;
}

/** 换的这一端自己记的（每条会话一份）。 */
export interface SwitchLocal {
  pending: { kind: SwitchKind; pick: AgentPick } | null;
  /** 读回的字盖在后端那一格上：`was` ＝ 盖上那一刻后端那一格的字；那一格变了 ⇒ 不再盖。 */
  over: Partial<Record<SwitchKind, { text: string; value: string; was: string | null }>>;
}

/** 一颗此刻画成什么样。 */
export interface ChipFace {
  text: string;
  /** 能点（菜单打得开）。 */
  enabled: boolean;
  /** 灰着的那一句（那台写的）；能点 ⇒ `null`。 */
  why: string | null;
  spinning: boolean;
}

/** 两颗各自交给哪条命令（灰不灰也看那台握手时说没说做不到它）。 */
export const SWITCH_OP: Record<SwitchKind, "session-model" | "session-permission"> = { model: "session-model", permission: "session-permission" };

/** 两颗各自那两句：这颗是什么（悬停 · 菜单组名 · 没字时 chip 上那个字）· 没成又没那台那一句时的「结果未知」。 */
const WORDS: Record<SwitchKind, { title: () => string; unsure: () => string }> = {
  model: { title: () => copyText("session.model.title"), unsure: () => copyText("session.model.unsure") },
  permission: { title: () => copyText("session.permission.title"), unsure: () => copyText("session.permission.unsure") },
};

/** 这颗是什么。 */
export function switchTitle(kind: SwitchKind): string {
  return WORDS[kind].title();
}

/** 后端那一格的字。 */
function factText(tab: Tab, kind: SwitchKind): string | null {
  return kind === "model" ? tab.modelText : tab.permissionModeText;
}

/** 还盖着的那一份读回（后端那一格从盖上起没变过）；没有 ⇒ `null`。 */
function overOf(tab: Tab, kind: SwitchKind, local: SwitchLocal): { text: string; value: string } | null {
  const o = local.over[kind];
  return o && o.was === factText(tab, kind) ? o : null;
}

/** chip 上的字：还盖着的读回 ＞ 后端那一格 ＞ 这颗是什么。 */
export function shownText(tab: Tab, kind: SwitchKind, local: SwitchLocal): string {
  return overOf(tab, kind, local)?.text ?? factText(tab, kind) ?? switchTitle(kind);
}

/**
 * 一颗此刻的样子。正在换 ⇒ 两颗都不可点、换的那颗写选的那一项并转圈；那台握手说做不到 ⇒ 灰、写那一句；
 * 那一家清单是空的（`picks` ＝ 0；还没问到 ⇒ `null`）⇒ 只显示字；会话帧说不能 ⇒ 灰、写 `why`；这次没收到过那一帧 ⇒ 不判（能点，命令照实回）。
 */
export function chipFace(tab: Tab, kind: SwitchKind, local: SwitchLocal, unavailable: string | null, picks: number | null): ChipFace {
  const p = local.pending;
  if (p) return { text: p.kind === kind ? p.pick.text : shownText(tab, kind, local), enabled: false, why: null, spinning: p.kind === kind };
  const text = shownText(tab, kind, local);
  if (unavailable !== null) return { text, enabled: false, why: unavailable, spinning: false };
  // 那一家没有可换的（清单问到了、是空的）⇒ 只显示字。
  if (picks === 0) return { text, enabled: false, why: null, spinning: false };
  const can = tab.switch?.[kind];
  if (can?.can === "no") return { text, enabled: false, why: can.why, spinning: false };
  return { text, enabled: true, why: null, spinning: false };
}

/** 这条会话是那台列的哪一家（按会话事实的 `agent` 认；认不出 ⇒ 那台列的第一家）。 */
export function agentOf(facts: OpenFacts, kind: string | null): AgentFacts | null {
  return facts.agents.find((a) => a.kind === kind) ?? facts.agents[0] ?? null;
}

/** 菜单里列的（模型：「默认」在前 ＋ 那一家的模型，底下那一行；权限档：那一家的档）。 */
export function picksOf(agent: AgentFacts, kind: SwitchKind): { picks: AgentPick[]; note: string | null } {
  if (kind === "permission") return { picks: agent.permissionModes, note: null };
  return { picks: agent.modelDefault ? [agent.modelDefault, ...agent.models] : agent.models, note: agent.modelNote };
}

/** 勾哪一项（原值）：权限档 ＝ 此刻那一档；模型 ＝ 上次在这里换成了的那一项（会话事实只给原值，对不上清单）。 */
export function checkedValue(tab: Tab, kind: SwitchKind, local: SwitchLocal): string | null {
  const o = overOf(tab, kind, local);
  if (o) return o.value;
  return kind === "permission" ? tab.permissionMode : null;
}

/** 换完之后：chip 换成哪个字（`null` ＝ 不换）· 要不要 toast（标题是那台那一句；没成又没那一句 ⇒「结果未知」）。 */
export function afterSwitch(kind: SwitchKind, r: Switched): { text: string | null; toast: { title: string; level: "info" | "warn" } | null } {
  if (r.done && r.text !== null) return { text: r.text, toast: null };
  if (r.done) return { text: null, toast: r.said ? { title: r.said, level: "info" } : null };
  return { text: null, toast: { title: r.said ?? WORDS[kind].unsure(), level: "warn" } };
}

export interface SwitchHost {
  /** 那台起会话能选的（`session-open-facts`）。 */
  facts(origin: string): Promise<OpenFacts>;
  /** 交那条命令，结局读成 [`Switched`]（不抛）。 */
  send(tab: Tab, kind: SwitchKind, value: string): Promise<Switched>;
  /** 那台握手时说做不到 `op` ⇒ 那一句；做得到 / 没把握 ⇒ `null`。 */
  unavailable(origin: string, op: string): string | null;
  /** 换成了 ⇒ 重问会话事实。 */
  reread(sid: string): void;
  toast(title: string, detail: string, level: "info" | "warn"): void;
  /** 当前这个 tab（换完回来时它可能已经不是了）。 */
  active(): Tab | null;
}

/** 会话头上那两颗（DOM ＋ 每条会话那一份「正在换 · 读回」）。 */
export class SwitchChips {
  readonly el: HTMLSpanElement;
  private readonly local = new Map<string, SwitchLocal>();
  /** 各台起会话能选的（问到了的那一份；问不到 ⇒ 删掉，下次点开再问）。 */
  private readonly facts = new Map<string, Promise<OpenFacts>>();
  private readonly known = new Map<string, OpenFacts>();
  /** 问过没问到的那几台：重画时不再自己去问（点开时再问）。 */
  private readonly failed = new Set<string>();
  private readonly btn: Record<SwitchKind, HTMLButtonElement>;
  private readonly tips: Record<SwitchKind, string> = { model: "", permission: "" };
  private drawn = "";

  constructor(private readonly host: SwitchHost) {
    this.el = document.createElement("span");
    this.el.className = s.shSw;
    this.btn = { model: this.chip("model"), permission: this.chip("permission") };
    this.el.append(this.btn.model, this.btn.permission);
  }

  private chip(kind: SwitchKind): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = s.shChip;
    b.dataset.switch = kind;
    attachTooltip(b, () => this.tips[kind]);
    b.addEventListener("click", () => {
      const tab = this.host.active();
      if (tab && b.getAttribute("aria-disabled") !== "true") void this.open(tab, kind, b);
    });
    return b;
  }

  private localOf(sid: string): SwitchLocal {
    let l = this.local.get(sid);
    if (!l) {
      l = { pending: null, over: {} };
      this.local.set(sid, l);
    }
    return l;
  }

  /** 照这个 tab 此刻重画两颗（与上次一样就一个 DOM 都不写）。 */
  render(tab: Tab): void {
    const l = this.localOf(tab.sessionId);
    if (!this.failed.has(tab.origin)) void this.factsOf(tab.origin).catch(() => {});
    const got = this.known.get(tab.origin);
    const agent = got ? agentOf(got, tab.agent) : null;
    const count = (k: SwitchKind): number | null => (got ? (agent ? picksOf(agent, k).picks.length : 0) : null);
    const faces = (["model", "permission"] as const).map((k) => [k, chipFace(tab, k, l, this.host.unavailable(tab.origin, SWITCH_OP[k]), count(k))] as const);
    const key = JSON.stringify([tab.sessionId, faces]);
    if (key === this.drawn) return;
    this.drawn = key;
    for (const [k, f] of faces) {
      const b = this.btn[k];
      const t = document.createElement("span");
      t.className = s.shChipText;
      t.textContent = f.text;
      const caret = document.createElement("span");
      caret.className = s.shChipCaret;
      caret.appendChild(icon("caretDown", "compact"));
      b.replaceChildren(...(f.spinning ? [spinner()] : []), t, caret);
      b.setAttribute("aria-label", `${switchTitle(k)}${copyText("kit.text.sep")}${f.text}`);
      if (f.enabled) b.removeAttribute("aria-disabled");
      else b.setAttribute("aria-disabled", "true");
      b.dataset.pending = String(f.spinning);
      this.tips[k] = f.why ? `${switchTitle(k)}${copyText("kit.text.sep")}${f.why}` : f.text === switchTitle(k) ? f.text : `${switchTitle(k)}${copyText("kit.text.sep")}${f.text}`;
    }
  }

  /** 点开：问那台那一家的清单（问过就用那一份）、锚在 chip 下开菜单。 */
  private async open(tab: Tab, kind: SwitchKind, anchor: HTMLElement): Promise<void> {
    let items: MenuItem[];
    try {
      const agent = agentOf(await this.factsOf(tab.origin), tab.agent);
      const { picks, note } = agent ? picksOf(agent, kind) : { picks: [], note: null };
      const on = checkedValue(tab, kind, this.localOf(tab.sessionId));
      items = [
        { label: switchTitle(kind), heading: true },
        ...picks.map((p): MenuItem => ({ label: p.text, checked: p.value === on, onClick: () => void this.pick(tab, kind, p) })),
        ...(note ? [{ label: "", divider: true }, { label: note, heading: true }] : []),
      ];
    } catch (e) {
      items = [{ label: switchTitle(kind), heading: true }, { label: e instanceof Error ? e.message : String(e), enabled: false }];
    }
    if (!anchor.isConnected) return;
    openMenu({ el: anchor, align: "start" }, items);
  }

  /** 选了一项：两颗都不可点、原位转圈；结局回来照 [`afterSwitch`] 换字或出 toast。 */
  private async pick(tab: Tab, kind: SwitchKind, p: AgentPick): Promise<void> {
    const l = this.localOf(tab.sessionId);
    if (l.pending) return;
    l.pending = { kind, pick: p };
    this.repaint(tab.sessionId);
    const r = await this.host.send(tab, kind, p.value);
    l.pending = null;
    const out = afterSwitch(kind, r);
    if (out.text !== null) {
      l.over[kind] = { text: out.text, value: p.value, was: factText(tab, kind) };
      this.host.reread(tab.sessionId);
    }
    if (out.toast) this.host.toast(out.toast.title, tab.title, out.toast.level);
    this.repaint(tab.sessionId);
  }

  /** 那台的清单：问过 ⇒ 那一份；问不到 ⇒ 不留，下次再问。到了 ⇒ 重画（几颗能不能点跟着定）。 */
  private factsOf(origin: string): Promise<OpenFacts> {
    this.failed.delete(origin);
    let p = this.facts.get(origin);
    if (!p) {
      p = this.host.facts(origin).then(
        (f) => {
          this.known.set(origin, f);
          const t = this.host.active();
          if (t && t.origin === origin) this.render(t);
          return f;
        },
        (e: unknown) => {
          this.facts.delete(origin);
          this.failed.add(origin);
          throw e;
        },
      );
      this.facts.set(origin, p);
    }
    return p;
  }

  private repaint(sid: string): void {
    const t = this.host.active();
    if (t && t.sessionId === sid) this.render(t);
  }
}
