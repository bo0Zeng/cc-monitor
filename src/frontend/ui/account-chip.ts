// 状态栏「当前账号」：有当前会话时画本会话那个号，没有 ⇒ 本机的默认号；点了一律开账号面板（`acct-panel.ts`）。
//
// 有当前会话 ⇒ 照会话那台写好的那一份（`session-account`，`appStore.sessionAccount`）画，这里不判、不比、不合；
// 没有会话 ⇒ 本机的默认号。按钮上没有下拉：「新会话默认」只在那台的账号设置里改。
import { deriveUi, defaultAccount, accountColorsActive, type AccountsState } from "./accounts";
import { fetchAccounts } from "./account-reads";
import { accountAvatarEl } from "./account-color";
import { LOCAL, type Machine } from "./ipc/origin";
import { copyText } from "./copy-table";
import { appStore } from "./app-store";
import { attachTooltip } from "./kit/tooltip";
import { icon } from "./kit/icon";
import { acctAvatar, hoverTable } from "./acct-dom";
import { machineLabel } from "./acct-view";
import type { SessionAccountReply } from "./generated/SessionAccountReply";
import s from "./acct.module.css";

// ------------------------------------------------------------ 纯函数（可测）


/** chip 文本（不含图标）。纯函数，据 UI 状态 + 当前默认账号算。 */
export function chipLabel(state: AccountsState | null): string {
  if (!state) return copyText("accountChip.label.noRemote");
  const ui = deriveUi(state);
  switch (ui.kind) {
    case "needs-update":
      return copyText("accountChip.label.backendOld");
    case "query-failed":
      return copyText("accountChip.label.queryFailed");
    case "not-enabled":
      return copyText("accountChip.label.disabled");
    case "ready": {
      const def = defaultAccount(state);
      return def ? def.name : copyText("accountChip.label.disabled");
    }
  }
}

// ------------------------------------------------------------ chip 组件

export interface AccountChipDeps {
  /** 点按钮：开 / 关「账号」面板（没有当前会话 ⇒ `sid` 为 `null`、那台是本机）。 */
  togglePanel?: (sid: string | null, origin: Machine) => void;
}

export class AccountChip {
  readonly element: HTMLButtonElement;
  private labelSpan: HTMLElement;
  private iconEl: HTMLElement;
  /** 无会话时名字前那一格 `默认`。 */
  private prefixEl: HTMLElement;
  /** 本会话那一形：`[⇄] [头像] 名 窗口 用量 [↻重置]`（无会话时藏着）。 */
  private sessionEl: HTMLElement;
  /** 此刻的当前会话（`null` ＝ 没有 tab ⇒ 按钮退成「默认 work」）。 */
  private active: { sid: string; origin: Machine } | null = null;
  private chipModel: SessionAccountReply | null = null;
  /** 没有会话时 chip 看的那台：本机。 */
  private readonly origin: Machine = LOCAL;
  /** `refresh` 跑过一次没有（在那之前 chip 不绑任何一台，`state` 恒 `null`）。 */
  private bound = false;
  /** 读 store 里 chip 绑的那一台（`account-reads.ts` 每取回一次写进去），不自己存一份。本机那一档 not-ready ⇒ `null`（整个隐藏）。 */
  private get state(): AccountsState | null {
    if (!this.bound) return null;
    const st = appStore.accounts.get().get(this.origin) ?? null;
    if (!st || deriveUi(st).kind !== "ready") return null;
    return st;
  }

