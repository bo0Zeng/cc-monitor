// A3：状态栏「当前账号」chip —— 全局默认账号的常驻显示 + 一键切换入口。
//
// 账号是 per-origin（每台远端一个 manifest）。cc-monitor 通常只连一台常用远端，
// 故 chip 绑「第一台可用远端」(pickPrimaryOrigin)，选单里若有多台再让用户切台。
// 点选账号 = **只改本机默认账号**（非破坏，DESIGN §2 的①），toast 明说已有会话不受影响。
import {
  fetchAccounts,
  deriveUi,
  currentWorkingAccount,
  isSelectable,
  setDefaultName,
  invalidateAccountsCache,
  type AccountsState,
  type Account,
} from "./accounts";
import { accountAvatarEl } from "./account-color";
import { readRemoteConfig, type RemoteHostConfig } from "./settings/remote-section";
import { showActionFailureToast } from "./error-toast";

// ------------------------------------------------------------ 纯函数（可测）

/** 选 chip 绑定的"主远端"：第一台非 daemonless 的已配置远端。无 → null（chip 隐藏）。 */
export function pickPrimaryOrigin(hosts: RemoteHostConfig[]): string | null {
  const h = hosts.find((x) => !x.daemonless && (x.label || x.host));
  return h ? h.label || h.host : null;
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
  /** account-ux U6：批量把不一致活会话按当前工作账号重启对齐（TabManager 提供，含两步确认）。 */
  alignAll?: () => void | Promise<void>;
  /** account-ux U6：切完当前工作账号后回调——让 main.ts 立刻重算会话账号/⚠k，
   *  否则会有最长 10s 的反向窗口（chip 已显新账号，对齐却把会话打回旧账号）。 */
  onDefaultChanged?: () => void;
}

export class AccountChip {
  readonly element: HTMLButtonElement;
  private labelSpan: HTMLElement;
  private iconEl: HTMLElement;
  private mismatchSpan: HTMLElement;
  /** account-ux U6：最近一次推进来的不一致数（菜单入口据它显隐）。 */
  private mismatchCount = 0;
  private origin: string | null = null;
  private state: AccountsState | null = null;
  private menu: HTMLElement | null = null;
  private menuClose: ((e: Event) => void) | null = null;

  constructor(private deps: AccountChipDeps) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "status-account";
    btn.title = "当前工作账号（点击切换 / 管理）";
    const icon = document.createElement("span");
    icon.className = "status-account-icon";
    icon.textContent = "👤";
    icon.setAttribute("aria-hidden", "true");
    btn.appendChild(icon);
    this.iconEl = icon;
    this.labelSpan = document.createElement("span");
    this.labelSpan.className = "status-account-label";
    btn.appendChild(this.labelSpan);
    // account-ux U6：⚠k 不一致计数（有活会话不在当前工作账号时显）。
    this.mismatchSpan = document.createElement("span");
    this.mismatchSpan.className = "status-account-mismatch";
    this.mismatchSpan.style.display = "none";
    btn.appendChild(this.mismatchSpan);
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
    // account-ux U4：ready 时把 👤 换成当前工作账号的彩色头像（与 tab 徽章同色系 → 肉眼可对应）。
    const cur = currentWorkingAccount(this.state);
    this.iconEl.textContent = "";
    if (cur) {
      this.iconEl.appendChild(accountAvatarEl(cur.name));
    } else {
      this.iconEl.textContent = "👤";
    }
    this.element.style.display = "";
    this.updateMismatchBadge(this.mismatchCount); // 用最近一次推进来的值重判（ready/可见性可能变了）
  }

  /** account-ux U6：把 ⚠k 不一致计数**推**进 chip（main.ts 在 setSessionAccounts 后同拍调，
   *  与 tab 徽章同源）。缓存下来给 toggleMenu 用，避免菜单再去反拉 TabManager。
   *  只在 chip 可见**且** ui 为 ready 时显：非 ready（未启用/需更新/daemonless）时菜单里根本没有
   *  对齐入口，显个"点开菜单可一键对齐"的计数就是死胡同（D 审计）。 */
  updateMismatchBadge(count: number): void {
    this.mismatchCount = count;
    const ready = this.state ? deriveUi(this.state).kind === "ready" : false;
    if (count > 0 && ready && this.element.style.display !== "none") {
      this.mismatchSpan.textContent = `⚠${count}`;
      const msg = `${count} 个正在跑的会话不在当前工作账号——点开菜单可一键对齐`;
      this.mismatchSpan.title = msg;
      this.mismatchSpan.setAttribute("aria-label", msg);
      this.mismatchSpan.style.display = "";
    } else {
      this.mismatchSpan.style.display = "none";
    }
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
      // account-ux U6：有不一致的活会话时，菜单顶部给一条批量对齐入口（破坏性 → 走 TabManager 的两步确认）。
      // 文案不说"当前账号"（单数）：多远端时计数是跨 origin 的、各按各自远端的当前账号对齐。
      const mismatch = this.mismatchCount;
      if (mismatch > 0 && this.deps.alignAll) {
        const align = this.menuAction(`⚠ 对齐 ${mismatch} 个账号不一致的会话…`, () => {
          void this.deps.alignAll?.();
        });
        align.classList.add("danger");
        menu.appendChild(align);
        const sepTop = document.createElement("div");
        sepTop.className = "account-picker-sep";
        menu.appendChild(sepTop);
      }
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
      invalidateAccountsCache(this.origin ?? undefined);
      await this.refresh(true);
      // 立刻重算会话账号/⚠k（否则 currentByOrigin 要等下一拍 10s 轮询，期间"对齐"会把会话
      // 打回刚被切走的旧账号——与用户意图正好相反）。
      this.deps.onDefaultChanged?.();
      showActionFailureToast(
        "已切当前工作账号",
        `以后新会话、以及没指定过账号的 resume 都会用 ${a.name}${a.email ? `（${a.email}）` : ""}；正在跑的会话不受影响（切号不重启任何东西）；已归属别的号的会话保持原号，要换到 ${a.name} 就在它的账号徽章/右键选「用 ${a.name} 重启」。`,
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
