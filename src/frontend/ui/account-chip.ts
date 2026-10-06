// A3：状态栏「当前账号」chip —— 全局默认账号的常驻显示 + 一键切换入口。
//
// 账号是 per-origin（每台远端一个 manifest）。cc-monitor 通常只连一台常用远端，
// 故 chip 绑「第一台可用远端」(pickPrimaryOrigin)，选单里若有多台再让用户切台。
// 点选账号 = **只改本机默认账号**（非破坏，DESIGN §2 的①），toast 明说已有会话不受影响。
//
// ★★ `K-H2b` `D1 阻-4`〔08-28 订正**两句假话**〕：本文件先前有两处注释写着
// 「远端全关掉时 `origin` 是 `null`，这里渲染的就是本机账号」——**那是假的**。
// `D1` 现打、PM 复核：`fetchAccounts` 只 `invoke("list_remote_accounts")`，本机那条是
// **另一个函数** `fetchLocalAccounts`，而它在本文件里当时命中 **0**
// ⇒ **chip 一次都没渲染过本机账号**（`origin` 为 `null` 时它整个隐藏、直接 `return`）。
// ⚠ 那两句假话不是笔误，是**从上一轮报告里抄来没量的**（`己1-f34` 记着这一条）。
//
// ★★ `D1 阻-5`：现在它**真的会**渲染本机账号 —— 没有远端时回落到
// `fetchLocalAccounts`；每一行的徽章带上「这个号走不走 apikey 端点改写」的三态
// （chip 那一台答的两格事实，本机远端同一条路）。
import { apikeyEndpointStateFor, deriveUi, currentWorkingAccount, accountColorsActive, isSelectable, accountStatusBadge, type AccountsState, type Account } from "./accounts";
import { fetchAccounts, fetchLocalAccounts, fetchMachineApikeyRouting, invalidateAccountsCache } from "./account-reads";
import { accountsSetDefault } from "./account-ops";
import type { ApikeyRoutingView } from "./apikey-reads";
import { accountAvatarEl } from "./account-color";
import { readRemoteConfig, type RemoteHostConfig } from "./remote-config";
import { toast } from "./kit/toast";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { appStore, putAccounts } from "./app-store";
import { closeMenu, menuAnchoredOn, openMenu, type MenuItem } from "./kit/menu";
import { attachTooltip } from "./kit/tooltip";
import { icon } from "./kit/icon";
import { acctAvatar, hoverTable } from "./acct-dom";
import { sessionChip, sessionHoverRows, usageOf, usageText, type ChipModel } from "./acct-view";
import s from "./acct.module.css";

// ------------------------------------------------------------ 纯函数（可测）

/** 选 chip 绑定的那台机器：第一台已配置远端；一台都没有 ⇒ **本机**（`LOCAL_ORIGIN`）。
 *  ⚠ `K-R59` 之前这里还排掉 `daemonless` 的主机 —— 那一档没了，不再有可排的。
 * 上一版「无 → `null`」，调用方再把 `null` 读成「回落本机」——
 *  两步说同一件事；`D1 阻-5` 之后「没有远端」就是「本机」，这里直接说出来。 */
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
      const def = currentWorkingAccount(state);
      return def ? def.name : copyText("accountChip.label.disabled");
    }
  }
}

// ------------------------------------------------------------ chip 组件

