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
// ⚠ 那两句假话不是笔误，是**从上一轮报告里抄来没量的**（`ROADMAP` `己1-f34` 记着这一条）。
//
// ★★ `D1 阻-5`〔08-28〕：现在它**真的会**渲染本机账号 —— 没有远端时回落到
// `fetchLocalAccounts`，并且那几行的徽章带上「这个号走不走本机中转」的三态
// （`accountStatusBadge` 的 `{scope:"local",…}`）。在此之前 `KH2B7` 那三态
// **在用户看得见的地方一处都没落地**（两个取值函数生产调用方各 0）。
import {
  fetchAccounts,
  fetchLocalAccounts,
  fetchLocalRelayRouting,
  localRelayStateFor,
  type RelayRoutingView,
  deriveUi,
  currentWorkingAccount,
  accountColorsActive,
  isSelectable,
  accountStatusBadge,
  setDefaultName,
  invalidateAccountsCache,
  type AccountsState,
  type Account,
} from "./accounts";
import { accountAvatarEl } from "./account-color";
import { readRemoteConfig, type RemoteHostConfig } from "./remote-config";
import { showActionFailureToast } from "./error-toast";

// ------------------------------------------------------------ 纯函数（可测）

/** 选 chip 绑定的"主远端"：第一台已配置远端。无 → null（chip 隐藏）。
 *  ⚠ `K-R59` 之前这里还排掉 `daemonless` 的主机 —— 那一档没了，不再有可排的。 */
export function pickPrimaryOrigin(hosts: RemoteHostConfig[]): string | null {
  const h = hosts.find((x) => x.label || x.host);
  return h ? h.label || h.host : null;
}


/** chip 文本（不含图标）。纯函数，据 UI 状态 + 当前默认账号算。 */
export function chipLabel(state: AccountsState | null): string {
  if (!state) return "未连远端";
  const ui = deriveUi(state);
  switch (ui.kind) {
    case "needs-update":
      return "daemon 需更新";
    case "not-enabled":
      return "未启用";
    case "ready": {
      const def = currentWorkingAccount(state);
      return def ? def.name : "未启用";
    }
  }
}

// ------------------------------------------------------------ chip 组件

export interface AccountChipDeps {
  /** 打开设置窗口的账号组（A3 设置组落地后接线；先给个跳设置的回调）。 */
  openSettings: () => void;
  /** F1：切完当前账号后回调——让 main.ts 立刻重算会话账号归属，否则会有最长 10s 的反向窗口。
   *  （chip 是纯全局切换器，只列账号点选切当前账号；批量对齐随 F09 一并删除，不在任何地方。） */
  onDefaultChanged?: () => void;
}

export class AccountChip {
  readonly element: HTMLButtonElement;
  private labelSpan: HTMLElement;
  private iconEl: HTMLElement;
  private origin: string | null = null;
  /** `D1 阻-5`：这一拍渲染的是**本机**账号吗（没有远端时回落）。 */
  private local = false;
  /** `D1 阻-5`：本机那几个 configDir 走不走中转。`null` = 没问到（远端那半恒 `null`）。 */
  private relayRouting: RelayRoutingView | null = null;
  private state: AccountsState | null = null;
  private menu: HTMLElement | null = null;
  private menuClose: ((e: Event) => void) | null = null;