  constructor(private deps: AccountChipDeps) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = `status-account ${s.acctChip}`;
    this.prefixEl = document.createElement("span");
    this.prefixEl.className = s.acctChipPrefix;
    this.prefixEl.textContent = copyText("resumeSelect.option.defaultBare");
    btn.appendChild(this.prefixEl);
    const iconSpan = document.createElement("span");
    iconSpan.className = "status-account-icon";
    iconSpan.appendChild(icon("account", "compact"));
    iconSpan.setAttribute("aria-hidden", "true");
    btn.appendChild(iconSpan);
    this.iconEl = iconSpan;
    this.labelSpan = document.createElement("span");
    this.labelSpan.className = "status-account-label one-line";
    btn.appendChild(this.labelSpan);
    this.sessionEl = document.createElement("span");
    this.sessionEl.className = s.acctChipSession;
    btn.appendChild(this.sessionEl);
    btn.addEventListener("click", () => this.deps.togglePanel?.(this.active?.sid ?? null, this.active?.origin ?? this.origin));
    // 本会话那一形才有悬停卡（表格，无可点项）；默认那一形只有一个名字，不另给提示。
    attachTooltip(btn, () => this.hoverCard());
    this.element = btn;
    this.element.style.display = "none"; // 拿到数据前先藏
    // 本窗口里任何一次取回（含会话账号刷新器的强制刷新）都经 store 订阅重画，不再只在自己 `refresh` 时画。
    appStore.accounts.subscribe(() => {
      if (this.bound) this.paint();
    });
    // 本会话那一形：会话那台的那一份换了 ⇒ 原地换数。
    appStore.sessionAccount.subscribe(() => {
      if (this.active) this.paint();
    });
  }

  /** 当前会话换了（`tabs.active`）：有 ⇒ 按钮画本会话那个号；没有 ⇒ 退成「默认 work」。 */
  setActive(a: { sid: string; origin: Machine } | null): void {
    if (this.active?.sid === a?.sid && this.active?.origin === a?.origin) return;
    this.active = a;
    this.paint();
  }


  /** 本会话那一形这一刻的样子（没有当前会话 / 一样都不知道 ⇒ `null`）。 */
  private sessionModel(): SessionAccountReply | null {
    const a = this.active;
    if (!a) return null;
    const entry = appStore.sessionAccount.get().get(a.sid);
    return entry && entry.origin === a.origin ? entry.read : null;
  }

  private hoverCard(): HTMLElement | null {
    const a = this.active;
    const m = this.chipModel;
    if (!a || !m) return null;
    // 采样那一格后面接上是哪台采的（问的是哪台、那台叫什么是这一侧的事）。
    const rows = m.hover.map((r, i) => (i === m.sampled ? [...r, machineLabel(a.origin)] : [...r]));
    const tone = m.usage?.tone ?? "plain";
    const tones: Record<number, "refused" | "warn"> = {};
    if (tone !== "plain") tones[1] = tone === "fail" ? "refused" : "warn";
    return hoverTable(m.account, rows, tones);
  }

  /** 画本会话那一形（`m` 由 [`sessionModel`] 算）。 */
  private paintSession(m: SessionAccountReply): void {
    const el = this.sessionEl;
    el.replaceChildren();
    if (m.from !== null) {
      const sw = document.createElement("span");
      sw.className = s.acctChipSwap;
      sw.appendChild(icon("swap", "compact"));
      el.appendChild(sw);
    }
    if (m.account !== null) el.appendChild(acctAvatar(m.account, 16, this.active?.origin));
    const name = document.createElement("span");
    name.className = s.acctChipName;
    name.textContent = m.name;
    el.appendChild(name);
    const u = m.usage;
    if (u?.window !== undefined) {
      const w = document.createElement("span");
      w.className = s.acctChipWindow;
      w.textContent = u.window;
      el.appendChild(w);
    }
    if (u) {
      const v = document.createElement("span");
      v.className = s.acctChipValue;
      v.textContent = u.value;
      el.appendChild(v);
    }
    if (u?.reset !== undefined) {
      const r = document.createElement("span");
      r.className = s.acctChipReset;
      r.textContent = u.reset;
      el.appendChild(r);
    }
    this.element.dataset.shade = u?.tone ?? "plain";
    this.element.dataset.stale = String(m.stale);
    this.element.setAttribute("aria-label", copyText("tabBar.hover.account", { name: m.name }));
  }

  /** 问本机、刷新 chip（初始 / 推来变了 / 手动）。 */
  async refresh(): Promise<void> {
    // 问回来的那一份进 store（`account-reads.ts` 问回时写），`state` 读 store。
    await fetchAccounts(this.origin);
    this.bound = true;
    this.paint();
  }

  /** 按此刻 store 里那一台画 chip（`refresh` 末尾 ＋ store 订阅）。 */
  private paint(): void {
    const m = this.sessionModel();
    this.chipModel = m;
    this.element.dataset.mode = m ? "session" : "default";
    for (const el of [this.prefixEl, this.iconEl, this.labelSpan]) el.style.display = m ? "none" : "";
    this.sessionEl.style.display = m ? "" : "none";
    if (m) {
      this.paintSession(m);
      this.element.style.display = "";
      return;
    }
    delete this.element.dataset.shade;
    delete this.element.dataset.stale;
    this.sessionEl.replaceChildren();
    const st = this.state;
    if (!st) {
      this.element.style.display = "none"; // 本机 not-ready（`D2 阻-7`）/ store 里还没有那一台
      return;
    }
    const text = chipLabel(st);
    if (!text) {
      this.element.style.display = "none"; // 文本为空 → 完全不显示
      return;
    }
    this.labelSpan.textContent = text;
    this.element.setAttribute("aria-label", copyText("acct.chip.ariaDefault", { name: text }));
    // ready 时账号图标换成当前账号的彩色头像（与 tab 徽章同色系）；只有 1 个可选账号时颜色区分不了什么 ⇒ 退回账号图标。
    const cur = defaultAccount(st);
    this.iconEl.replaceChildren(cur && accountColorsActive(st) ? accountAvatarEl(cur.name, { origin: st.origin }) : icon("account", "compact"));
    this.element.style.display = "";
  }
}