export interface AccountChipDeps {
  /** 打开设置窗口的账号组（A3 设置组落地后接线；先给个跳设置的回调）。 */
  openSettings: () => void;
  /** 有当前会话时点按钮：开 / 关本会话的「账号」面板。 */
  togglePanel?: (sid: string, origin: Origin) => void;
  /** F1：切完当前账号后回调——让 main.ts 立刻重算会话账号归属，否则会有最长 10s 的反向窗口。
   *  （chip 是纯全局切换器，只列账号点选切当前账号；批量对齐随 F09 一并删除，不在任何地方。） */
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
  /** `D1 阻-5`：这一拍渲染的是**本机**账号吗（没有远端时回落）。 */
  private local = false;
  /** `D1 阻-5`：本机那几个 configDir 走不走 apikey 端点改写。`null` = 没问到（远端那半恒 `null`）。 */
  private apikeyRouting: ApikeyRoutingView | null = null;
  /** `refresh` 跑过一次没有（在那之前 chip 不绑任何一台，`state` 恒 `null`）。 */
  private bound = false;
  /**
   * chip 不再自己存一份账号清单：读 store 里它绑的那一台（`account-reads.ts` 每取回一次写进去）。
   * 本机那一档 not-ready ⇒ `null`（整个隐藏，`D2 阻-7`）。
   */
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
    iconSpan.textContent = copyText("accountChip.ctor.icon");
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
    return sessionChip(entry, this.fallbackAccount(a.sid), appStore.quota.get().get(a.origin) ?? null);
  }

  private hoverCard(): HTMLElement | null {
    const a = this.active;
    if (!a || !this.chipModel) return null;
    const entry = appStore.sessionRotation.get().get(a.sid);
    const rows = sessionHoverRows(entry, this.fallbackAccount(a.sid), appStore.quota.get().get(a.origin) ?? null);
    if (!rows) return null;
    const tone = this.chipModel.tone;
    const tones: Record<number, "refused" | "warn"> = {};
    if (tone !== "neutral") tones[1] = tone === "refused" ? "refused" : "warn";
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
    // ★ `D1 阻-5`：**没有远端不等于没有账号** —— 本机那份账号清单
    //   一直在，只是此前没有任何界面渲染它（`fetchLocalAccounts` 全仓生产调用方只有
    //   fork 那个小窗）。⇒ 回落到本机那一份，并把「走不走 apikey 端点改写」一起问出来。
    this.local = isLocalOrigin(this.origin);
    this.apikeyRouting = null;
    // 取回来的那一份进 store（`account-reads.ts` 取回时已写；同一份再写是空操作），`state` 读 store。
    putAccounts(this.origin, this.local ? await fetchLocalAccounts(force) : await fetchAccounts(this.origin, force));
    this.bound = true;
    // ★★ `D2 阻-7`：**本机那一档 not-ready 就整个隐藏** —— 与本件之前**逐字节相同**（`state` getter 回 `null`，`paint` 藏）。
    //
    // 不加这一格的话，「没有远端 + 本机也没有 accounts.json」会从「整个隐藏」变成
    // chip 显出来、菜单里写一句**远端口吻的假话**「该远端尚未启用多账号」——
    // 而那台「远端」根本不存在。⇒ 本件只该**加**「本机有账号时能看见」，
    // 不该**改**「什么都没有时看不见」。
    if (this.state) {
      // 只问**说得出 configDir** 的那几个（账号 0 没有目录 ⇒ 推不出 key 表里的 id）。问的是 chip 这一台（本机远端同一条路）。
      const dirs = this.state.accounts
        .map((a) => a.configDir)
        .filter((d): d is string => typeof d === "string" && d.length > 0);
      try {
        this.apikeyRouting = dirs.length ? await fetchMachineApikeyRouting(this.origin, dirs) : null;
      } catch {
        // 问不到就**不表态** —— 徽章回落到「只说条件、不下判断」那一档，不猜。
        this.apikeyRouting = null;
      }
    }
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
    // account-ux U4：ready 时把 👤 换成当前账号的彩色头像（与 tab 徽章同色系 → 肉眼可对应）。
    // U8 休眠：只有 1 个可选账号时颜色区分不了任何东西 → 退回 👤，等加了第二个号再点亮。
    const cur = currentWorkingAccount(st);
    this.iconEl.textContent = "";
    if (cur && accountColorsActive(st)) {
      this.iconEl.appendChild(accountAvatarEl(cur.name));
    } else {
      this.iconEl.textContent = copyText("accountChip.refresh.icon");
    }
    this.element.style.display = "";
  }

  /** account-ux U8：给快捷键用的显式入口。合成 `element.click()` 在 chip 隐藏时照样会派发，
   *  能开出一个 getBoundingClientRect() 全 0、飘到视口外的菜单（看不见却吞点击）——
   *  今天靠 chipLabel 恒非空才碰不到，那是巧合不是设计。这里显式挡住。 */
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
    return st && deriveUi(st).kind === "ready" ? (currentWorkingAccount(st)?.name ?? null) : null;
  }

  private async toggleMenu(anchor: HTMLElement = this.element, origin: Origin = this.origin): Promise<void> {
    if (menuAnchoredOn(anchor)) {
      closeMenu();
      return;
    }
    // `D1 阻-5`：**本机那一档没有 origin，但有账号** ⇒ 这道门改问「有没有状态」。
    //   ⚠ 这道门先前还替**用量探针**（`loadCurrentAccountUsage`）守着 origin ——
    //   探针已随退役，那半个理由跟着没了；这道门今天只为「列不列得出账号」。
    //   🔴 而 [`snapshotReady`] 先前也留在 origin 那道门后面，`D4 阻-4` 查实那是个洞：
    //   **chip 显示、菜单里能切号，而 Ctrl+K 命令面板拿到 `null`** —— 同一件事两个答案，
    //   而且静默。⇒ 它已经与本行**同源**（`accountPickerState()`），别再把两处分开写。
    const st = origin === this.origin ? this.accountPickerState() : (appStore.accounts.get().get(origin) ?? null);
    if (!st) return;
    const ui = deriveUi(st);
    const items: MenuItem[] = [];
    if (ui.kind !== "ready") {
      // 未启用 / 需更新：只给一条「去设置 / 管理」。
      const info =
        ui.kind === "needs-update"
          ? copyText("accountChip.menu.backendOld")
          : ui.kind === "query-failed"
            ? copyText("accountChip.menu.queryFailed", { reason: ui.reason })
            : copyText("accountChip.menu.notEnabled");
      items.push({ label: info, enabled: false }, { label: copyText("accountChip.menu.manageDeploy"), onClick: () => this.deps.openSettings() });
    } else {
      const def = currentWorkingAccount(st);
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
   * 一个账号那一项：当前的打勾、头像、邮箱 ＋ 登录态（取值只住 `accounts.ts::accountStatusBadge`，与设置里那张账号表同源）；
   * 每一项带上「走不走 apikey 端点改写」的三态（问不到 routing 时不表态）。
   */
  private accountItem(a: Account, isCurrent: boolean, origin: Origin): MenuItem {
    const selectable = isSelectable(a);
    const badge = accountStatusBadge(a, this.apikeyRouting && origin === this.origin ? apikeyEndpointStateFor(a, this.apikeyRouting) : undefined);
    // 用量（`5h 63%` · 被拒 `5h ✕ ↻19:00` · `按量`）：那台额度账上有这个号才写；没有 ⇒ 照旧写登录态。
    const led = appStore.quota.get().get(origin)?.accounts.find((x) => x.account === a.name);
    const now = appStore.quota.get().get(origin)?.now ?? 0;
    const u = led && badge.warn !== true ? usageOf(led, led.reading?.resetsAt, now, -new Date(now * 1000).getTimezoneOffset()) : null;
    const usage = u ? usageText(u) : null;
    return {
      label: a.name,
      checked: isCurrent,
      avatar: accountAvatarEl(a.name, { size: 16 }),
      note: a.email || undefined,
      detail: usage ?? badge.text,
      detailTone: (u && u.tone !== "neutral") || badge.warn ? "warn" : undefined,
      title: badge.title,
      enabled: selectable,
      onClick: selectable && !isCurrent ? () => void this.selectDefault(a, origin) : undefined,
    };
  }

  /**
   * ★★ `D4 阻-4`：**「这个 chip 现在有没有一份可以列出来的账号」只许有一份判断。**
   *
   * 先前 [`toggleMenu`] 与 [`snapshotReady`] 各写各的：前者 `D1 阻-5` 放宽成
   * 「有远端 **或** 本机那一档」，后者留在 `!this.origin` 那道旧门后面。
   * ⇒ **本机 ready 那一档：chip 显示、菜单里能切号，而 Ctrl+K 命令面板拿到 `null`。**
   * 两处对同一件事给两个答案，**而且静默** —— 正是 `KA6a` 第一轮栽过的「同职两处不同源」。
   *
   * ⚠ 它**不管** ready 不 ready（那是 `deriveUi` 的活，两个调用方各自按需要判）：
   * 本函数只答「渲染这份账号列表的前置条件成立吗」，成立就把那份状态一起交出去。
   * ★ **回状态而不是回 `boolean`** 是承重的：回 `boolean` 时两个调用方还得各自再写一次
   * `!this.state`，那就又是两份判断 —— 而这一条治的正是「同一件事两处各判一次」。
   */
  private accountPickerState(): AccountsState | null {
    // 上一版这里先挡「`origin` 为 `null` 且不是本机」—— 那一档只在 `refresh` 之前出现，
    //   而那时 `state` 本来就是 `null`；`origin` 不再为 `null` 之后那道门与下一行说的是同一件事。
    return this.state;
  }

  /**
   * 同步快照当前 ready 账号（供 Ctrl+K buildCommands 同步读缓存）。非 ready → null。
   *
   * 🔴 `D4 阻-4`：这道门**已与 [`toggleMenu`] 同源**（[`accountPickerState`]）——
   * 本机那一档从此也回得出一份，命令面板里列得出 chip 菜单里列得出的那几个号。
   *
   * ⚠ 快照来自**本机**那一半时 `origin` 是 `LOCAL_ORIGIN`（上一版是 `null`）。
   * 今天唯一的消费方 `buildAccountCommands` 只读 `accounts` / `defaultName`
   * （`account-commands.ts::AccountCommandsInput` 逐字，它连 `origin` 这个键都没有）
   * ⇒ 留着这个字段是为了让读的人看得出这份快照是哪一半的。
   */
  snapshotReady(): { origin: Origin; accounts: Account[]; defaultName: string | null } | null {
    const st = this.accountPickerState();
    if (!st) return null;
    const ui = deriveUi(st);
    if (ui.kind !== "ready") return null;
    return { origin: this.origin, accounts: ui.accounts, defaultName: currentWorkingAccount(st)?.name ?? null };
  }

  /** 按名字切默认账号（供 Ctrl+K 命令；找不到/不可选则忽略）。 */
  async applyDefaultByName(name: string): Promise<void> {
    const snap = this.snapshotReady();
    const a = snap?.accounts.find((x) => x.name === name);
    if (a && isSelectable(a)) await this.selectDefault(a);
  }

  private async selectDefault(a: Account, origin: Origin = this.origin): Promise<void> {
    this.closeMenu();
    try {
      await accountsSetDefault(origin, a.name);
      // 默认账号住那台的账号库清单：只清这一台的缓存。
      invalidateAccountsCache(origin);
      if (origin === this.origin) await this.refresh(true);
      else putAccounts(origin, isLocalOrigin(origin) ? await fetchLocalAccounts(true) : await fetchAccounts(origin, true));
      // 立刻重算会话账号/⚠k（否则 currentByOrigin 要等下一拍 10s 轮询，期间"对齐"会把会话
      // 打回刚被切走的旧账号——与用户意图正好相反）。
      this.deps.onDefaultChanged?.();
      // 选即生效、不另报：下拉合上、按钮上的字就是结果。
    } catch (e) {
      toast(copyText("accountChip.selectDefault.failed"), String(e), { level: "error" });
    }
  }

  private closeMenu(): void {
    closeMenu();
  }
}
