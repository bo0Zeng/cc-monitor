/**
 * 机器页「账号」栏：`{machine} 上的账号 · 新会话默认 X` ＋ 一张表（一号一行，44 高：头像 · 名 ＋ 默认 · 种类 · `5h` · `7d` · 动作 · ⋯），
 * 点行展开详情（命令 · API key · 默认模型 · 账号目录 · 删除）。本机远端同一张表，差别只在「打开账号目录」是文件夹还是文件窗口。
 *
 * 判定全在那台后端：清单与登录态（`accounts-list`）· 删了默认号之后谁接（`meta.nextDefault`）· 用量（`quota-read`）·
 * 对不上的几处（`accounts-verify`，打开时顺手核一次）· 命令名（配置文件里认作这个号的那几段，`profiles-read` 的 `accountShape`）·
 * 能不能开终端窗口（壳那一处）。这里只排版、只认最后一趟回答（切机器快过读时，晚到的整份作废）。
 * 那台账号清单 / 凭据 / 用量一变，后端推一帧（`accounts-changed` · `quota-changed`），这一页自己重读。
 */
import { agoText } from "./ago";
import { emit } from "@tauri-apps/api/event";
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { SETTINGS_APPLIED_EVENT, SETTINGS_GO_EVENT } from "./events";
import { renderNewAccountForm, checkBaseUrl, type NewAccountForm, type NewAccountRequest } from "./account-new-form";
import { openLoginWindow, loginInTmux } from "./account-login";
import { machineHasTmux } from "../resume-defaults";
import { accountRowKind, deriveUi, defaultAccount, type Account, type AccountsState } from "../accounts";
import { accountsAgentProfile, fetchAccounts, invalidateAccountsCache, launchAgentId } from "../account-reads";
import { setModelForAccount, getModelForAccount } from "../account-prefs";
import { accountAvatarEl } from "../account-color";
import { readQuota } from "../quota-reads";
import { slotLabel, slotText, type QuotaRead } from "../acct-words";
import type { QuotaShow } from "../generated/QuotaShow";
import { readProfiles } from "../profiles-reads";
import { writeApikeyKey } from "../apikey-reads";
import { revealInFolder } from "../reveal-in-folder";
import { openFileWindow } from "../file-window";
import { findHostByOrigin, readRemoteConfig, type RemoteHostConfig } from "../remote-config";
import { bindEvents } from "../events";
import { accountsAdd, accountsInit, accountsRemove, accountsRepair, accountsRollback, accountsSetDefault, accountsVerify, validateAcctName, type VerifyReport } from "../account-ops";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import { button, buttonRow, setBusy, setDisabled } from "../kit/button";
import { banner } from "../kit/banner";
import { emptyState } from "../kit/empty";
import { skeletonRows } from "../kit/skeleton";
import { field } from "../kit/field";
import { openMenu, type MenuItem } from "../kit/menu";
import { spinner } from "../kit/progress";
import { tag } from "../kit/badge";
import { toast, failToast } from "../kit/toast";
import { homeShort } from "../kit/path";
import { copyText } from "../copy-table";
import { writeClipboard } from "../clipboard";
import { machineName, saidOfControl } from "../control-said";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";

/** 一趟读回来的那一台（都是那台后端的成品；读不到的那一格是 `null`）。 */
interface Facts {
  origin: Origin;
  state: AccountsState;
  quota: QuotaRead | null;
  verify: VerifyReport | null;
  /** 号名 ⇒ 别名清单里认作它的那几条命令名。 */
  commands: Map<string, string[]>;
  /** 机器表里没有这台 ⇒ 没连上过（不去问）。 */
  unknown: boolean;
}

/** 注入缝（判据换掉确认框）。 */
export interface AccountsSectionOptions {
  confirm?: ConfirmFn;
}

/** 推来的帧合并成一次重读的间隔。 */
const PUSH_COALESCE_MS = 300;

export class AccountsSection {
  readonly element: HTMLElement;
  private readonly titleEl: HTMLElement;
  private readonly subEl: HTMLElement;
  private readonly newBtn: HTMLButtonElement;
  private readonly formSlot: HTMLElement;
  private readonly body: HTMLElement;
  private readonly confirm: ConfirmFn;
  private origin: Origin = LOCAL_ORIGIN;
  /** 每趟重读一个号；晚到的那一趟作废。 */
  private seq = 0;
  /** 展开着的行（按「机器 ＋ 号」记，重读后不收起）。 */
  private readonly open = new Set<string>();
  /** 这次开着窗口时为它开过登录窗口的号（那一行「等待终端登录…」＋［重新打开登录窗口］）。 */
  private readonly waiting = new Set<string>();
  /** 开不了终端窗口的号（那一行改说「本机无法开终端窗口」＋［在 tmux 里登录］）。 */
  private readonly noWindow = new Set<string>();
  /** 这次窗口里修复过的机器（警告条 ⋯ 里才有「恢复到修复之前」）。 */
  private readonly repaired = new Set<Origin>();
  private form: NewAccountForm | null = null;
  private facts: Facts | null = null;
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private subscribed = false;
  /** 宿主调过 `loadNow` 没有（之前切机器只记下，不发 I/O）。 */
  private loaded = false;
  /** 机器表（这一次打开里读一次；问到表里没有的那台再重读一次）。 */
  private hosts: Promise<RemoteHostConfig[] | null> | null = null;

