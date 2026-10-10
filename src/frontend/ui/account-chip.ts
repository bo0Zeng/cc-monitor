// 状态栏「当前账号」：全局默认账号的常驻显示 ＋ 一键切换入口；有当前会话时画本会话那个号。
//
// 账号是按机器的：chip 绑第一台已配置的远端（`pickPrimaryOrigin`），一台都没有就是本机。
// 点选账号只改那台的默认账号（非破坏），已有会话不受影响。每一行的徽章带上「这个号走不走 apikey 端点改写」的三态。
import { deriveUi, defaultAccount, accountColorsActive, type AccountsState, type Account } from "./accounts";
import { fetchAccounts, fetchLocalAccounts, invalidateAccountsCache } from "./account-reads";
import { accountsSetDefault } from "./account-ops";
import { accountAvatarEl } from "./account-color";
import { readRemoteConfig, type RemoteHostConfig } from "./remote-config";
import { failToast } from "./kit/toast";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { appStore, putAccounts } from "./app-store";
import { closeMenu, menuAnchoredOn, openMenu, type MenuItem } from "./kit/menu";
import { attachTooltip } from "./kit/tooltip";
import { icon } from "./kit/icon";
import { acctAvatar, hoverTable } from "./acct-dom";
import { sessionChip, sessionHoverRows, type ChipModel } from "./acct-view";
import s from "./acct.module.css";

// ------------------------------------------------------------ 纯函数（可测）

