// A3：状态栏「当前账号」chip —— 全局默认账号的常驻显示 + 一键切换入口。
//
// 账号是 per-origin（每台远端一个 manifest）。cc-monitor 通常只连一台常用远端，
// 故 chip 绑「第一台可用远端」(pickPrimaryOrigin)，选单里若有多台再让用户切台。
// 点选账号 = **只改本机默认账号**（非破坏，DESIGN §2 的①），toast 明说已有会话不受影响。
import {
  fetchAccounts,
  deriveUi,
  currentWorkingAccount,
  accountColorsActive,
  isSelectable,
  setDefaultName,
  invalidateAccountsCache,
  type AccountsState,
  type Account,
} from "./accounts";
import { fetchAccountUsage, OK_USAGE_UNVERIFIED_CAVEAT, type AccountUsageOutcome } from "./account-usage";
import { accountAvatarEl } from "./account-color";
import { readRemoteConfig, type RemoteHostConfig } from "./remote-config";
import { showActionFailureToast } from "./error-toast";

// ------------------------------------------------------------ 纯函数（可测）

/** 选 chip 绑定的"主远端"：第一台非 daemonless 的已配置远端。无 → null（chip 隐藏）。 */
export function pickPrimaryOrigin(hosts: RemoteHostConfig[]): string | null {
  const h = hosts.find((x) => !x.daemonless && (x.label || x.host));
  return h ? h.label || h.host : null;
}

/** F10：把 `AccountUsageOutcome` 压成**折叠态 chip**（`status-account-usage`，`10ch` 宽的
 *  label 旁边）能放下的极短摘要——"38%"（单窗口）、"38/71/12%"（多窗口,省重复的 % 符号）；
 *  失败态一律空串（不占地方——折叠态空间真的挤不下任何失败短句，"没查过"和"查了但失败"在
 *  这里视觉相同，是空间约束下的取舍，不是遗漏；想看失败原因走 `formatUsageSummaryForMenu`
 *  的菜单行，那里有富余空间）。 */
export function formatUsageSummaryCompact(outcome: AccountUsageOutcome): string {
  if (outcome.status !== "ok" || outcome.buckets.length === 0) return "";
  if (outcome.buckets.length === 1) return `${outcome.buckets[0].usedPercent}%`;
  return `${outcome.buckets.map((b) => b.usedPercent).join("/")}%`;
}

/** F10 Phase D 审计（UX，重要）：**菜单里当前账号那一行**的用量摘要——跟折叠态 chip 不一样,
 *  这一行本来就已经在展示名字/邮箱/登录态,富余空间放得下几个字的失败短句,不该跟"没查过"
 *  一样空白。 */
export function formatUsageSummaryForMenu(outcome: AccountUsageOutcome): string {
  if (outcome.status === "ok") return formatUsageSummaryCompact(outcome);
  const short: Record<Exclude<AccountUsageOutcome["status"], "ok">, string> = {
    "not-logged-in": "未登录",
    "cli-missing": "无 claude",
    unrecognized: "读不到",
    "probe-failed": "探测失败",
  };
  return short[outcome.status];
}