  constructor(opts: AccountsSectionOptions = {}) {
    this.confirm = opts.confirm ?? confirmDialog;
    const root = document.createElement("div");
    root.className = "acct-page";
    const head = document.createElement("div");
    head.className = "acct-head";
    this.titleEl = document.createElement("h3");
    this.titleEl.className = "acct-head-title";
    this.subEl = document.createElement("span");
    this.subEl.className = "acct-head-sub";
    const right = document.createElement("div");
    right.className = "acct-head-actions";
    this.newBtn = button({ label: copyText("acctPage.head.new"), icon: "plus", size: "compact", onClick: () => this.toggleForm() });
    const refresh = button({
      label: copyText("acctPage.head.refresh"),
      kind: "icon",
      icon: "refresh",
      size: "compact",
      hint: copyText("acctPage.head.refresh"),
      onClick: () => void this.reload(true),
    });
    right.append(this.newBtn, refresh);
    head.append(this.titleEl, this.subEl, right);
    this.formSlot = document.createElement("div");
    this.formSlot.className = "acct-form-slot";
    this.body = document.createElement("div");
    this.body.className = "acct-body";
    root.append(head, this.formSlot, this.body);
    root.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape" && this.form && this.formSlot.contains(ev.target as Node)) {
        ev.stopPropagation();
        void this.form.dismiss();
      }
    });
    this.element = root;
    subscribeMachine((origin) => {
      if (!this.loaded || origin === this.origin) return;
      this.origin = origin;
      this.closeForm();
      void this.reload(true);
    });
  }

  /** 宿主在这一栏第一次可见时调（重开设置再调一次）。 */
  loadNow(): void {
    this.loaded = true;
    this.hosts = null;
    this.origin = getCurrentMachine();
    // 每次（重）开都看新读数：不吃账号缓存。
    void this.reload(true);
    void this.subscribe();
  }

  private readHosts(fresh = false): Promise<RemoteHostConfig[] | null> {
    if (fresh || this.hosts === null) {
      this.hosts = readRemoteConfig().then(
        (c) => c.hosts,
        () => null,
      );
    }
    return this.hosts;
  }

  /** 每台订 `accounts-changed` ＋ `quota-changed`：一来就合并成一次重读（只重读此刻在看的那台）。 */
  private async subscribe(): Promise<void> {
    if (this.subscribed) return;
    this.subscribed = true;
    // 机器表读不到：只订本机。
    const origins: Origin[] = [LOCAL_ORIGIN, ...((await this.readHosts()) ?? []).map((h) => h.label)];
    const pushed = (): void => {
      if (this.pushTimer !== null) clearTimeout(this.pushTimer);
      // 调度：合批 —— 后端推来的账号 / 额度变更：300ms 内几帧合成一次重读
      this.pushTimer = setTimeout(() => {
        this.pushTimer = null;
        void this.reload(true, true);
      }, PUSH_COALESCE_MS);
    };
    try {
      await bindEvents({ onLine: () => {}, onSessionEnded: () => {}, onAccountsChanged: pushed, onQuotaChanged: pushed }, { accounts: origins, quota: origins });
    } catch (e) {
      console.warn("[accounts] 订不上账号推送：", e);
    }
  }

  private key(name: string, origin: Origin = this.origin): string {
    return `${origin}\u0000${name}`;
  }

  // ───────────────────────────── 读 ─────────────────────────────

  /** `quiet`：推来的 / 动作之后的那一趟不画骨架（原位换）。 */
  private async reload(force: boolean, quiet = false): Promise<void> {
    const my = ++this.seq;
    const origin = this.origin;
    this.titleEl.textContent = copyText("acctPage.head.title", { machine: machineName(origin) });
    if (!quiet || this.facts?.origin !== origin) {
      this.facts = null;
      this.subEl.textContent = "";
      this.body.replaceChildren(skeletonRows(3));
      this.body.setAttribute("aria-busy", "true");
    }
    if (force) invalidateAccountsCache(origin);
    const facts = await this.read(origin, force);
    if (my !== this.seq) return;
    this.body.removeAttribute("aria-busy");
    this.facts = facts;
    this.paint(facts);
  }

  private async read(origin: Origin, force: boolean): Promise<Facts> {
    const empty = new Map<string, string[]>();
    if (!isLocalOrigin(origin)) {
      let hosts = await this.readHosts();
      // 表是空的（没配远端，或读不到）就不核：读不到不等于这台不在。同一次打开里刚加进机器表的那台：再读一次。
      if (hosts !== null && hosts.length > 0 && findHostByOrigin(hosts, origin) === null) hosts = await this.readHosts(true);
      if (hosts !== null && hosts.length > 0 && findHostByOrigin(hosts, origin) === null) {
        const state: AccountsState = { origin, available: false, error: null, oldBackend: false, meta: null, accounts: [], notice: null };
        return { origin, state, quota: null, verify: null, commands: empty, unknown: true };
      }
    }
    const state = await fetchAccounts(origin, force);
    if (!state.available || deriveUi(state).kind !== "ready") {
      return { origin, state, quota: null, verify: null, commands: empty, unknown: false };
    }
    const [quota, verify, book] = await Promise.all([
      readQuota(origin).catch(() => null),
      accountsVerify(origin).catch(() => null),
      readProfiles(origin).catch(() => null),
    ]);
    const commands = new Map<string, string[]>();
    for (const p of book?.profiles ?? []) {
      const g = p.accountShape;
      if (g) commands.set(g.account, [...(commands.get(g.account) ?? []), p.name]);
    }
    return { origin, state, quota, verify, commands, unknown: false };
  }

  // ───────────────────────────── 画 ─────────────────────────────

  private paint(f: Facts): void {
    const machine = machineName(f.origin);
    const out: HTMLElement[] = [];
    setDisabled(this.newBtn, null);
    if (f.unknown) {
      setDisabled(this.newBtn, copyText("acctPage.never.newHint"));
      out.push(this.emptyCard(copyText("acctPage.never.title", { machine }), copyText("acctPage.never.hint")));
      this.body.replaceChildren(...out);
      return;
    }
    const s = f.state;
    // 这一次没问到：有上次的 ⇒ 画上次的（只读）＋ 警告条；没有 ⇒ 照实说。
    if (!s.available) {
      setDisabled(this.newBtn, copyText("acctPage.offline.hover"));
      const last = s.last ?? null;
      const retry = button({ label: copyText("acctPage.offline.retry"), size: "compact", onClick: () => void this.reload(true) });
      const text = last
        ? copyText("acctPage.offline.bar", { machine, ago: agoText(Date.now() - last.atMs) })
        : copyText("acctPage.offline.noLast", { machine, why: s.error ?? "" });
      out.push(banner("warn", text, [retry]));
      if (last && last.accounts.length > 0) {
        const def = last.meta.effectiveDefault;
        this.subEl.textContent = def === null ? "" : copyText("acctPage.head.default", { name: def });
        out.push(this.table({ ...f, state: { ...s, meta: last.meta, accounts: last.accounts } }, true));
      }
      this.body.replaceChildren(...out);
      return;
    }
    // 这台做不了多账号（后端答的）⇒ 一张卡，不摆启用表单。
    if (s.meta?.unsupported) {
      setDisabled(this.newBtn, s.meta.unsupported);
      out.push(this.emptyCard(copyText("acctPage.unsupported.title"), copyText("acctPage.unsupported.hint")));
      this.body.replaceChildren(...out);
      return;
    }
    const ui = deriveUi(s);
    if (ui.kind !== "ready") {
      setDisabled(this.newBtn, copyText("acctPage.notEnabled.newHint"));
      out.push(this.enableCard(f.origin));
      this.body.replaceChildren(...out);
      return;
    }
    const def = defaultAccount(s);
    this.subEl.textContent = def ? copyText("acctPage.head.default", { name: def.name }) : "";
    const v = this.verifyBar(f);
    if (v) out.push(v);
    if (ui.notice) out.push(banner("warn", ui.notice));
    out.push(this.table(f, false), this.mcpLine(f.origin));
    this.body.replaceChildren(...out);
  }

  private emptyCard(text: string, hint: string): HTMLElement {
    const box = document.createElement("div");
    box.className = "acct-box";
    box.appendChild(emptyState({ text, hint, icon: "empty" }));
    return box;
  }

  /** 没启用多账号：名字框 ＋［启用］；点了先列要做的几步（那台预演）再确认。 */
  private enableCard(origin: Origin): HTMLElement {
    const box = document.createElement("div");
    box.className = "acct-box acct-enable";
    const t = document.createElement("div");
    t.className = "acct-enable-title";
    t.textContent = copyText("acctPage.notEnabled.title");
    const hint = document.createElement("div");
    hint.className = "acct-enable-hint";
    hint.textContent = copyText("acctPage.notEnabled.hint");
    const name = field({ label: copyText("acctPage.notEnabled.name"), placeholder: copyText("acctNew.name.example"), noteOnDemand: true });
    const go = button({ label: copyText("acctPage.notEnabled.enable"), kind: "primary" });
    go.addEventListener("click", () => {
      const n = name.input.value.trim();
      const ok = validateAcctName(n);
      if (!ok.ok) {
        name.setError(ok.reason);
        return;
      }
      name.setError(null);
      void (async () => {
        setBusy(go, copyText("acctPage.notEnabled.enabling"));
        try {
          const plan = await accountsInit(origin, { name: n, dryRun: true });
          const yes = await this.confirm({
            title: copyText("acctPage.notEnabled.confirmTitle", { machine: machineName(origin) }),
            action: copyText("acctPage.notEnabled.enable"),
            list: plan.steps,
          });
          if (!yes) return;
          await accountsInit(origin, { name: n });
          await this.reload(true);
        } catch (e) {
          name.setError(saidOfControl(e));
        } finally {
          setBusy(go, null);
        }
      })();
    });
    const row = document.createElement("div");
    row.className = "acct-enable-row";
    row.append(name.root, go);
    box.append(t, hint, row);
    return box;
  }

  /** 打开时顺手核过一次：有对不上的才出一条警告条 ＋［修复…］；这次窗口里修复过 ⇒ ⋯ 里有「恢复到修复之前」。 */
  private verifyBar(f: Facts): HTMLElement | null {
    const machine = machineName(f.origin);
    const fails = f.verify?.checks.filter((c) => c.level === "fail") ?? [];
    if (fails.length === 0) return null;
    const first = fails[0];
    const text = first.account ? copyText("acctPage.verify.bar", { name: first.account, what: first.text }) : copyText("acctPage.verify.barGlobal", { what: first.text });
    const more = fails.length > 1 ? copyText("acctPage.verify.andMore", { n: fails.length - 1 }) : "";
    const actions: HTMLElement[] = [button({ label: copyText("acctPage.verify.fix"), size: "compact", onClick: () => void this.repair(f.origin, machine) })];
    if (this.repaired.has(f.origin)) {
      actions.push(
        button({
          label: copyText("acctPage.verify.more"),
          kind: "icon",
          icon: "more",
          size: "compact",
              onClick: (ev) => {
            openMenu({ el: ev.currentTarget as HTMLElement, align: "end" }, [{ label: copyText("acctPage.verify.rollback"), onClick: () => void this.rollback(f.origin) }]);
          },
        }),
      );
    }
    return banner("warn", more ? `${text}${copyText("kit.text.sep")}${more}` : text, actions);
  }

  private async repair(origin: Origin, machine: string): Promise<void> {
    try {
      const plan = await accountsRepair(origin, { dryRun: true });
      if (plan.steps.length === 0) {
        toast(copyText("acctPage.verify.nothing", { machine }), "");
        return;
      }
      const yes = await this.confirm({
        title: copyText("acctPage.verify.repairTitle", { machine }),
        action: copyText("acctPage.verify.repair"),
        body: copyText("acctPage.verify.repairBody"),
        list: plan.steps,
      });
      if (!yes) return;
      await accountsRepair(origin, {});
      this.repaired.add(origin);
      toast(copyText("acctPage.verify.repaired", { machine }), "");
    } catch (e) {
      failToast(copyText("acctPage.verify.failed", { machine }), e, { level: "error" });
    }
    await this.reload(true, true);
  }

  private async rollback(origin: Origin): Promise<void> {
    const machine = machineName(origin);
    try {
      const plan = await accountsRollback(origin, { dryRun: true });
      const yes = await this.confirm({ title: copyText("acctPage.verify.rollbackTitle", { machine }), action: copyText("acctPage.verify.rollback"), list: plan.steps });
      if (!yes) return;
      await accountsRollback(origin, plan.backup ? { backup: plan.backup } : {});
      this.repaired.delete(origin);
    } catch (e) {
      failToast(copyText("acctPage.verify.failed", { machine }), e, { level: "error" });
    }
    await this.reload(true, true);
  }

  /** 表下一行「共用 MCP：别名与配置文件」：点了切到同一台的「别名与配置文件」栏。 */
  private mcpLine(origin: Origin): HTMLElement {
    const line = document.createElement("div");
    line.className = "acct-mcp-line";
    const t = document.createElement("span");
    t.textContent = copyText("acctPage.mcp.where");
    const go = document.createElement("button");
    go.type = "button";
    go.className = "acct-pointer-link";
    go.dataset.go = "config";
    go.textContent = copyText("machinePage.tab.config");
    go.addEventListener("click", () => {
      this.element.dispatchEvent(new CustomEvent(SETTINGS_GO_EVENT, { bubbles: true, detail: { machine: origin, tab: "config", anchor: "shared-mcp" } }));
    });
    line.append(t, go);
    return line;
  }

  // ───────────────────────────── 表 ─────────────────────────────

  private table(f: Facts, readonly: boolean): HTMLElement {
    const list = document.createElement("div");
    list.className = "acct-table";
    list.setAttribute("role", "list");
    if (readonly) list.dataset.readonly = "true";
    const now = f.quota?.now ?? Math.floor(Date.now() / 1000);
    const agent = safeAgent();
    for (const a of f.state.accounts) list.appendChild(this.row(f, a, f.quota ? quotaOf(f.quota, agent, a.name) : null, readonly, now));
    return list;
  }

  private row(f: Facts, a: Account, q: QuotaShow | null, readonly: boolean, now: number): HTMLElement {
    const origin = f.origin;
    const key = this.key(a.name, origin);
    const wrap = document.createElement("div");
    wrap.className = "acct-row-wrap";
    wrap.setAttribute("role", "listitem");
    wrap.dataset.account = a.name;
    const row = document.createElement("div");
    row.className = "acct-row";
    row.tabIndex = 0;
    row.setAttribute("aria-expanded", String(this.open.has(key)));
    if (readonly) row.title = copyText("acctPage.offline.hover");

    const who = document.createElement("div");
    who.className = "acct-row-who";
    const line1 = document.createElement("div");
    line1.className = "acct-row-name";
    const nm = document.createElement("span");
    nm.textContent = a.name;
    line1.appendChild(nm);
    const isDef = a.name === f.state.meta?.effectiveDefault;
    if (isDef) line1.appendChild(tag(copyText("acctPage.row.default")));
    const waiting = this.waiting.has(key) && accountRowKind(a) === "notLoggedIn";
    if (waiting) line1.appendChild(spinner());
    if (q?.login === "needsKey") {
      const t = tag(copyText("acctPage.row.keyNotSaved"));
      t.dataset.shade = "error";
      line1.appendChild(t);
    }
    const line2 = document.createElement("div");
    line2.className = "acct-row-kind";
    line2.textContent = kindLine(a, waiting);
    who.append(line1, line2);

    const u5 = document.createElement("div");
    u5.className = "acct-row-slot";
    const u7 = document.createElement("div");
    u7.className = "acct-row-slot";
    fillUsage(u5, u7, q, now);

    const act = document.createElement("div");
    act.className = "acct-row-act";
    const stop = (fn: () => void) => (ev: MouseEvent): void => {
      ev.stopPropagation();
      fn();
    };
    if (!readonly) {
      if (this.noWindow.has(key)) {
        const why = document.createElement("span");
        why.className = "acct-row-why";
        why.textContent = copyText("acctPage.login.noWindow");
        act.append(why);
        // 那台说了没有 tmux ⇒ 不出［在 tmux 里登录］（tmux 一律可选，不画成「要装」）。
        if (machineHasTmux(origin) !== false) act.appendChild(button({ label: copyText("acctPage.login.inTmux"), size: "compact", onClick: stop(() => void this.tmuxLogin(origin, a)) }));
      } else if (waiting) {
        act.appendChild(button({ label: copyText("acctPage.login.reopen"), size: "compact", onClick: stop(() => void this.login(origin, a)) }));
      } else if (!isDef) {
        const setDef = button({ label: copyText("acctPage.row.setDefault"), size: "compact", onClick: stop(() => void this.setDefault(origin, a)) });
        setDef.classList.add("acct-row-hoverbtn");
        act.appendChild(setDef);
      }
    }
    const more = button({
      label: copyText("acctPage.row.more", { name: a.name }),
      kind: "icon",
      icon: "more",
      size: "compact",
      onClick: (ev) => {
        ev.stopPropagation();
        openMenu({ el: ev.currentTarget as HTMLElement, align: "end" }, this.menu(f, a, readonly));
      },
    });

    row.append(accountAvatarEl(a.name, { size: 24 }), who, u5, u7, act, more);
    wrap.appendChild(row);
    const toggle = (): void => {
      if (this.open.has(key)) this.open.delete(key);
      else this.open.add(key);
      if (this.facts) this.paint(this.facts);
    };
    row.addEventListener("click", toggle);
    row.addEventListener("keydown", (ev) => {
      if ((ev.key === "Enter" || ev.key === " ") && ev.target === row) {
        ev.preventDefault();
        toggle();
      }
    });
    if (this.open.has(key)) wrap.appendChild(this.detail(f, a, readonly));
    return wrap;
  }

  private menu(f: Facts, a: Account, readonly: boolean): MenuItem[] {
    const origin = f.origin;
    const off = readonly ? { enabled: false, title: copyText("acctPage.offline.hover") } : {};
    const items: MenuItem[] = [];
    if (a.name !== f.state.meta?.effectiveDefault) items.push({ label: copyText("acctPage.row.setDefault"), onClick: () => void this.setDefault(origin, a), ...off });
    if (accountRowKind(a) === "apikey") items.push({ label: copyText("acctPage.menu.changeKey"), onClick: () => this.openDetail(a.name), ...off });
    else if (a.configDir !== null) items.push({ label: copyText("acctPage.menu.relogin"), onClick: () => void this.login(origin, a), ...off });
    const cmds = f.commands.get(a.name) ?? [];
    if (cmds.length > 0) items.push({ label: copyText("acctPage.menu.copyCommands"), onClick: () => copyCmds(cmds) });
    if (a.configDir !== null) {
      const dir = a.configDir;
      items.push({ label: revealLabel(origin), onClick: () => void this.reveal(origin, dir) });
      items.push({ divider: true, label: "" });
      items.push({ label: copyText("acctPage.menu.remove", { name: a.name }), danger: true, onClick: () => void this.remove(f, a), ...off });
    }
    return items;
  }

  private openDetail(name: string): void {
    this.open.add(this.key(name));
    if (this.facts) this.paint(this.facts);
  }

  /** 点开那一行：命令 · API key · 默认模型 · 账号目录 ·［删除 x…］。 */
  private detail(f: Facts, a: Account, readonly: boolean): HTMLElement {
    const origin = f.origin;
    const box = document.createElement("div");
    box.className = "acct-detail";
    const line = (label: string, ...value: Node[]): void => {
      const k = document.createElement("div");
      k.className = "acct-detail-key";
      k.textContent = label;
      const v = document.createElement("div");
      v.className = "acct-detail-val";
      v.append(...value);
      box.append(k, v);
    };
    const cmds = f.commands.get(a.name) ?? [];
    if (cmds.length > 0) {
      const copy = button({ label: copyText("acctPage.detail.copy"), kind: "icon", icon: "copy", size: "compact", onClick: () => copyCmds(cmds) });
      line(copyText("acctPage.detail.commands"), mono(cmds.join(copyText("kit.text.sep"))), copy);
    }
    if (accountRowKind(a) === "apikey") line(copyText("acctPage.detail.apikey"), this.keyEditor(origin, a, readonly));
    // 默认模型：键 ＝ 这台 ＋ 这个号（选中即存；第一项 ＝ 跟着这一家自己的默认）。选项是这一家认得的模型（后端画像）；
    // 盘上存的那个不在里面 ⇒ 照样多列它一项（不丢用户自己写的）。
    const profile = accountsAgentProfile();
    const model = document.createElement("select");
    model.className = "acct-detail-model";
    model.setAttribute("aria-label", copyText("acctPage.detail.model"));
    model.disabled = readonly;
    const fill = (saved: string | undefined): void => {
      const names = [...(profile?.models ?? [])];
      if (saved && !names.includes(saved)) names.push(saved);
      const opt = (value: string, label: string): HTMLOptionElement => {
        const o = document.createElement("option");
        o.value = value;
        o.textContent = label;
        return o;
      };
      model.replaceChildren(opt("", copyText("acctPage.detail.modelDefault", { agent: profile?.displayName ?? "" })), ...names.map((n) => opt(n, n)));
      model.value = saved ?? "";
    };
    fill(undefined);
    void getModelForAccount(origin, a.name).then((m) => {
      if (document.activeElement !== model) fill(m);
    });
    const modelErr = document.createElement("span");
    modelErr.className = "acct-detail-err";
    model.addEventListener("change", () => {
      void setModelForAccount(origin, a.name, model.value || null).then(
        () => (modelErr.textContent = ""),
        (e: unknown) => (modelErr.textContent = saidOfControl(e)),
      );
    });
    const scope = document.createElement("span");
    scope.className = "acct-detail-scope";
    scope.textContent = copyText("acctPage.detail.modelScope", { machine: machineName(origin), name: a.name });
    line(copyText("acctPage.detail.model"), model, scope, modelErr);
    if (a.configDir !== null) {
      const dir = a.configDir;
      line(copyText("acctPage.detail.dir"), mono(homeShort(dir, f.state.meta?.home)), button({ label: revealLabel(origin), kind: "ghost", size: "compact", onClick: () => void this.reveal(origin, dir) }));
      const del = button({ label: copyText("acctPage.menu.remove", { name: a.name }), kind: "danger-text", size: "compact", onClick: () => void this.remove(f, a) });
      if (readonly) setDisabled(del, copyText("acctPage.offline.hover"));
      const foot = document.createElement("div");
      foot.className = "acct-detail-foot";
      foot.appendChild(del);
      box.appendChild(foot);
    }
    return box;
  }

  /** API key 那一格：［更换…］就地展开 地址 ＋ key；保存失败错误句落在框下、填的不丢。key 只显示那台写完回的掩码（末四位）。 */
  private keyEditor(origin: Origin, a: Account, readonly: boolean): HTMLElement {
    const box = document.createElement("div");
    box.className = "acct-key";
    const shown = mono("");
    const host = document.createElement("span");
    host.className = "acct-key-host";
    const show = (masked: string | null, baseUrl: string | null): void => {
      shown.textContent = masked ?? copyText("acctPage.row.keyNotSaved");
      host.textContent = baseUrl ? hostOf(baseUrl) : "";
      host.hidden = !baseUrl;
    };
    show(a.keyMasked ?? null, a.baseUrl ?? null);
    const change = button({ label: copyText("acctPage.detail.keyChange"), size: "compact" });
    if (readonly) setDisabled(change, copyText("acctPage.offline.hover"));
    const form = document.createElement("div");
    form.className = "acct-key-form";
    form.hidden = true;
    const base = field({ label: copyText("acctNew.base.label"), placeholder: copyText("acctNew.base.example"), noteOnDemand: true });
    const keyF = field({ label: copyText("acctNew.key.label"), noteOnDemand: true });
    const keyIn = keyF.input as HTMLInputElement;
    keyIn.type = "password";
    keyIn.autocomplete = "off";
    const save = button({ label: copyText("acctPage.detail.keySave"), kind: "primary", size: "compact" });
    const cancel = button({ label: copyText("acctNew.form.cancel"), size: "compact" });
    form.append(base.root, keyF.root, buttonRow(cancel, save));
    change.addEventListener("click", () => {
      form.hidden = false;
      keyIn.focus();
    });
    cancel.addEventListener("click", () => {
      keyIn.value = "";
      form.hidden = true;
    });
    save.addEventListener("click", () => {
      const k = keyIn.value.trim();
      if (!k || a.configDir === null) return;
      const b = checkBaseUrl(base.input.value);
      if (!b.ok) {
        base.setError(b.reason);
        return;
      }
      base.setError(null);
      const dir = a.configDir;
      void (async () => {
        setBusy(save, copyText("acctPage.detail.keySaving"));
        try {
          const w = await writeApikeyKey(origin, dir, k, b.value);
          keyIn.value = "";
          form.hidden = true;
          show(w.masked || null, w.baseUrl);
          keyF.setError(null);
        } catch (e) {
          keyF.setError(saidOfControl(e));
        } finally {
          setBusy(save, null);
        }
      })();
    });
    box.append(shown, host, change, form);
    return box;
  }

  // ───────────────────────────── 动作 ─────────────────────────────

  private async setDefault(origin: Origin, a: Account): Promise<void> {
    try {
      await accountsSetDefault(origin, a.name);
      toast(copyText("acctPage.default.done", { name: a.name }), "");
      void emit(SETTINGS_APPLIED_EVENT);
    } catch (e) {
      failToast(copyText("acctPage.default.failed", { name: a.name }), e, { level: "error" });
    }
    await this.reload(true, true);
  }

  /** 删号（撤不回 ⇒ 确认框，默认焦点「取消」）：删的是默认号时多一句「之后新会话默认 X」（那台答的）。默认模型那一格一起删。 */
  private async remove(f: Facts, a: Account): Promise<void> {
    const origin = f.origin;
    const machine = machineName(origin);
    const cmds = f.commands.get(a.name) ?? [];
    const next = a.isDefault ? (f.state.meta?.nextDefault ?? null) : null;
    const yes = await this.confirm({
      title: copyText("acctPage.remove.title", { name: a.name, machine }),
      action: copyText("acctPage.remove.action", { name: a.name }),
      danger: true,
      body: next ? copyText("acctPage.remove.next", { name: next }) : undefined,
      rows: [
        {
          label: copyText("acctPage.remove.cut"),
          items: [copyText("acctPage.remove.login"), copyText("acctPage.remove.settings"), ...(cmds.length ? [copyText("acctPage.remove.commands", { names: cmds.join(" ") })] : [])],
        },
        { label: copyText("acctPage.remove.keep"), items: [copyText("acctPage.remove.sessions")] },
      ],
    });
    if (!yes) return;
    try {
      await accountsRemove(origin, { name: a.name, force: a.isDefault });
      await setModelForAccount(origin, a.name, null).catch(() => {});
      this.open.delete(this.key(a.name, origin));
      toast(copyText("acctPage.remove.done", { name: a.name, machine }), "");
      if (a.isDefault) void emit(SETTINGS_APPLIED_EVENT);
    } catch (e) {
      failToast(copyText("acctPage.remove.failed", { name: a.name }), e, { level: "error" });
    }
    await this.reload(true, true);
  }

  private async login(origin: Origin, a: Account, cmd?: string): Promise<void> {
    const key = this.key(a.name, origin);
    try {
      if ((await openLoginWindow(origin, a.name, cmd)) === "opened") {
        this.noWindow.delete(key);
        this.waiting.add(key);
      } else {
        this.noWindow.add(key);
      }
    } catch (e) {
      failToast(copyText("acctPage.login.failed", { name: a.name }), e, { level: "error" });
    }
    if (this.facts) this.paint(this.facts);
  }

  private async tmuxLogin(origin: Origin, a: Account): Promise<void> {
    if (a.configDir === null) return;
    const key = this.key(a.name, origin);
    try {
      await loginInTmux(origin, a.name, a.configDir);
      this.noWindow.delete(key);
      this.waiting.add(key);
    } catch (e) {
      failToast(copyText("acctPage.login.failed", { name: a.name }), e, { level: "error" });
    }
    if (this.facts) this.paint(this.facts);
  }

  private async reveal(origin: Origin, dir: string): Promise<void> {
    try {
      if (isLocalOrigin(origin)) {
        await revealInFolder(dir);
        return;
      }
      const host = findHostByOrigin((await readRemoteConfig()).hosts, origin);
      if (host) await openFileWindow(host, { dir });
    } catch (e) {
      failToast(copyText("acctPage.reveal.failed"), e, { level: "error" });
    }
  }

  // ───────────────────────────── 新建 ─────────────────────────────

  private toggleForm(): void {
    if (this.form) {
      void this.form.dismiss();
      return;
    }
    const origin = this.origin;
    const form = renderNewAccountForm(origin, machineName(origin), {
      onCreate: (req) => this.create(origin, req),
      onCancel: () => this.closeForm(),
      confirm: this.confirm,
    });
    this.form = form;
    this.formSlot.replaceChildren(form.root);
    this.newBtn.setAttribute("aria-expanded", "true");
    form.focus();
  }

  private closeForm(): void {
    this.form = null;
    this.formSlot.replaceChildren();
    this.newBtn.setAttribute("aria-expanded", "false");
  }

  /** 建好：订阅号开登录窗口（那一行「等待终端登录…」，登录完后端推一帧、行自己变「已登录」）；API 号 key 没存上 ⇒ 那一行展开。 */
  private async create(origin: Origin, req: NewAccountRequest): Promise<void> {
    const machine = machineName(origin);
    let change;
    try {
      change = await accountsAdd(origin, req);
    } catch (e) {
      failToast(copyText("acctPage.new.failed", { name: req.name }), e, { level: "error" });
      return;
    }
    this.closeForm();
    const key = this.key(req.name, origin);
    if (req.kind !== "api-key" && !req.credFile) {
      const got = await openLoginWindow(origin, req.name, change.loginCmd ?? undefined).catch(() => "noWindow" as const);
      if (got === "opened") this.waiting.add(key);
      else this.noWindow.add(key);
      // 下一行照后端回的那几句（如「已开的终端要重读别名」），界面不另拼。
      toast(got === "opened" ? copyText("acctPage.new.doneLogin", { name: req.name, machine }) : copyText("acctPage.new.done", { name: req.name, machine }), change.notes.join("\n"));
    } else {
      toast(copyText("acctPage.new.done", { name: req.name, machine }), [change.keyProblem, ...change.notes].filter((x): x is string => !!x).join("\n"), change.keyProblem ? { level: "error" } : {});
      if (change.keyProblem) this.open.add(key);
    }
    if (req.isDefault) void emit(SETTINGS_APPLIED_EVENT);
    await this.reload(true, true);
  }
}

