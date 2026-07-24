// A3：状态栏「当前账号」chip —— 全局默认账号的常驻显示 + 一键切换入口。
//
// 账号是 per-origin（每台远端一个 manifest）。cc-monitor 通常只连一台常用远端，
// 故 chip 绑「第一台可用远端」(pickPrimaryOrigin)，选单里若有多台再让用户切台。
// 点选账号 = **只改本机默认账号**（非破坏，DESIGN §2 的①），toast 明说已有会话不受影响。
import {
  fetchAccounts,
  deriveUi,
  effectiveDefault,
  isSelectable,
  setDefaultName,
  invalidateAccountsCache,
  type AccountsState,
  type Account,
} from "./accounts";
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
      const def = effectiveDefault(state);
      return def ? def.name : "未启用";
    }
  }
}

// ------------------------------------------------------------ chip 组件

export interface AccountChipDeps {
  /** 打开设置窗口的账号组（A3 设置组落地后接线；先给个跳设置的回调）。 */
  openSettings: () => void;
}

export class AccountChip {
  readonly element: HTMLButtonElement;
  private labelSpan: HTMLElement;
  private origin: string | null = null;
  private state: AccountsState | null = null;
  private menu: HTMLElement | null = null;
  private menuClose: ((e: Event) => void) | null = null;

  constructor(private deps: AccountChipDeps) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "status-account";
    btn.title = "当前账号（点击切换默认账号 / 管理）";
    const icon = document.createElement("span");
    icon.className = "status-account-icon";
    icon.textContent = "👤";
    icon.setAttribute("aria-hidden", "true");
    btn.appendChild(icon);
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
    this.element.style.display = "";
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
      const def = effectiveDefault(this.state);
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
    return { origin: this.origin, accounts: ui.accounts, defaultName: effectiveDefault(this.state)?.name ?? null };
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
      showActionFailureToast(
        "已切默认账号",
        `新会话将使用 ${a.name}${a.email ? `（${a.email}）` : ""}。已有会话不受影响——需要换号请在会话上选「用 ${a.name} 重启」。`,
        { level: "info", durationMs: 6000 },
      );
    } catch (e) {
      showActionFailureToast("切换默认账号失败", String(e), { level: "error" });
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