/** chip 文本（不含图标）。纯函数，据 UI 状态 + 当前默认账号算。 */
export function chipLabel(state: AccountsState | null): string {
  if (!state) return "未连远端";
  const ui = deriveUi(state);
  switch (ui.kind) {
    case "hidden":
      return ""; // 调用方据空串隐藏
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
  /** F10：折叠态紧邻 label 的用量摘要（如 "62%"）——只在菜单展开、懒加载完当前账号用量后才
   *  填，默认空（不自动探测,较重操作）。 */
  private usageSpan: HTMLElement;
  private iconEl: HTMLElement;
  private origin: string | null = null;
  private state: AccountsState | null = null;
  private menu: HTMLElement | null = null;
  private menuClose: ((e: Event) => void) | null = null;
  /** 菜单里"当前账号那一行"的用量展示节点——`loadCurrentAccountUsage` 懒加载完成后回填。
   *  菜单每次开合都是全新 DOM,这个引用只在菜单开着的这段时间有效。 */
  private menuCurrentUsageEl: HTMLElement | null = null;

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
    this.usageSpan = document.createElement("span");
    this.usageSpan.className = "status-account-usage";
    btn.appendChild(this.usageSpan);
    btn.addEventListener("click", () => void this.toggleMenu());
    this.element = btn;
    this.element.style.display = "none"; // 拿到数据前先藏
  }

  /** 拉数据刷新 chip（初始 / 设置变更 / 手动）。force 透传给缓存。
   *  F10：每次刷新都清空折叠态用量摘要——账号可能已经变了（切号/换远端），旧用量数字对新
   *  账号是错的；用量本身**不**在这里重新懒加载（较重操作，见类头注），等用户下次展开菜单。 */
  async refresh(force = false): Promise<void> {
    this.usageSpan.textContent = "";
    this.usageSpan.title = "";
    try {
      const cfg = await readRemoteConfig();
      this.origin = cfg.enabled ? pickPrimaryOrigin(cfg.hosts) : null;
    } catch {
      this.origin = null;
    }
    if (!this.origin) {
      this.state = null;
      this.element.style.display = "none";
      return;
    }
    this.state = await fetchAccounts(this.origin, force);
    const text = chipLabel(this.state);
    if (!text) {
      this.element.style.display = "none"; // daemonless 等 → 完全不显示
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
   *  今天靠 pickPrimaryOrigin 过滤 daemonless 才碰不到，那是巧合不是设计。这里显式挡住。 */
  async openMenu(): Promise<void> {
    if (this.element.style.display === "none") return;
    await this.toggleMenu();
  }

  private async toggleMenu(): Promise<void> {
    if (this.menu) {
      this.closeMenu();
      return;
    }
    if (!this.origin || !this.state) return;
    const ui = deriveUi(this.state);
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
      const def = currentWorkingAccount(this.state);
      this.menuCurrentUsageEl = null;
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
      // F10：与"刷新"（账号列表本身）语义分开——只重查当前账号的用量,不连带重拉整份账号
      // 列表(那是"刷新"的事,两者混在一起会让用户以为点了刷新账号列表也会顺带重新探测用量)。
      // F10 Phase D 审计（UX，重要）：menuAction 点击会立刻关闭菜单,用户看不到刷新过程——
      // 补一条完成 toast（同 selectDefault 既有惯例），不然用户只能凭空猜"刚才点了有没有生效"。
      menu.appendChild(this.menuAction("刷新用量", () => this.loadCurrentAccountUsage(def, true, true)));
      // 菜单展开时才懒加载当前账号用量(不是 app 启动/`refresh()` 时——那是轻量调用,不该
      // 背上几秒的探针成本)。
      if (def) this.loadCurrentAccountUsage(def, false);
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
    if (a.mode === "in-place") {
      status.textContent = "逃生口";
      row.title = "in-place 模式：不支持按会话切号";
    } else if (!a.loggedIn) {
      status.textContent = "未登录 ⚠";
      row.title = "该账号尚未登录——请在终端里用它 /login";
    } else {
      status.textContent = "已登录";
    }
    row.appendChild(status);

    // F10：只有当前账号那一行才懒加载用量摘要（其余账号不主动拉，除非用户切过去变成当前）。
    if (isCurrent) {
      const usage = document.createElement("span");
      usage.className = "account-picker-usage";
      row.appendChild(usage);
      this.menuCurrentUsageEl = usage;
    }

    if (selectable && !isCurrent) {
      row.addEventListener("click", () => void this.selectDefault(a));
    } else if (!selectable) {
      row.addEventListener("click", (e) => e.preventDefault());
    }
    return row;
  }

  /** F10：懒加载当前账号的 plan 用量窗口%，同时回填折叠态 chip（`usageSpan`，跨菜单开合存活）
   *  与菜单里当前账号行（`menuCurrentUsageEl`，菜单一关就失效——"刷新用量"点击时菜单已经
   *  被 `menuAction` 关掉了，这条更新是无害的 no-op，折叠态仍会正确更新）。`force=false`
   *  时走 `fetchAccountUsage` 的去抖缓存——菜单短时间内反复展开不会重复戳网络。
   *  `notify=true`（"刷新用量"按钮用）：`menuAction` 点击会立刻关闭菜单，用户看不到刷新
   *  过程——补一条完成 toast（同 `selectDefault` 既有惯例），不然只能凭空猜"刚才点了有没有
   *  生效"。 */
  private loadCurrentAccountUsage(def: Account | null | undefined, force: boolean, notify = false): void {
    if (!def || !this.origin) return;
    const origin = this.origin;
    const accountName = def.name;
    // F10 Phase D 审计（UX，重要）：菜单展开期间当前账号那一行此前完全空白,跟"探测失败"/
    // "没查过"视觉上无法区分——探测开始就先给一个占位,resolve 后再换成真实结果/失败短句。
    if (this.menuCurrentUsageEl) this.menuCurrentUsageEl.textContent = "…";
    // Z01：账号 0 没有 configDir，用量探测要起一次带 CLAUDE_CONFIG_DIR 的隐藏会话
    // ⇒ 今天起不了。**明说**而不是空白（同本文件其余状态一律给短句的口径）。
    if (def.configDir === null) {
      if (this.menuCurrentUsageEl) this.menuCurrentUsageEl.textContent = "账号 0 暂不支持用量查询";
      return;
    }
    const defConfigDir = def.configDir;
    void fetchAccountUsage(origin, accountName, defConfigDir, { force }).then((outcome) => {
      // F10 Phase D 审计（UX，阻塞）：探测耗时可达数秒到 25s,期间用户可能已经切到另一个账号
      // （selectDefault → refresh 会同步清空/重填 usageSpan）——不加这道身份校验,姗姗来迟的
      // 结果会把"账号名已经是新账号,百分比却是旧账号的"这种静默误标写进折叠态 chip,用户毫无
      // 办法察觉。`menuCurrentUsageEl` 分支目前"意外地"安全（菜单关闭后节点已从 DOM 摘除,
      // 写入是无效操作）,但这是巧合不是设计,同样加上校验防重构后复发。
      if ((this.state && currentWorkingAccount(this.state)?.name) !== accountName) {
        if (notify) {
          showActionFailureToast("用量刷新已过期", "刷新期间当前账号已切换，结果不再适用，已丢弃。", {
            level: "info",
            durationMs: 4000,
          });
        }
        return;
      }
      // F10 Phase D 审计（后端架构+UX 均指出，重要）：与 accounts-section.ts 共享同一句
      // "格式未经真机验证"提示（`OK_USAGE_UNVERIFIED_CAVEAT`）——ok 状态看起来是确定的数字，
      // 但解析成功不代表百分比方向/数值本身已验证过。
      const okTitle = outcome.status === "ok" ? OK_USAGE_UNVERIFIED_CAVEAT : "";
      this.usageSpan.textContent = formatUsageSummaryCompact(outcome);
      this.usageSpan.title = okTitle;
      if (this.menuCurrentUsageEl) {
        this.menuCurrentUsageEl.textContent = formatUsageSummaryForMenu(outcome);
        this.menuCurrentUsageEl.title = okTitle;
      }
      if (notify) {
        const ok = outcome.status === "ok";
        showActionFailureToast(
          ok ? "用量已刷新" : "用量刷新：读不到",
          ok
            ? outcome.buckets
                .map((b) => `${b.label} ${b.usedPercent}%${b.resetIn ? ` · 重置${b.resetIn}` : ""}`)
                .join("；")
            : formatUsageSummaryForMenu(outcome),
          { level: ok ? "info" : "error", durationMs: 5000 },
        );
      }
    });
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

  /** 同步快照当前 ready 账号（供 Ctrl+K buildCommands 同步读缓存）。非 ready → null。 */
  snapshotReady(): { origin: string; accounts: Account[]; defaultName: string | null } | null {
    if (!this.origin || !this.state) return null;
    const ui = deriveUi(this.state);
    if (ui.kind !== "ready") return null;
    return { origin: this.origin, accounts: ui.accounts, defaultName: currentWorkingAccount(this.state)?.name ?? null };
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