// ───────────────────────────── 纯排版 ─────────────────────────────

/** 复制这个号的命令名（空格隔开）：写不进 ⇒ 报一条（不装作复制了）。 */
function copyCmds(cmds: readonly string[]): void {
  void writeClipboard(cmds.join(" ")).catch((e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" }));
}

function safeAgent(): string {
  try {
    return launchAgentId();
  } catch {
    return "";
  }
}

function mono(t: string): HTMLElement {
  const e = document.createElement("code");
  e.className = "acct-detail-mono";
  e.textContent = t;
  return e;
}

function revealLabel(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("acctPage.menu.revealLocal") : copyText("acctPage.menu.revealRemote");
}

/** 一个号在那台额度账上的显示态（出过数的那一格；账号库有、没出过数 ⇒ 无采样那一形）。 */
function quotaOf(quota: QuotaRead, agent: string, account: string): QuotaShow | null {
  const led = quota.accounts.find((x) => x.agent === agent && x.account === account);
  if (led) return led;
  const u = quota.unseen.find((x) => x.agent === agent && x.account === account);
  return u ? { kind: u.kind, state: "unseen", stale: false, slots: [], login: u.login } : null;
}

/** 第二行：`订阅 · {email}` ／ `API key · {地址}` ／ `订阅 · 等待终端登录…` ／ `订阅 · 未登录`。 */
function kindLine(a: Account, waiting: boolean): string {
  const kind = accountRowKind(a);
  if (kind === "apikey") return a.baseUrl ? copyText("acctPage.row.apikeyAt", { host: hostOf(a.baseUrl) }) : copyText("acctPage.row.apikey");
  if (waiting) return copyText("acctPage.row.waiting");
  if (kind === "notLoggedIn") return copyText("acctPage.row.notLoggedIn");
  return a.email ? copyText("acctPage.row.sub", { email: a.email }) : copyText("acctPage.row.subNoEmail");
}

/** `5h 63% ↻18:30` · `7d 41%`；用满 `5h ✕ ↻19:00`（红）· 被拒没用满 `5h 58% · 被拒`（红）；按量号 `按量`；没采样 `—`。 */
function fillUsage(u5: HTMLElement, u7: HTMLElement, q: QuotaShow | null, now: number): void {
  if (!q) return;
  if (q.kind === "api") {
    u5.textContent = copyText("acct.kind.api");
    if (q.state === "refused") u5.dataset.shade = "refused";
    return;
  }
  for (const [slot, cell] of [
    ["5h", u5],
    ["7d", u7],
  ] as const) {
    const x = q.slots.find((v) => v.slot === slot);
    const here = (q.limiting ?? "5h") === slot;
    const parts = [slotLabel(slot), slotText(q, slot)];
    if (x?.resetsAt !== undefined && x.resetsAt > now && (slot === "5h" || (here && q.state === "refused"))) parts.push(copyText("acct.reset.at", { at: x.resetsAtText ?? "" }));
    cell.textContent = parts.join(" ");
    if (x?.full || (here && q.state === "refused")) cell.dataset.shade = "refused";
    else if (here && (q.state === "near" || q.state === "overageInUse")) cell.dataset.shade = "warn";
    if (q.stale) cell.dataset.stale = "true";
  }
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