/** 选 chip 绑定的那台机器：第一台已配置远端；一台都没有 ⇒ 本机（`LOCAL_ORIGIN`）。 */
export function pickPrimaryOrigin(hosts: RemoteHostConfig[]): Origin {
  const h = hosts.find((x) => x.label || x.host);
  return h ? h.label || h.host : LOCAL_ORIGIN;
}


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
  /** 打开设置窗口的账号组。 */
  openSettings: () => void;
  /** 有当前会话时点按钮：开 / 关本会话的「账号」面板。 */
  togglePanel?: (sid: string, origin: Origin) => void;
  /** 切完当前账号后回调：让 main.ts 立刻重算会话账号归属，否则最长有 10s 的反向窗口。 */
  onDefaultChanged?: () => void;
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
  private active: { sid: string; origin: Origin } | null = null;
  private chipModel: ChipModel | null = null;
  /** chip 绑的那台机器（`refresh` 之前没意义 —— 那时 `state` 也是 `null`）。 */
  private origin: Origin = LOCAL_ORIGIN;
  /** 这一拍渲染的是本机账号吗（没有远端时回落）。 */
  private local = false;
  /** `refresh` 跑过一次没有（在那之前 chip 不绑任何一台，`state` 恒 `null`）。 */
  private bound = false;
  /** 读 store 里 chip 绑的那一台（`account-reads.ts` 每取回一次写进去），不自己存一份。本机那一档 not-ready ⇒ `null`（整个隐藏）。 */
  private get state(): AccountsState | null {
    if (!this.bound) return null;
    const st = appStore.accounts.get().get(this.origin) ?? null;
    if (this.local && (!st || deriveUi(st).kind !== "ready")) return null;
    return st;
  }

  constructor(private deps: AccountChipDeps) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = `status-account ${s.acctChip}`;
    this.prefixEl = document.createElement("span");
    this.prefixEl.className = s.acctChipPrefix;
    this.prefixEl.textContent = copyText("acct.chip.default");
    btn.appendChild(this.prefixEl);
    const iconSpan = document.createElement("span");
    iconSpan.className = "status-account-icon";
    iconSpan.appendChild(icon("account", "compact"));
    iconSpan.setAttribute("aria-hidden", "true");
    btn.appendChild(iconSpan);
    this.iconEl = iconSpan;
    this.labelSpan = document.createElement("span");
    this.labelSpan.className = "status-account-label";
    btn.appendChild(this.labelSpan);
    this.sessionEl = document.createElement("span");
    this.sessionEl.className = s.acctChipSession;
    btn.appendChild(this.sessionEl);
    btn.addEventListener("click", () => {
      if (this.active && this.chipModel && this.deps.togglePanel) this.deps.togglePanel(this.active.sid, this.active.origin);
      else void this.toggleMenu();
    });
    // 本会话那一形才有悬停卡（表格，无可点项）；默认那一形点开就是下拉，不另给提示。
    attachTooltip(btn, () => this.hoverCard());
    this.element = btn;
    this.element.style.display = "none"; // 拿到数据前先藏
    // 本窗口里任何一次取回（含会话账号刷新器的强制刷新）都经 store 订阅重画，不再只在自己 `refresh` 时画。
    appStore.accounts.subscribe(() => {
      if (this.bound) this.paint();
    });
    // 本会话那一形：额度账 · 会话轮换格 · 会话归属那份快照任何一样变了 ⇒ 原地换数。
    const repaint = (): void => {
      if (this.active) this.paint();
    };
    appStore.quota.subscribe(repaint);
    appStore.sessionRotation.subscribe(repaint);
    appStore.sessionAccounts.subscribe(repaint);
  }

  /** 当前会话换了（`tabs.active`）：有 ⇒ 按钮画本会话那个号；没有 ⇒ 退成「默认 work」。 */
  setActive(a: { sid: string; origin: Origin } | null): void {
    if (this.active?.sid === a?.sid && this.active?.origin === a?.origin) return;
    this.active = a;
    this.paint();
  }

  /** 这个会话在 tab 栏徽章那份快照里归属的号（中转没见过它时只能画这个）。 */
  private fallbackAccount(sid: string): string | null {
    return appStore.sessionAccounts.get()?.rows.find((r) => r.sessionId === sid)?.account ?? null;
  }

  /** 本会话那一形这一刻的样子（没有当前会话 / 一样都不知道 ⇒ `null`）。 */
  private sessionModel(): ChipModel | null {
    const a = this.active;
    if (!a) return null;
    const entry = appStore.sessionRotation.get().get(a.sid);
    return sessionChip(entry, this.fallbackAccount(a.sid));
  }

  private hoverCard(): HTMLElement | null {
    const a = this.active;
    if (!a || !this.chipModel) return null;
    const entry = appStore.sessionRotation.get().get(a.sid);
    const rows = sessionHoverRows(entry, this.fallbackAccount(a.sid), appStore.quota.get().get(a.origin) ?? null);
    if (!rows) return null;
    const tone = this.chipModel.tone;
    const tones: Record<number, "refused" | "warn"> = {};
    if (tone !== "plain") tones[1] = tone === "fail" ? "refused" : "warn";
    return hoverTable(this.chipModel.account, rows, tones);
  }

  /** 画本会话那一形（`m` 由 [`sessionModel`] 算）。 */
  private paintSession(m: ChipModel): void {
    const el = this.sessionEl;
    el.replaceChildren();
    if (m.swapped) {
      const sw = document.createElement("span");
      sw.className = s.acctChipSwap;
      sw.appendChild(icon("swap", "compact"));
      el.appendChild(sw);
    }
    if (m.account !== null) el.appendChild(acctAvatar(m.account));
    const name = document.createElement("span");
    name.className = s.acctChipName;
    name.textContent = m.name;
    el.appendChild(name);
    if (m.window !== null) {
      const w = document.createElement("span");
      w.className = s.acctChipWindow;
      w.textContent = m.window;
      el.appendChild(w);
    }
    if (m.value !== null) {
      const v = document.createElement("span");
      v.className = s.acctChipValue;
      v.textContent = m.value;
      el.appendChild(v);
    }
    if (m.reset !== null) {
      const r = document.createElement("span");
      r.className = s.acctChipReset;
      r.textContent = m.reset;
      el.appendChild(r);
    }
    this.element.dataset.shade = m.tone;
    this.element.dataset.stale = String(m.stale);
    this.element.setAttribute("aria-label", copyText("acct.chip.aria", { name: m.name }));
  }

  /** 拉数据刷新 chip（初始 / 设置变更 / 手动）。force 透传给缓存。 */
  async refresh(force = false): Promise<void> {
    try {
      const cfg = await readRemoteConfig();
      this.origin = cfg.hosts.some((h) => h.connect) ? pickPrimaryOrigin(cfg.hosts.filter((h) => h.connect)) : LOCAL_ORIGIN;
    } catch {
      this.origin = LOCAL_ORIGIN;
    }
    // 没有远端不等于没有账号 ⇒ 回落到本机那一份，并把「走不走 apikey 端点改写」一起问出来。
    this.local = isLocalOrigin(this.origin);
    // 取回来的那一份进 store（`account-reads.ts` 取回时已写；同一份再写是空操作），`state` 读 store。
    putAccounts(this.origin, this.local ? await fetchLocalAccounts(force) : await fetchAccounts(this.origin, force));
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
    this.iconEl.replaceChildren(cur && accountColorsActive(st) ? accountAvatarEl(cur.name) : icon("account", "compact"));
    this.element.style.display = "";
  }

  /** 给快捷键用的入口：chip 隐藏时合成 `click()` 照样派发，会开出一个飘到视口外、看不见却吞点击的菜单 ⇒ 这里显式挡住。 */
  async openMenu(): Promise<void> {
    if (this.element.style.display === "none") return;
    await this.toggleMenu();
  }

  /**
   * 「新会话默认」那个下拉锚到别处（账号面板底栏）：那台是 chip 绑的这台 ⇒ 同一份；别的台 ⇒ 现取那台的账号清单。
   */
  async openDefaultMenu(anchor: HTMLElement, origin: Origin): Promise<void> {
    if (origin !== this.origin) {
      const st = isLocalOrigin(origin) ? await fetchLocalAccounts() : await fetchAccounts(origin);
      putAccounts(origin, st);
    }
    await this.toggleMenu(anchor, origin);
  }

  /** 那台的新会话默认是哪个号（面板底栏那颗按钮上的字）；store 里还没有那台 ⇒ 现取一次（取回来面板随 store 重画）、这一次 `null`。 */
  defaultOf(origin: Origin): string | null {
    if (!appStore.accounts.get().has(origin)) {
      void (isLocalOrigin(origin) ? fetchLocalAccounts() : fetchAccounts(origin)).then((st) => putAccounts(origin, st)).catch(() => putAccounts(origin, null));
      return null;
    }
    const st = appStore.accounts.get().get(origin) ?? null;
    return st && deriveUi(st).kind === "ready" ? (defaultAccount(st)?.name ?? null) : null;
  }

  private async toggleMenu(anchor: HTMLElement = this.element, origin: Origin = this.origin): Promise<void> {
    if (menuAnchoredOn(anchor)) {
      closeMenu();
      return;
    }
    // 门问「有没有状态」（本机那一档没有远端但有账号）。与 [`snapshotReady`] 同源（`accountPickerState()`），别再分开写。
    const st = origin === this.origin ? this.accountPickerState() : (appStore.accounts.get().get(origin) ?? null);
    if (!st) return;
    const ui = deriveUi(st);
    const items: MenuItem[] = [];
    if (ui.kind !== "ready") {
      // 未启用 / 需更新：只给一条「去设置 / 管理」。
      const info =
        ui.kind === "needs-update"
          ? ui.reason
          : ui.kind === "query-failed"
            ? copyText("accountChip.menu.queryFailed", { reason: ui.reason })
            : copyText("accountChip.menu.notEnabled");
      items.push({ label: info, enabled: false }, { label: copyText("accountChip.menu.manageDeploy"), onClick: () => this.deps.openSettings() });
    } else {
      const def = defaultAccount(st);
      items.push({ label: copyText("acct.foot.default"), enabled: false });
      for (const a of ui.accounts) items.push(this.accountItem(a, def?.name === a.name, origin));
      items.push(
        { label: "", divider: true },
        { label: copyText("accountChip.menu.manage"), onClick: () => this.deps.openSettings() },
        {
          label: copyText("accountChip.menu.refresh"),
          onClick: () => {
            // 账号缓存的键就是 origin ⇒ 只清这一台。
            invalidateAccountsCache(origin);
            void this.refresh(true);
          },
        },
      );
    }
    openMenu({ el: anchor, align: "end" }, items, { label: copyText("accountChip.menu.label") });
  }

  /**
   * 一个账号那一项：当前的打勾、头像、邮箱 ＋ 徽章（那台后端写好的 `badge`，`accounts_query::badge_of`）；
   * 每一项带上「走不走 apikey 端点改写」的三态（问不到 routing 时不表态）。
   */
  private accountItem(a: Account, isCurrent: boolean, origin: Origin): MenuItem {
    const selectable = a.selectable;
    const badge = a.badge;
    // 用量（`5h 63%` · 用满 `5h ✕ ↻19:00` · 被拒 `5h 58% · 被拒 ↻19:00` · `按量`）：那台额度账上有这个号才写；没有 ⇒ 照旧写登录态。
    const led = appStore.quota.get().get(origin)?.accounts.find((x) => x.account === a.name);
    const u = led && badge.warn !== true ? led.usage : null;
    const usage = u ? u.text : null;
    return {
      label: a.name,
      checked: isCurrent,
      avatar: accountAvatarEl(a.name, { size: 16 }),
      note: a.email || undefined,
      detail: usage ?? badge.text,
      detailTone: (u && u.tone !== "plain") || badge.warn ? "warn" : undefined,
      title: badge.title,
      enabled: selectable,
      onClick: selectable && !isCurrent ? () => void this.selectDefault(a, origin) : undefined,
    };
  }

  /**
   * 「这个 chip 现在有没有一份可以列出来的账号」只许有一份判断：[`toggleMenu`] 与 [`snapshotReady`] 都经这里，
   * 否则会出现 chip 菜单里能切号、Ctrl+K 命令面板却拿到 `null` 的两个答案。
   * 不管 ready 不 ready（调用方各自按需判）；回状态而不回 `boolean`，免得调用方再各自判一次 `!this.state`。
   */
  private accountPickerState(): AccountsState | null {
    return this.state;
  }

  /**
   * 同步快照当前 ready 账号（供 Ctrl+K 命令同步读）；非 ready ⇒ null。门与 [`toggleMenu`] 同源（[`accountPickerState`]）。
   * 快照来自本机那一半时 `origin` 是 `LOCAL_ORIGIN`（消费方不读它，留着让人看得出是哪一半）。
   */
  snapshotReady(): { origin: Origin; accounts: Account[]; defaultName: string | null } | null {
    const st = this.accountPickerState();
    if (!st) return null;
    const ui = deriveUi(st);
    if (ui.kind !== "ready") return null;
    return { origin: this.origin, accounts: ui.accounts, defaultName: defaultAccount(st)?.name ?? null };
  }

  /** 按名字切默认账号（供 Ctrl+K 命令；找不到/不可选则忽略）。 */
  async applyDefaultByName(name: string): Promise<void> {
    const snap = this.snapshotReady();
    const a = snap?.accounts.find((x) => x.name === name);
    if (a?.selectable) await this.selectDefault(a);
  }

  private async selectDefault(a: Account, origin: Origin = this.origin): Promise<void> {
    this.closeMenu();
    try {
      await accountsSetDefault(origin, a.name);
      // 默认账号住那台的账号库清单：只清这一台的缓存。
      invalidateAccountsCache(origin);
      if (origin === this.origin) await this.refresh(true);
      else putAccounts(origin, isLocalOrigin(origin) ? await fetchLocalAccounts(true) : await fetchAccounts(origin, true));
      // 立刻重算会话账号（否则要等下一拍 10s 轮询，期间「对齐」会把会话打回刚被切走的旧账号）。
      this.deps.onDefaultChanged?.();
      // 选即生效、不另报：下拉合上、按钮上的字就是结果。
    } catch (e) {
      failToast(copyText("accountChip.selectDefault.failed"), e, { level: "error" });
    }
  }

  private closeMenu(): void {
    closeMenu();
  }
}