  constructor(private deps: AccountChipDeps) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "status-account";
    btn.title = "当前账号（点击切换 / 管理）";
    const icon = document.createElement("span");
    icon.className = "status-account-icon";
    icon.textContent = "👤";
    icon.setAttribute("aria-hidden", "true");
    btn.appendChild(icon);
    this.iconEl = icon;
    this.labelSpan = document.createElement("span");
    this.labelSpan.className = "status-account-label";
    btn.appendChild(this.labelSpan);
    btn.addEventListener("click", () => void this.toggleMenu());
    this.element = btn;
    this.element.style.display = "none"; // 拿到数据前先藏
  }

  /** 拉数据刷新 chip（初始 / 设置变更 / 手动）。force 透传给缓存。 */
  async refresh(force = false): Promise<void> {
    try {
      const cfg = await readRemoteConfig();
      this.origin = cfg.enabled ? pickPrimaryOrigin(cfg.hosts) : null;
    } catch {
      this.origin = null;
    }
    // ★ `D1 阻-5`：**没有远端不等于没有账号** —— 本机 `~/.claude-accts/` 那份 manifest
    //   一直在，只是此前没有任何界面渲染它（`fetchLocalAccounts` 全仓生产调用方只有
    //   fork 那个小窗）。⇒ 回落到本机那一份，并把「走不走中转」一起问出来。
    this.local = !this.origin;
    this.relayRouting = null;
    this.state = this.local
      ? await fetchLocalAccounts(force)
      : await fetchAccounts(this.origin as string, force);
    // ★★ `D2 阻-7`：**本机那一档 not-ready 就整个隐藏** —— 与本件之前**逐字节相同**。
    //
    // 不加这一格的话，「没有远端 + 本机也没有 accounts.json」会从「整个隐藏」变成
    // chip 显出来、菜单里写一句**远端口吻的假话**「该远端尚未启用多账号」——
    // 而那台「远端」根本不存在。⇒ 本件只该**加**「本机有账号时能看见」，
    // 不该**改**「什么都没有时看不见」。
    if (this.local && (!this.state || deriveUi(this.state).kind !== "ready")) {
      this.state = null;
      this.relayRouting = null;
      this.element.style.display = "none";
      return;
    }
    if (this.local && this.state) {
      // 只问**说得出 configDir** 的那几个（账号 0 没有目录 ⇒ 推不出中转表里的 id）。
      const dirs = this.state.accounts
        .map((a) => a.configDir)
        .filter((d): d is string => typeof d === "string" && d.length > 0);
      try {
        this.relayRouting = dirs.length ? await fetchLocalRelayRouting(dirs) : null;
      } catch {
        // 问不到就**不表态** —— 徽章回落到「只说条件、不下判断」那一档，不猜。
        this.relayRouting = null;
      }
    }
    const text = chipLabel(this.state);
    if (!text) {
      this.element.style.display = "none"; // 文本为空 → 完全不显示
      return;
    }
    this.labelSpan.textContent = text;
    // account-ux U4：ready 时把 👤 换成当前账号的彩色头像（与 tab 徽章同色系 → 肉眼可对应）。
    // U8 休眠：只有 1 个可选账号时颜色区分不了任何东西 → 退回 👤，等加了第二个号再点亮。
    const cur = currentWorkingAccount(this.state);
    this.iconEl.textContent = "";
    if (cur && accountColorsActive(this.state)) {
      this.iconEl.appendChild(accountAvatarEl(cur.name));
    } else {
      this.iconEl.textContent = "👤";
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

  private async toggleMenu(): Promise<void> {
    if (this.menu) {
      this.closeMenu();
      return;
    }
    // `D1 阻-5`：**本机那一档没有 origin，但有账号** ⇒ 这道门改问「有没有状态」。
    //   ⚠ 这道门先前还替**用量探针**（`loadCurrentAccountUsage`）守着 origin ——
    //   探针已随 `设计/50` 退役，那半个理由跟着没了；这道门今天只为「列不列得出账号」。
    //   🔴 而 [`snapshotReady`] 先前也留在 origin 那道门后面，`D4 阻-4` 查实那是个洞：
    //   **chip 显示、菜单里能切号，而 Ctrl+K 命令面板拿到 `null`** —— 同一件事两个答案，
    //   而且静默。⇒ 它已经与本行**同源**（`accountPickerState()`），别再把两处分开写。
    const st = this.accountPickerState();
    if (!st) return;
    const ui = deriveUi(st);
    const menu = document.createElement("div");
    menu.className = "account-picker";

    if (ui.kind !== "ready") {
      // 未启用 / 需更新：只给一条"去设置/管理"
      const info = document.createElement("div");
      info.className = "account-picker-info";
      info.textContent =
        ui.kind === "needs-update"
          ? "远端 daemon 需要更新才能用多账号"
          : "该远端尚未启用多账号";
      menu.appendChild(info);
      menu.appendChild(this.menuAction("管理 / 部署…", () => this.deps.openSettings()));
    } else {
      const def = currentWorkingAccount(st);
      // F1：chip 是纯全局切换器——只列账号点选切当前账号；批量对齐随 F09 一并删除。
      for (const a of ui.accounts) {
        menu.appendChild(this.accountRow(a, def?.name === a.name));
      }
      const sep = document.createElement("div");
      sep.className = "account-picker-sep";
      menu.appendChild(sep);
      menu.appendChild(this.menuAction("管理账号…", () => this.deps.openSettings()));
      menu.appendChild(
        this.menuAction("刷新", () => {
          invalidateAccountsCache(this.origin ?? undefined);
          void this.refresh(true);
        }),
      );
    }

    const r = this.element.getBoundingClientRect();
    menu.style.bottom = `${Math.max(4, window.innerHeight - r.top + 4)}px`;
    menu.style.right = `${Math.max(4, window.innerWidth - r.right)}px`;
    document.body.appendChild(menu);
    this.menu = menu;
    // 照 SFTP host-picker：Esc / 外部 pointerdown 关，下一拍挂监听防自关
    const close = (ev: Event): void => {
      if (ev instanceof KeyboardEvent && ev.key !== "Escape") return;
      if (ev.type === "pointerdown" && menu.contains(ev.target as Node)) return;
      this.closeMenu();
    };
    this.menuClose = close;
    setTimeout(() => {
      if (this.menu !== menu) return;
      document.addEventListener("pointerdown", close);
      document.addEventListener("keydown", close);
    }, 0);
  }

  private accountRow(a: Account, isCurrent: boolean): HTMLElement {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "account-picker-item";
    const selectable = isSelectable(a);
    if (!selectable) row.classList.add("disabled");
    if (isCurrent) row.classList.add("current");

    const mark = document.createElement("span");
    mark.className = "account-picker-mark";
    mark.textContent = isCurrent ? "●" : "○";
    row.appendChild(mark);

    row.appendChild(accountAvatarEl(a.name, { size: 16 }));

    const name = document.createElement("span");
    name.className = "account-picker-name";
    name.textContent = a.name;
    row.appendChild(name);

    const email = document.createElement("span");
    email.className = "account-picker-email";
    email.textContent = a.email || "";
    row.appendChild(email);

    const status = document.createElement("span");
    status.className = "account-picker-status";
    // K-A1（第二轮）：三态（逃生口 / api-key（未配置端点）/ 未登录 / 已登录）的取值
    // **只住** `accounts.ts::accountStatusBadge` —— 这里不再自己判。
    //
    // 为什么：这一段与设置里那张账号表（`settings/accounts-section.ts:661-672`）是**同职两处**，
    // 渲染的是同一个概念（一个账号的登录态）。第一轮只改了设置那侧，结果 `KA6a` 那段文案
    // 只堵了一半 —— api-key 号在**本菜单**里仍显示「已登录」，而那正是 `KA6a` 点名的坏体验。
    // 现在两处同源，由 `account-availability-guard.vitest.ts` 钉住「不许再开第三处」。
    //
    // ⚠ 两处文案与第二轮替换**前**逐字不同，如实记在这里：
    //   ① 订阅号缺凭据：「未登录 ⚠」→「未登录」+ `.warn` 类。
    //      ★ **第三轮已裁：警示走 CSS，不进文本。** 上一轮把这一格写成「无解」，
    //      那只在「⚠ 必须住在**文本**里」这个前提下成立 —— 把它挪到 CSS，冲突就没了：
    //      语义住布尔（`accountStatusBadge` 的 `warn`）、呈现住 CSS
    //      （`.account-picker-status.warn` —— 本轮新加，`src/styles.css:6111-6119`）。
    //      设置那侧本来就是这么做的（`.accounts-row-badge.warn`）⇒ 两处同职、同一套约定。
    //      而 api-key 那格本来就该是 warn 色（选得中却连不上，那是警示态不是正常态），
    //      它的文本仍逐字是「api-key（未配置端点）」—— 不拼任何字形。
    //   ② in-place：title「in-place 模式：不支持按会话切号」→
    //      「in-place 模式：cc-monitor 不支持对它按会话切号」（同义、更明确，但不逐字相同）。
    // `K-H2b` `KH2B7`〔08-28 `D1 阻-4` 订正了这一段：先前它写着「远端全关掉时这里渲染的
    // 就是本机账号」，而那是假的 —— `fetchAccounts` 只问 `list_remote_accounts`，
    // `origin` 为 `null` 时 `refresh` 整个隐藏并 `return`，一行都渲染不到。〕
    //
    // ★ `D1 阻-5`：本机那几行带上「走不走中转」的三态；远端那几行明说是远端那一半。
    //   问不到 routing（`null`）时**不表态** —— 回落到缺席那一档（只说条件、不下判断）。
    const s = accountStatusBadge(
      a,
      this.local
        ? this.relayRouting
          ? localRelayStateFor(a, this.relayRouting)
          : undefined
        : { scope: "remote" },
    );
    status.textContent = s.text;
    if (s.warn) status.classList.add("warn");
    row.title = s.title;
    row.appendChild(status);

    if (selectable && !isCurrent) {
      row.addEventListener("click", () => void this.selectDefault(a));
    } else if (!selectable) {
      row.addEventListener("click", (e) => e.preventDefault());
    }
    return row;
  }

  private menuAction(label: string, onClick: () => void): HTMLElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "account-picker-action";
    b.textContent = label;
    b.addEventListener("click", () => {
      this.closeMenu();
      onClick();
    });
    return b;
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
    if (!this.origin && !this.local) return null;
    return this.state;
  }

  /**
   * 同步快照当前 ready 账号（供 Ctrl+K buildCommands 同步读缓存）。非 ready → null。
   *
   * 🔴 `D4 阻-4`：这道门**已与 [`toggleMenu`] 同源**（[`accountPickerState`]）——
   * 本机那一档从此也回得出一份，命令面板里列得出 chip 菜单里列得出的那几个号。
   *
   * ⚠ `origin` 因此**可空**：`null` = 这份快照来自**本机**那一半。
   * 今天唯一的消费方 `buildAccountCommands` 只读 `accounts` / `defaultName`
   * （`account-commands.ts::AccountCommandsInput` 逐字，它连 `origin` 这个键都没有）
   * ⇒ 放空不改任何行为；留着这个字段是为了让读的人看得出这份快照是哪一半的。
   */
  snapshotReady(): { origin: string | null; accounts: Account[]; defaultName: string | null } | null {
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

  private async selectDefault(a: Account): Promise<void> {
    this.closeMenu();
    try {
      await setDefaultName(a.name);
      // audit-fixes I5：`defaultName` 是**全局单值**（config.json accounts.defaultName），切它要清
      // **所有 origin** 的缓存——否则多远端下非当前 origin 最长 30s(ACCOUNTS_TTL) 仍用旧默认账号判
      // "不一致"/对齐目标错，且 follow 会按旧账号持久化 pin。
      invalidateAccountsCache();
      await this.refresh(true);
      // 立刻重算会话账号/⚠k（否则 currentByOrigin 要等下一拍 10s 轮询，期间"对齐"会把会话
      // 打回刚被切走的旧账号——与用户意图正好相反）。
      this.deps.onDefaultChanged?.();
      showActionFailureToast(
        "已切当前账号",
        `以后新会话、以及没指定过账号的 resume 都会用 ${a.name}${a.email ? `（${a.email}）` : ""}；正在跑的会话不受影响（切号不重启任何东西）；已归属别的号的会话保持原号，要换到 ${a.name} 就在那个 tab 上右键选「把此会话切到账号 ${a.name}」。`,
        { level: "info", durationMs: 8000 },
      );
    } catch (e) {
      showActionFailureToast("切换当前账号失败", String(e), { level: "error" });
    }
  }

  private closeMenu(): void {
    if (this.menuClose) {
      document.removeEventListener("pointerdown", this.menuClose);
      document.removeEventListener("keydown", this.menuClose);
      this.menuClose = null;
    }
    this.menu?.remove();
    this.menu = null;
  }
}
