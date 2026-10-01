// A3：设置面板「账号」组（多账号；账号库由那台机器的后端建立和维护）。占用原「远端」空占位组。
//
// 展示某台远端的账号列表（名/邮箱/mode/登录态/configDir/默认）+ 设为默认 / 复制 configDir /
// 刷新。**只读 + 改本机默认账号**（写 config.json，不碰远端 manifest、不注入、不重启——A4/A5）。
// 部署引导（未启用时）留 A6 填；本组先给出"如何启用"的说明与 manifest 路径。
//
// 设置窗独立于主窗、拿不到活跃会话，故用远端选择器（多台时下拉）。改默认账号后
// emit(SETTINGS_APPLIED_EVENT) 让主窗状态栏 chip 同步。
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { emit } from "@tauri-apps/api/event";
import { openTerminal } from "../terminal-open";
import { readApikeyStatus, writeApikeyKey, type ApikeyCredentialsStatus, type ApikeyRoutingView } from "../apikey-reads";
import { LOCAL_ACCOUNTS_COPY, deriveUi, currentWorkingAccount, isSelectable, accountStatusBadge, accountLoginActionLabel, localApikeyEndpointStateFor, type ApikeyEndpointState, type AccountsState, type Account } from "../accounts";
import { fetchAccounts, fetchLocalAccounts, fetchLocalApikeyRouting, fetchMachineApikeyRouting, invalidateAccountsCache } from "../account-reads";
import { setDefaultName, getModelForAccount, setModelForAccount } from "../account-prefs";
import { accountAvatarEl } from "../account-color";
import { readRemoteConfig, type RemoteHostConfig } from "../remote-config";
import { showActionFailureToast } from "../error-toast";
// 本机那一支新长的字全走文案表：一处取文，判据按表逐条量。
import { copyText } from "../copy-table";
// 本机那个串（后端那个本机表示），以及「本机刻意不开终端窗口」那句话的跨语言标记（唯一住址在 `remote-launch-run.ts`）。
import { LOCAL_ORIGIN as BACKEND_LOCAL_ORIGIN } from "../backend-policy";
import { isLocalOrigin, isRemoteOrigin, type Origin } from "../ipc/origin";
import { POSIX_NO_WINDOW_MARKER } from "../remote-launch-run";
// 别名那一块与用户级 PATH 那一格都搬去了机器页「本机 → 工具 → 别名」
//—— 两者是同一个问题（「这台机器的终端怎么找到 ccm」）的两条路。
// A2：新建账号那张表单。
import { renderNewAccountForm, type NewAccountRequest } from "./account-new-form";
import { SETTINGS_APPLIED_EVENT } from "./events";
import { recordFacet, LOCAL_MACHINE_KEY } from "./machine-status";
// 改账号库的那几件都问这一页那台机器的后端（本机远端同一条路）。
import {
  accountsAdd,
  accountsInit,
  accountsLoginCmd,
  accountsRemove,
  accountsRepair,
  accountsRollback,
  accountsVerify,
  validateAcctName,
  type AccountChange,
} from "../account-ops";
import { askConfirm } from "../ask-dialog";
import { machineName, saidOfControl } from "../control-said";

/**
 * apikey 那一格要显的**一个账号**。只带界面真正用得到的三样。
 *
 * ⚠⚠ `K-H2c` `KH2C1`：`configDir` 在前端是一个**不透明串** —— 前端一个字都不解析它，
 * 原样递给那条命令，由 Rust 用**全仓唯一那份规则**（`history::apikey_account_id_of_dir`）
 * 推出账号 id。前端自己从那个路径里取末段名，就是在长**第二份**规则，
 * ⚠ 这句话**刻意不写成代码形状** —— `accounts-section.vitest.ts` 里那条机检
 * （标题以「KH2C1 机检：前端一个字都不推账号 id」打头的那个 `it`）
 * 扫的是整份文件（含注释），写成代码形状会让它红在一句注释上。
 * 漂开的那天症状是「设置里说这个号用 apikey 表里那一行、起会话时没用上」，而两边看起来都没错。
 * 由那条机检钉着。
 *
 * ⚠ 上面两处原先都点着
 * `the_ui_never_derives_the_account_id_itself`〔散文墓碑〕—— **那个名字全仓零定义**。
 * 真正钉这件事的是一条**中文标题**的 `it`，它本来就没有 snake_case 名字，
 * 而这两处一直**当现状在说**。
 */
export interface ApikeyEditorAccount {
  /** 显示用的名字（**只用来显示**，绝不当成 id 递给后端）。 */
  name: string;
  /** 递给后端那条命令的不透明串。 */
  configDir: string;
  /** apikey 表里今天有没有它那一行 —— `KH2B7` 的答案，**后端算的**。 */
  routed: boolean;
}

/**
 * `K-H2a` `KS6` 前端那一半：**第三方 API key 那份文件**的一块（apikey 表，不是中转的东西 ——
 * 用户 09 月裁「中转层不要有账号，账号就账号、中转就中转」）。
 *
 * 🔴 关键二：**这一块里不再有账号下拉。**
 * 它原先自带一个 `<select>` 选「配给哪个账号」—— 于是「哪个账号」在界面上被问两次
 * （账号表一次、这里一次），那正是「apikey 端点表与账号 manifest 在后端是两份，界面照着抄成
 * 两个控件」。⇒ 配 key 变成**账号那一行自己的一格**（[`renderApikeyEditor`]），
 * 这一块只剩**文件本身**的事：
 *
 * - `KS9`：**把那份文件的路径显出来** —— 一个「能手编但没人知道在哪」的文件等于不能手编。
 * - `KS11`：权限过宽 / 查不出来时**在界面上出声**（件计划定的是「出声」不是「拒绝」）。
 * - 文件读坏了**不许静默当成「没配」**。
 * - `KH2C3`：**顶层那一把**（历史格式那一行）单独一行显 —— 它说的不是任何一个账号。
 */
export function renderApikeyFileBlock(status: ApikeyCredentialsStatus): HTMLElement {
  const box = document.createElement("div");
  box.className = "apikey-file-block";

  const title = document.createElement("div");
  title.className = "apikey-file-title";
  title.textContent = copyText("accounts.apikeyFile.title");
  box.appendChild(title);

  // `KS9`：路径要能被找到，人才改得动它。
  const where = document.createElement("div");
  where.className = "apikey-file-path";
  where.textContent = copyText("accounts.apikeyFile.path", { path: status.path });
  where.title = copyText("accounts.apikeyFile.editHint");
  box.appendChild(where);

  // `KS11`：过宽 / 查不出来要**在界面上显出来**。
  if (status.notice) {
    const warn = document.createElement("div");
    warn.className = "apikey-file-notice";
    warn.textContent = status.notice;
    box.appendChild(warn);
  }
  // 文件读坏了（人手编打错一个逗号）——**不许静默当成「没配」**。
  if (status.problem) {
    const bad = document.createElement("div");
    bad.className = "apikey-file-problem";
    bad.textContent = status.problem;
    box.appendChild(bad);
  }
  // ★ `KH2C3`：**顶层那一把**（历史格式那一行）单独一行显。它读得出来，
  //   但界面不再往那儿写 —— 与每一行上「这个号配没配」是**两件事**，不许合成一行。
  if (status.configured) {
    const legacy = document.createElement("div");
    legacy.className = "apikey-file-legacy";
    legacy.textContent = copyText("accounts.apikeyFile.legacyTop", { masked: status.masked });
    box.appendChild(legacy);
  }
  return box;
}

/**
 * 关键二：**某一个账号**的第三方 API key —— 账号那一行展开出来的一格。
 *
 * 「哪个账号」由它挂在哪一行回答，**不再有下拉**。
 *
 * # ★★ **永不回显**（`KS6`）
 *
 * 「配好之后，后端**永远**不把明文回给前端；界面只显示掩码或『已配置』。**要改就重新输。**」
 * ⇒ 结构上做不到回显，两道：
 *   ① 入参里**没有明文那个字段**（[`ApikeyEditorAccount`] 只有 name / configDir / routed）；
 *   ② 输入框**从不预填**（`value` 一次都不被赋非空值），存之前先清空。
 * 由 `accounts-section.vitest.ts` 里 `KS6` 那一族钉住，含一条**源码扫描**。
 */
export function renderApikeyEditor(
  a: ApikeyEditorAccount,
  onSave: (key: string, configDir: string) => void | Promise<void>,
): { editor: HTMLElement; toggle: HTMLButtonElement } {
  const box = document.createElement("div");
  box.className = "accounts-row-apikey";
  // 默认收着；由同一个函数里造的那颗按钮开合（`hidden` 与类名写在同一处，
  // `css-conventions` 的 S30 ⑦ 那把尺子才推得出它切的是哪个类）。
  box.hidden = true;
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "accounts-row-apikey-toggle";
  toggle.textContent = a.routed ? copyText("accounts.apikeyEditor.replace") : copyText("accounts.apikeyEditor.add");
  toggle.setAttribute("aria-expanded", "false");
  toggle.addEventListener("click", () => {
    box.hidden = !box.hidden;
    toggle.setAttribute("aria-expanded", String(!box.hidden));
  });

  const state = document.createElement("div");
  state.className = "accounts-row-apikey-state";
  state.textContent = a.routed
    ? copyText("accounts.apikeyEditor.hasRow", { name: a.name })
    : copyText("accounts.apikeyEditor.noRow", { name: a.name });
  box.appendChild(state);

  const input = document.createElement("input");
  input.type = "password";
  input.className = "accounts-row-apikey-input";
  input.autocomplete = "off";
  // ★★ **这里刻意什么都不做** —— 不预填、不 placeholder 回显掩码。
  input.placeholder = a.routed ? copyText("accounts.apikeyEditor.replaceHint") : copyText("accounts.apikeyEditor.pasteHint");
  box.appendChild(input);

  const save = mkBtn(copyText("accounts.apikeyEditor.save"));
  save.className = "accounts-row-apikey-save";
  save.addEventListener("click", () => {
    const v = input.value.trim();
    if (!v) return;
    // 先清空再交出去：**明文在 DOM 里停留的时间越短越好**（截图 / 录屏那两个出口）。
    input.value = "";
    void onSave(v, a.configDir);
  });
  box.appendChild(save);
  return { editor: box, toggle };
}

export class AccountsSection {
  readonly element: HTMLElement;
  private body: HTMLElement;
  private hosts: RemoteHostConfig[] = [];
  /** 在看哪台机器（本机 = `LOCAL_ORIGIN`；初值由 `init` 从共用 store 取）。 */
  private origin: Origin = BACKEND_LOCAL_ORIGIN;
  /** U7：维护区展开态。null=用户还没表态（默认折叠）；true/false=用户手动开合过，reload 后保持。 */
  private maintOpen: boolean | null = null;

  constructor() {
    const root = document.createElement("div");
    root.className = "settings-group settings-accounts";

    // 顶部：远端选择 + 刷新
    const bar = document.createElement("div");
    bar.className = "accounts-bar";
    // E59：**这里原来有一个 origin 下拉，已删。**
    //
    // 本分节只作为「机器详情页」上的一块存在（`panel.ts` 的 `perMachineBlocks`，
    // 唯一的构造点）。页头已经说了「你在看哪台机器」，分节里再放一个选择器就是**两层上下文**——
    // 而且它能指向与页头**不同**的那台，写动作又按分节自己的 `this.origin` 定目标
    // ⇒ **在标着 A 的页面上把东西写进 B**，`router.activeId` 仍是 A、界面上看不出来。
    //
    // 选「删」而不是「藏」是用户 2026-08-01 拍板的：这次重做的整条论证就是
    // 「机器是中心对象、上下文由页面给」，留一个能绕过页面上下文的入口，
    // 等于把地基判据降级成约定。⇒ `origin` **只能**来自共用 store。
    const refresh = document.createElement("button");
    refresh.type = "button";
    refresh.className = "accounts-refresh";
    refresh.textContent = copyText("accounts.ctor.refresh");
    refresh.addEventListener("click", () => {
      // 本机远端都清（与账号 chip 同一条：缓存的键就是 origin，本机也一样）。
      invalidateAccountsCache(this.origin);
      void this.reload(true);
    });
    bar.appendChild(refresh);
    root.appendChild(bar);

    this.body = document.createElement("div");
    this.body.className = "accounts-body";
    root.appendChild(this.body);

    this.element = root;

    /**
     * S4a：把本分节的 origin 选择接到**共用**的「当前在看哪台机器」store 上。
     *
     * 病因见 `machine-context.ts` 头注：这四块此前各维护一份 `this.origin`，
     * 用户在一处切了机器，另外三处还停在上一台 —— 而它们讲的是同一台机器。
     *
     * **收到 `null`（本机）**：原先这里写的是「本分节的下拉只列远端，收到 `null` 就原地不动」。
     * 下拉早删了（E59），本机那一支也早有了（`N-F1b`）⇒ `null` 就切到本机那一支
     * （见 [`followMachine`]）。
     */
    subscribeMachine((origin) => this.followMachine(origin));
    // ST1「延后加载」：构造期不再 `void this.init()` —— 见 `loadNow()`。
  }

  /**
   * ST1「延后加载」（**子页内容只在该子页可见时才发 I/O**）：
   * 构造期不再发 I/O；宿主（`panel.ts`）在**某台机器的子页第一次可见**时调它。
   * 重开设置后宿主会再调一次（重开要看新读数）。
   */
  loadNow(): void {
    void this.init();
  }

  private async init(): Promise<void> {
    try {
      const cfg = await readRemoteConfig();
      this.hosts = cfg.enabled ? cfg.hosts : [];
    } catch {
      this.hosts = [];
    }
    // 🔴 **初值只认共用 store**：`null` 就是本机（`machine-context.ts` 头注）。
    //    这里原先是 `getCurrentMachine() ?? pickPrimaryOrigin(...) ?? hosts[0]` —— E59 留的
    //    「兜底落点」。而 store 的初值恰好是 `null`（本机页一出现 per-machine 那几块就落在它上面，
    //    `panel.ts` 的 S4b-2 那句注释逐字说两者对齐）⇒ 配了远端的机器上，**本机页这一节显的是
    //    主远端的账号**，本机那一支（与 A3 的两条本机命令）在那种机器上一次都走不到。
    //    ⚠ 这里**不**拿主机清单去核 store 说的那台：清单读失败时 `hosts` 是空的，而页头说的
    //    那一台照样是那一台 —— 核了的话，读配置失败会让 aya 那一页显出本机的账号（比读不到更糟）。
    //    「认不认得」只在切机器那一跳上核（[`followMachine`]，E59 原样）。
    this.origin = getCurrentMachine();
    await this.reload(false);
  }

  /** 这台是不是已加载的主机清单里的一台。 */
  private knows(origin: string): boolean {
    return this.hosts.some((h) => (h.label || h.host) === origin);
  }

  /**
   * S4a：跟随共用 store 切机器。见构造里那段注释。
   *
   * `null`（本机）原先在这里**原地不动**（「本分节表示不了」）—— 那句话自 `N-F1b`
   * 起就不成立了（`origin` 为空时走本机那一支），留着它的后果是：从 aya 那一页切回本机页，
   * 这一节还停在 aya 的账号上。⇒ 本机也跟。
   */
  private followMachine(origin: Origin): void {
    // E59：判据从「在不在我自己的下拉里」改成「在不在已加载的主机清单里」——
    // 下拉没了，而这条判据本来问的就是「这台我认不认得」。
    if (isRemoteOrigin(origin) && !this.knows(origin)) return;
    if (this.origin === origin) return;
    this.origin = origin;
    void this.reload(true);
  }

  /**
   * 给状态账本记 `accounts` 那一格（远端按那台的名字，本机按 `LOCAL_MACHINE_KEY`）。
   * 读不出来 / 后端不在 / 没启用 / 读到了几个号 —— 每一档写法各不相同，「还差什么」那张清单才说得清缺的是哪一件。
   */
  private note(facet: "accounts", state: { kind: "ok" | "fail" | "na"; detail?: string }): void {
    recordFacet(isLocalOrigin(this.origin) ? LOCAL_MACHINE_KEY : this.origin, facet, state);
  }

  /** 〔缺口二〕空态里 `accounts` 那一格：没启用 ⇒ 缺；启用着（零个号）⇒ 读到了。 */
  private enabledFacet(enabled: boolean): { kind: "ok" | "fail"; detail: string } {
    return enabled
      ? { kind: "ok", detail: copyText("accounts.status.read") }
      : { kind: "fail", detail: copyText("accounts.status.multiOff") };
  }

  private async reload(force: boolean): Promise<void> {
    this.body.innerHTML = "";
    // 🔴 ST1「切机器 pending」：一次切机器 = 这一块重读一趟（远端是一次 SSH 往返）。
    //    原先这段时间这一块是**空的** —— 与「这台机器没有账号」在屏幕上分不开。
    //    ⇒ 先挂一行「正在读」，读回来（成或败）那一刻撤掉。
    const pending = document.createElement("div");
    pending.className = "accounts-info";
    pending.dataset.pending = "accounts";
    pending.setAttribute("aria-busy", "true");
    pending.textContent = copyText("accounts.reload.reading", { machine: isLocalOrigin(this.origin) ? copyText("accounts.who.local") : this.origin });
    this.body.appendChild(pending);
    try {
      await this.reloadInner(force);
    } finally {
      pending.remove();
    }
  }

  private async reloadInner(force: boolean): Promise<void> {
    if (isLocalOrigin(this.origin)) {
      // `N-F1b`：这里原先逐字印
      // 「没有已配置的远端。账号功能在远端 Linux 上——先在「连接」组配一台远端。」
      // 然后早返回。那句话在一台**本来就有账号**的机器上是一句坏话：它把「这一节
      // 没接上本机那条路」说成了「你缺一台远端 Linux」，而账号今天就读得出来
      //（定框 `N1` 09-05 订正段 / `N-F1` 摸底）。⇒ 换成走本机那条路。
      await this.reloadLocal(force);
      return;
    }
    let state: AccountsState;
    try {
      state = await fetchAccounts(this.origin, force);
    } catch (e) {
      this.note("accounts", { kind: "fail", detail: copyText("accounts.status.pullFailed") });
      this.info(copyText("accounts.status.pullFailedBody", { e: String(e) }));
      return;
    }
    const ui = deriveUi(state);
    switch (ui.kind) {
      // 🔴 `K-R59`：这里原来还有一支 `case "hidden"`，把账号那一格记成 `na`、理由「用户显式选的降级」。
      //    那一档（`daemonless`）整格没了 ⇒ 支也没了。
      case "needs-update":
        this.note("accounts", { kind: "fail", detail: copyText("accounts.status.backendOld") });
        this.info(copyText("accounts.status.backendOldBody", { reason: ui.reason }));
        return;
      // 没问出来 ≠ 要更新：照实说查询失败与原因。
      case "query-failed":
        this.note("accounts", { kind: "fail", detail: copyText("accounts.status.queryFailed") });
        this.info(copyText("accounts.status.queryFailedBody", { reason: ui.reason }));
        return;
      case "not-enabled":
        // 〔缺口二〕启用没启用记在 accounts（启用着只是零个号 ⇒ 读到了）。
        this.note("accounts", this.enabledFacet(state.meta?.enabled === true));
        this.renderInitWizard(this.body, ui.manifestPath, ui.reason, "remote");
        return;
      case "ready":
        this.note("accounts", { kind: "ok", detail: copyText("accounts.status.count", { n: ui.accounts.length }) });
        await this.renderTable(state, ui.accounts, ui.notice);
        return;
    }
  }

  /**
   * `N-F1b`：**本机页讲的是这台机器。** 本机那一支自己一条渲染路（本机那一页的字一句「远端」都不许出现），
   * 而改账号库那几件与远端同一条路：问本机后端（`account-ops.ts`，`origin` = 本机）。
   *
   * # 三个结局，一个都不许合并
   *
   * 读不出来 / 一个号都没有 / 有号 —— 前两个长得像但完全不是一回事：
   * 把「读不出来」渲染成「你没有账号」，用户会去启用一个他已经启用了的东西。
   * 由 `NF1bD1` 那一族的两条「诚实降级」判据钉着。每一档往账本里记的 `accounts` 那一格也各不相同。
   *
   * ⚠ **诚实边界**：前端分不出后端那三档里的 `NoBackend` 与 `Unreadable`（两者一起是 `available:false` ＋ 一句 `error`），
   * 所以这里的档名说的是**面板看得见的那三档**。
   */
  private async reloadLocal(force: boolean): Promise<void> {
    const box = document.createElement("div");
    box.className = "accounts-local";
    this.body.appendChild(box);
    AccountsSection.line(box, "accounts-meta accounts-local-head", LOCAL_ACCOUNTS_COPY.heading);

    let state: AccountsState;
    try {
      state = await fetchLocalAccounts(force);
    } catch (e) {
      // 档三：**读不动** —— 那条 Promise 直接 rejected，命令根本没跑通。
      this.note("accounts", { kind: "fail", detail: copyText("accounts.local.unreadable") });
      this.localFail(box, String(e));
      return;
    }
    if (!state.available) {
      // 档二：**后端不在** —— 后端答了「不可用」，那句原因在 `state.error` 里。
      this.note("accounts", { kind: "fail", detail: copyText("accounts.local.noBackend") });
      this.localFail(box, state.error ?? LOCAL_ACCOUNTS_COPY.unknownReason);
      return;
    }
    if (!state.meta?.enabled || state.accounts.length === 0) {
      // 档一的空态：没启用 ⇒ accounts 缺（启用没启用住这一格）；启用着只是零个号 ⇒ 读到了。
      this.note("accounts", this.enabledFacet(state.meta?.enabled === true));
      AccountsSection.line(box, "accounts-info accounts-local-empty-title", LOCAL_ACCOUNTS_COPY.emptyTitle);
      if (!state.meta?.enabled) {
        this.renderInitWizard(box, state.meta?.manifestPath ?? null, state.meta?.error ?? "", "local");
      } else {
        AccountsSection.line(box, "accounts-hint accounts-local-empty-next", LOCAL_ACCOUNTS_COPY.emptyNext);
        box.appendChild(this.localNewForm());
      }
      return;
    }

    // 档一：**读出来了**，而且这台机真的启用着隔离账号。
    this.note("accounts", { kind: "ok", detail: copyText("accounts.status.count", { n: state.accounts.length }) });
    AccountsSection.line(
      box,
      "accounts-meta accounts-local-count",
      `${state.accounts.length} ${LOCAL_ACCOUNTS_COPY.countSuffix} · ` +
        `${LOCAL_ACCOUNTS_COPY.manifestPrefix} ${state.meta.manifestPath}`,
    );
    // 「当前账号」在本机与远端是**同一格**（`config.json` 的 `accounts.defaultName`），
    // 所以这里就用那个既有的纯函数，不长第二套判定。
    const cur = currentWorkingAccount(state);
    const table = document.createElement("div");
    table.className = "accounts-local-table";
    // 本机这一半的两格事实（apikey 表里有没有它那一行 · 本机中转在不在跑）问后端要。
    const routing = await this.readLocalRouting(state.accounts);
    for (const a of state.accounts) {
      table.appendChild(
        this.localRow(a, cur?.name === a.name, routing ? localApikeyEndpointStateFor(a, routing) : undefined),
      );
    }
    box.appendChild(table);
    AccountsSection.line(box, "accounts-hint accounts-local-hint", copyText("accountsLocal.list.scope"));
    // 本机也能新建账号：与远端同一张表单、同一条 `accounts-add`，只是问的是本机后端。
    box.appendChild(this.localNewForm());
    box.appendChild(this.renderMaintenance());
  }

  /** 本机那一页的新建表单：两句提示换成本机的话（本机 Linux 不开终端窗口）。 */
  private localNewForm(): HTMLElement {
    return renderNewAccountForm(BACKEND_LOCAL_ORIGIN, (req) => this.createAccount(req), {
      subscription: copyText("accountsLocal.new.subscriptionHint"),
      apikey: copyText("accountsLocal.new.apikeyHint"),
    });
  }

  /**
   * 读不出来时那一格。
   *
   * ⚠ **原因一定要带出来**：一个不说原因的失败，用户修不了；
   * 而 `fetchLocalAccounts` 的两条失败路（invoke 抛错 / `available:false`）
   * 都带着后端给的那句话。
   */
  private localFail(box: HTMLElement, why: string): void {
    AccountsSection.line(
      box,
      "accounts-info accounts-local-fail",
      `${LOCAL_ACCOUNTS_COPY.loadFailed}：${why}`,
    );
  }

  /** 一行文本。`cls` 里第一个类名一律取既有的那几个（`accounts-info` / `accounts-hint` / `accounts-meta`），第二个是本机那一支自己的钩子。 */
  private static line(parent: HTMLElement, cls: string, text: string): HTMLElement {
    const el = document.createElement("div");
    el.className = cls;
    el.textContent = text;
    parent.appendChild(el);
    return el;
  }

  /**
   * 本机那两格事实：问本机后端（经通道 `apikey-routing`）。
   *
   * ⚠ **问不到就是 `null`，不是「表里没有」**：`null` 让徽章走「没被告知 ⇒ 不替它下判断」那一支；
   * 当成空表的话，一个其实配好了的号会被说成「apikey 凭据文件里没有这个账号的一行」。
   * 回来的形状不对（桥接层异常）同样按「没问到」算。
   */
  private async readLocalRouting(accounts: Account[]): Promise<ApikeyRoutingView | null> {
    const dirs = accounts.map((a) => a.configDir).filter((d): d is string => !!d);
    if (dirs.length === 0) return null;
    try {
      const r = await fetchLocalApikeyRouting(dirs);
      return Array.isArray(r?.routed) && typeof r?.running === "boolean" ? r : null;
    } catch {
      return null;
    }
  }

  /**
   * 本机清单里的一行。**只读** —— 这一件不做切号，也不做加号。
   *
   * 徽章的 `endpoint` 从这一拍起**传本机那一半**（`{ scope: "local", … }`，
   * 由 `localApikeyEndpointStateFor` 从后端答的两格事实摊出来）。原先这里不传，理由是「那要多一条 IPC，属下一件」
   * —— 那一问早在盘上了，只是这一支没去问（今天经通道问 `apikey-routing`）。
   * 问不到 / 账号 0（没有 configDir）⇒ 仍然不传，徽章照旧「不替它下判断」。
   * 🔴 **千万别顺手传 `{ scope: "remote" }`** —— 那会让一台本机的号被解释成远端那一半，
   * 文案里当场出现「远端」两个字；`NF1bD2` 那条判据正是钉这个的。
   */
  private localRow(a: Account, isCurrent: boolean, relay?: ApikeyEndpointState): HTMLElement {
    const row = document.createElement("div");
    row.className = isCurrent ? "accounts-local-row current" : "accounts-local-row";
    row.appendChild(accountAvatarEl(a.name, { size: 16, ghost: !isSelectable(a) }));
    AccountsSection.line(row, "accounts-local-row-name", a.name);
    AccountsSection.line(row, "accounts-local-row-email", a.email);

    const badge = accountStatusBadge(a, relay);
    const badgeEl = AccountsSection.line(
      row,
      badge.warn ? "accounts-local-row-badge warn" : "accounts-local-row-badge",
      badge.text,
    );
    if (badge.title) badgeEl.title = badge.title;

    // 账号 0（`mode: "bare"`）没有 configDir —— 那一格空着，不许编一个出来。
    AccountsSection.line(row, "accounts-local-row-dir", a.configDir ?? "");
    if (isCurrent) {
      AccountsSection.line(row, "accounts-local-row-mark", LOCAL_ACCOUNTS_COPY.currentMark);
    }
    // 有自己目录的号才有「去登录」「删除」（账号 0 没有目录；in-place 那种旧号的目录就是共享库，删不得）。
    if (a.configDir && a.mode !== "in-place") {
      const acts = document.createElement("span");
      acts.className = "accounts-local-row-actions";
      const login = mkBtn(accountLoginActionLabel(a).label);
      login.addEventListener("click", () => void this.loginAccount(a));
      acts.appendChild(login);
      const del = mkBtn(copyText("accounts.row.remove"));
      del.classList.add("danger");
      del.addEventListener("click", () => void this.removeAccount(a));
      acts.appendChild(del);
      row.appendChild(acts);
    }
    return row;
  }

  /**
   * 在终端里起 claude 登录一个号（那一行是那台后端答的，claude 自己的登录界面）。
   * 远端：弹一个终端连过去跑；本机 Linux 刻意不开窗口（按后端自己的声明判，不按 OS 猜）⇒ 那一行复制给人自己跑。
   */
  private async openLogin(origin: Origin, cmd: string): Promise<void> {
    try {
      await openTerminal(origin, cmd);
      showActionFailureToast(copyText("accounts.login.launched"), copyText("accounts.login.launchedNext"), {
        level: "info",
        durationMs: 5000,
      });
    } catch (err) {
      let copied = true;
      try {
        await navigator.clipboard.writeText(cmd);
      } catch {
        copied = false; // 那一行在提示里照样看得见，可以手动复制
      }
      const byDesign = String(err).includes(POSIX_NO_WINDOW_MARKER);
      const headline = byDesign
        ? copied
          ? copyText("accountsLocal.new.noWindowCopied")
          : copyText("accountsLocal.new.noWindowNotCopied")
        : copied
          ? copyText("accountsLocal.new.failedCopied")
          : copyText("accountsLocal.new.failedNotCopied");
      showActionFailureToast(headline, copyText("accountsLocal.new.pasteBody", { reason: String(err), cmd }), {
        level: byDesign ? "info" : "error",
        durationMs: 10000,
      });
    }
  }

  /** 行上的「去登录」：问那台后端要登录那一行，交给开终端那一步。 */
  private async loginAccount(a: Account): Promise<void> {
    const origin = this.machineOrigin();
    let cmd: string;
    try {
      cmd = await accountsLoginCmd(origin, a.name);
    } catch (e) {
      showActionFailureToast(copyText("accounts.login.failed"), saidOfControl(e), { level: "error" });
      return;
    }
    await this.openLogin(origin, cmd);
  }

  /** 行上的「删除」：确认（默认号多说一句）→ 那台后端删（只删它自己的目录，先备份）。 */
  private async removeAccount(a: Account): Promise<void> {
    const origin = this.machineOrigin();
    const machine = machineName(origin);
    const msg = a.isDefault
      ? copyText("accounts.remove.confirmDefault", { machine, name: a.name })
      : copyText("accounts.remove.confirm", { machine, name: a.name });
    if (!(await askConfirm(msg))) return;
    try {
      const done = await accountsRemove(origin, a.isDefault ? { name: a.name, force: true } : { name: a.name });
      this.changed(copyText("accounts.remove.done", { name: a.name }), done);
    } catch (e) {
      showActionFailureToast(copyText("accounts.remove.failed"), saidOfControl(e), { level: "error" });
    }
  }


  /**
   * A6：未启用 → 内联「启用多账号」：给现在这个登录起个名字 → 那台后端预演（将要做的那几步上屏）→ 确认 → 它建库。
   * 本机远端同一张（`where` 只换引言那一句：本机那一页不说「远端」）。
   */
  private renderInitWizard(parent: HTMLElement, manifestPath: string | null, reason: string, where: "local" | "remote"): void {
    const box = document.createElement("div");
    box.className = "accounts-not-enabled";

    const h = document.createElement("div");
    h.className = "accounts-ne-title";
    h.textContent = where === "local" ? copyText("accountsLocal.init.title") : copyText("accounts.notEnabled.title");
    box.appendChild(h);

    const p = document.createElement("div");
    p.className = "accounts-ne-body";
    p.textContent =
      where === "local"
        ? copyText("accountsLocal.init.intro", { path: manifestPath ?? copyText("accounts.notEnabled.noPath") })
        : copyText("accounts.notEnabled.intro", { reason, path: manifestPath ?? copyText("accounts.notEnabled.noPath") });
    box.appendChild(p);

    const wiz = document.createElement("div");
    wiz.className = "accounts-wizard";
    const field = document.createElement("div");
    field.className = "accounts-wiz-field";
    const label = document.createElement("label");
    label.textContent = copyText("accounts.notEnabled.defaultName");
    const input = document.createElement("input");
    input.type = "text";
    input.className = "accounts-wiz-name";
    input.placeholder = copyText("accounts.notEnabled.defaultNameHint");
    field.append(label, input);
    const err = document.createElement("div");
    err.className = "accounts-wiz-err";
    field.appendChild(err);
    wiz.appendChild(field);

    // 将要做的那几步（那台后端预演的原话，只读）。
    const preview = document.createElement("pre");
    preview.className = "accounts-wiz-preview";
    wiz.appendChild(preview);

    const btns = document.createElement("div");
    btns.className = "accounts-wiz-btns";
    const bApply = mkBtn(copyText("accounts.notEnabled.enable"));
    bApply.classList.add("danger");
    btns.append(bApply);
    wiz.appendChild(btns);

    const origin = this.machineOrigin();
    // 输入一变就问一次预演，只认最后一次的答案（序号，零定时器）。
    let asked = 0;
    let planned: AccountChange | null = null;
    const sync = (): void => {
      const name = input.value.trim();
      const v = validateAcctName(name);
      err.textContent = name && !v.ok ? v.reason : "";
      bApply.disabled = true;
      planned = null;
      const my = ++asked;
      if (!v.ok) {
        preview.textContent = copyText("accounts.init.previewEmpty");
        return;
      }
      void accountsInit(origin, { name, dryRun: true }).then(
        (plan) => {
          if (my !== asked) return;
          planned = plan;
          preview.textContent = [copyText("accountNewForm.form.willRun"), ...plan.steps, ...plan.notes].join("\n");
          bApply.disabled = false;
        },
        (e: unknown) => {
          if (my !== asked) return;
          preview.textContent = copyText("accounts.init.previewEmpty");
          err.textContent = saidOfControl(e);
        },
      );
    };
    input.addEventListener("input", sync);
    bApply.addEventListener("click", () => {
      const name = input.value.trim();
      const plan = planned;
      if (!plan || !validateAcctName(name).ok) return;
      void (async () => {
        const msg = copyText("accounts.init.confirm", { machine: machineName(origin), name, steps: stepList(plan) });
        if (!(await askConfirm(msg))) return;
        bApply.disabled = true;
        try {
          const done = await accountsInit(origin, { name });
          this.changed(copyText("accounts.init.done", { name }), done);
        } catch (e) {
          showActionFailureToast(copyText("accounts.init.failed"), saidOfControl(e), { level: "error" });
          bApply.disabled = false;
        }
      })();
    });
    sync();

    box.appendChild(wiz);
    parent.appendChild(box);
  }


  /**
   * account-ux U7：顶部「当前账号」横幅——把 chip / tab 徽章上那个概念在设置里讲清楚:
   * 它管什么(新会话 + 没指定过账号的 resume)、不管什么(正在跑的会话)。
   * 当前账号**不可选**(未登录 / in-place / 目录缺失)时不装作有——那种状态下账号徽章的
   * "不一致"判定本来就不生效(见 accounts.ts currentAccountForBadge),横幅得如实说,否则用户
   * 会以为它在生效。
   */
  private renderCurrentBanner(def: Account | null): HTMLElement {
    const box = document.createElement("div");
    box.className = "accounts-current-banner";
    // def 为 null 在 ready 分支下**不可达**（deriveUi 保证 accounts.length ≥ 1，effectiveDefault
    // 则 find(isDefault) ?? accounts[0]）——这里只是防御，不给它编一套"未设置"的假文案。
    const usable = def !== null && isSelectable(def);
    if (!usable) box.classList.add("unusable"); // 语义=当前账号存在但不可用（非"没有当前账号"）

    // 不可用时用 U5 已有的 ghost 态（"软/不作数"的既有视觉词汇），别再造新概念。
    if (def) box.appendChild(accountAvatarEl(def.name, { size: 18, ghost: !usable }));

    const main = document.createElement("div");
    main.className = "accounts-current-main";
    const name = document.createElement("span");
    name.className = "accounts-current-name";
    name.textContent = def ? def.name : copyText("accounts.banner.none");
    main.appendChild(name);
    if (def?.email) {
      const email = document.createElement("span");
      email.className = "accounts-current-email";
      email.textContent = def.email;
      main.appendChild(email);
    }
    box.appendChild(main);

    const scope = document.createElement("span");
    scope.className = "accounts-current-scope";
    scope.textContent = usable
      ? copyText("accounts.banner.scope")
      : def
        ? copyText("accounts.banner.unavailable")
        : copyText("accounts.banner.pick");
    box.appendChild(scope);
    return box;
  }

  private async renderTable(
    state: AccountsState,
    accounts: Account[],
    notice: string | null = null,
  ): Promise<void> {
    const def = currentWorkingAccount(state);
    this.body.appendChild(this.renderCurrentBanner(def));
    // Z01：**能用但有缺**（那台后端旧到看不见账号 0）。列表本身是好的，
    // 所以不走 needs-update 那条整体降级——但也**绝不静默**：少一行账号用户看不出来。
    if (notice) {
      const n = document.createElement("div");
      n.className = "accounts-hint accounts-hint-warn";
      n.textContent = notice;
      this.body.appendChild(n);
    }
    const meta = state.meta;
    if (meta) {
      const info = document.createElement("div");
      info.className = "accounts-meta";
      info.textContent = copyText("accounts.table.summary", { n: accounts.length, path: meta.manifestPath, updated: meta.updatedAt ? copyText("accounts.table.updatedAt", { updatedAt: meta.updatedAt }) : "" });
      this.body.appendChild(info);
    }
    // 🔴 关键二：apikey 是**账号那一行自己的一格** ⇒ 画表之前先问清
    //    「哪几个号在 apikey 表里有一行」（`K-H2c` 口径①：问后端要，前端不推 id）。
    const apikey = await this.readApikeyState(accounts);
    const table = document.createElement("div");
    table.className = "accounts-table";
    for (const a of accounts) {
      const { row, editor } = await this.accountRow(
        a,
        def?.name === a.name,
        apikey.entries.find((e) => e.configDir === a.configDir) ?? null,
      );
      table.appendChild(row);
      if (editor) table.appendChild(editor);
    }
    this.body.appendChild(table);

    const hint = document.createElement("div");
    hint.className = "accounts-hint";
    // 管辖范围那句已由上方横幅说了（U7 前这里是唯一出处）——这里只留横幅**没说**的部分，
    // 别在同一屏里把同一句话逐字重复两遍。
    hint.textContent =
      copyText("accounts.table.switchScope");
    this.body.appendChild(hint);

    // F09 Phase D 审计（UX，建议）：批量对齐这个能力随 F09 整体删除后，没有任何地方告诉
    // 老用户它去哪了——用户在 Ctrl+K 里搜不到会以为是 bug。加一行最低成本的静态提示，
    // 别为这一件事新建一整套提示基础设施。
    //
    // 🔴 `N-F3`（09-05）订正：这条原来的头一句逐字写着「**本仓库没有任何 changelog/首次运行
    // 提示机制**」——**今天那是假话**，而且写下它的时候就已经不全对了。
    //
    // ⚠ 「有几处」也是在报一个数 ⇒ **分母与量法都写出来**（量于 `2afe176`）：
    //   分母 = `src` 下 205 个生产 `.ts`（去 `*.vitest.ts` / `*.test.ts`）；
    //   词表 `首次运行|first-run|onboarding|向导|wizard` ⇒ **原始命中 16 处 / 8 个文件**。
    //   16 处里**逐条看过**，属于「首次运行提示机制」的只有 1 处：
    //     · `main.ts:606/612`（`2afe176` 上）—— 命令 chip 的首运行微高亮，
    //       `LS_KEYS.cmdkHintSeen` ＋ `safeGet/safeSet`，非模态、见过即不再。
    //   其余 15 处是别的意思的「向导 / wizard / onboarding」（A6 启用多账号的内联向导 ·
    //   F08 安装向导 · 推公钥免密 · 部署向导措辞…）—— 它们是**配置向导**，
    //   不是「第一次打开时说点什么」。⚠ 这一分归类是**判断**不是读数，别当机检结果读。
    //   `N-F3` 本件之后再加 1 处：`first-run-hint.ts`（主窗口那条「还差什么」指路）。
    //
    // ⚠ 但**这一行的结论不变**：那两处都不是 changelog，「某个能力去哪了」今天仍然没有住址，
    //    所以下面这行静态提示照留。订正的是那句全称，不是这段的做法。
    const removedHint = document.createElement("div");
    removedHint.className = "accounts-hint";
    removedHint.textContent =
      copyText("accounts.table.bulkGone");
    this.body.appendChild(removedHint);

    // K-H2a：第三方 API key 那份**文件**（路径 / 权限 / 读坏了 / 顶层那一把）。
    // 挂在账号这一组里 —— 它是「用哪个身份打上游」这件事的一部分。配 key 本身在每一行上。
    this.body.appendChild(apikey.fileBlock);

    // 🔴 （A2）：**新建账号是一张常驻的表单**，不再藏在「维护」折叠组里、
    //    也不再是红色按钮。岔口（订阅 / 第三方 apikey）在表单里问。
    this.body.appendChild(renderNewAccountForm(this.origin, (req) => this.createAccount(req)));

    this.body.appendChild(this.renderMaintenance());
  }

  /**
   * A2：表单交上来一个新账号 ⇒ 那台后端一趟做完（建目录 · 链接 · 清单 · 凭据或 key · 别名）。
   * 订阅号没导入凭据 ⇒ 接着在终端里起 claude 登录（那一行也是后端答的）；API 号的 key 没写进去 ⇒ 说出来，在那一行重填。
   */
  private async createAccount(req: NewAccountRequest): Promise<void> {
    const origin = this.machineOrigin();
    let got: AccountChange;
    try {
      got = await accountsAdd(origin, req);
    } catch (e) {
      showActionFailureToast(copyText("accounts.add.failed"), saidOfControl(e), { level: "error" });
      return;
    }
    this.changed(copyText("accounts.add.done", { name: req.name }), got);
    if (got.keyProblem) showActionFailureToast(copyText("accounts.add.keyFailed"), got.keyProblem, { level: "error" });
    if (got.loginCmd) await this.openLogin(origin, got.loginCmd);
  }


  /**
   * 这一页此刻显的是哪台机器 —— apikey 那几条命令按它定目标（本机逐字送 `"<local>"`）。
   * 先前那三条命令不收 origin，远端页配的 key 落在本机。
   */
  private machineOrigin(): Origin {
    return this.origin ?? BACKEND_LOCAL_ORIGIN;
  }

  /** 唯一那一处把 key 交给后端的地方（行上的「保存」与表单的「建好后写」都走它）。 */
  private async writeApikey(
    key: string,
    configDir: string,
    name: string,
    // 只有表单「建好后写」那一路带它；行上的「保存」只配 key（后端那一格不碰）。
    baseUrl?: string,
  ): Promise<void> {
    try {
      // 经通道交那台机器的后端（`apikey-key-set`，账号 id 由后端推）；先前是 Tauri 命令 `write_apikey_credentials_key`〔散文墓碑〕。
      await writeApikeyKey(this.machineOrigin(), configDir, key, baseUrl);
      showActionFailureToast(copyText("accounts.writeApikey.done"), copyText("accounts.writeApikey.doneBody", { name }), {
        level: "info",
        durationMs: 3000,
      });
      void this.reload(true);
    } catch (e) {
      showActionFailureToast(copyText("accounts.writeApikey.failed"), String(e));
    }
  }

  /**
   * 读一次 apikey 那份文件的状态，并问后端「这几个号在不在 apikey 表里」。
   *
   * ⚠ **读失败不许静默**：凭据读不到时用户可能正打算配它 ⇒ 失败就把失败显出来
   * （文件那一块换成一句读不到的原因；每一行上配 key 那一格照常能用 —— 写不依赖读）。
   *
   * # ⚠ `K-H2c` 三条口径，一条都别省
   *
   * ① **「这个号在不在 apikey 表里」是问后端要的**（`KH2B7` 那条既有命令），
   *    前端不推账号 id、也不读那份凭据文件。它失败**不挡配 key** ——
   *    那只影响状态那一行的措辞，而配 key 本身是这一格存在的理由。
   * ② **没有 `configDir` 的账号（账号 0）不给这一格**：起会话那一侧对它逐字回 `None`
   *    （`apikey_account_id` 头注：「说不出 id 就不注入」）⇒ 给它配一把 key 是配了也不生效。 〔散文墓碑〕
   * ③ 这一页显的是 `this.origin` 那台机器的账号，读写那份文件的两条命令
   *    （读：经通道 `apikey-read`；写：经通道 `apikey-key-set`）**按同一台机器**去
   *    （[`machineOrigin`]）—— 远端页读写的是那台机器上那一份，不再是本机的。
   *    「有没有行」（经通道 `apikey-routing`）同样问这一页那台机器。
   */
  private async readApikeyState(
    accounts: Account[],
  ): Promise<{ entries: ApikeyEditorAccount[]; fileBlock: HTMLElement }> {
    const dirs = accounts.map((a) => a.configDir).filter((d): d is string => !!d);
    let routed: string[] = [];
    try {
      // 问**这一页那台机器**（远端由那台的后端答），不再问本机。
      routed = dirs.length ? (await fetchMachineApikeyRouting(this.machineOrigin(), dirs)).routed : [];
    } catch {
      // 口径①：这一格失败只让状态那一行说「还没有它那一行」，不挡配 key。
    }
    const entries: ApikeyEditorAccount[] = accounts
      .filter((a): a is Account & { configDir: string } => !!a.configDir)
      .map((a) => ({ name: a.name, configDir: a.configDir, routed: routed.includes(a.configDir) }));
    let fileBlock: HTMLElement;
    try {
      fileBlock = renderApikeyFileBlock(await readApikeyStatus(this.machineOrigin()));
    } catch (e) {
      fileBlock = document.createElement("div");
      fileBlock.className = "apikey-file-problem";
      fileBlock.textContent = copyText("accounts.readApikey.failed", { e: String(e) });
    }
    return { entries, fileBlock };
  }

  /**
   * 维护区（默认折叠）：核对 · 修复 · 回滚 —— 都问这一页那台机器的后端。核对只读，结果就地列出来；
   * 修复与回滚先预演、把将要做的那几步给人看一眼，确认了才让那台后端做（它先备份再改）。
   */
  private renderMaintenance(): HTMLElement {
    const wrap = document.createElement("details");
    wrap.className = "accounts-maint-wrap";
    // 用户手动开合过就以用户的选择为准（reload 会重建 DOM，不记住的话展开态会被吞掉）。
    wrap.open = this.maintOpen ?? false;
    wrap.addEventListener("toggle", () => {
      this.maintOpen = wrap.open;
    });
    const summary = document.createElement("summary");
    summary.textContent = copyText("accounts.maintenance.title");
    wrap.appendChild(summary);

    const box = document.createElement("div");
    box.className = "accounts-maint";
    const ops = document.createElement("div");
    ops.className = "accounts-maint-ops";
    const out = document.createElement("div");
    out.className = "accounts-maint-report";
    const verifyBtn = mkBtn(copyText("accounts.maintenance.verify"));
    verifyBtn.addEventListener("click", () => void this.runVerify(verifyBtn, out));
    const repairBtn = mkBtn(copyText("accounts.maintenance.sync"));
    repairBtn.title = copyText("accounts.maintenance.syncHint");
    repairBtn.addEventListener("click", () => void this.runRepair(repairBtn));
    const rollbackBtn = mkBtn(copyText("accounts.maintenance.rollback"));
    rollbackBtn.title = copyText("accounts.maintenance.rollbackHint");
    rollbackBtn.addEventListener("click", () => void this.runRollback(rollbackBtn));
    ops.append(verifyBtn, repairBtn, rollbackBtn);
    box.append(ops, out);
    wrap.appendChild(box);
    return wrap;
  }

  /** 核对一遍，逐条列出来（+ 通过 · ! 提示 · x 要修 · - 跳过）。 */
  private async runVerify(btn: HTMLButtonElement, out: HTMLElement): Promise<void> {
    btn.disabled = true;
    out.innerHTML = "";
    try {
      const r = await accountsVerify(this.machineOrigin());
      const head = document.createElement("div");
      head.className = r.pass ? "accounts-verify-head" : "accounts-verify-head warn";
      head.textContent = r.pass
        ? copyText("accounts.verify.pass", { warns: String(r.warns) })
        : copyText("accounts.verify.fail", { fails: String(r.fails), warns: String(r.warns) });
      out.appendChild(head);
      const mark = { ok: "+", warn: "!", fail: "x", skip: "-" } as const;
      for (const c of r.checks) {
        const line = document.createElement("div");
        line.className = `accounts-verify-check ${c.level}`;
        line.textContent = c.account ? `${mark[c.level]} ${c.account}：${c.text}` : `${mark[c.level]} ${c.text}`;
        out.appendChild(line);
      }
    } catch (e) {
      showActionFailureToast(copyText("accounts.verify.failed"), saidOfControl(e), { level: "error" });
    } finally {
      btn.disabled = false;
    }
  }

  /** 修复：预演 → 没事可做就说一声；有事可做 ⇒ 列出那几步、确认了才做。 */
  private async runRepair(btn: HTMLButtonElement): Promise<void> {
    const origin = this.machineOrigin();
    btn.disabled = true;
    try {
      const plan = await accountsRepair(origin, { dryRun: true });
      if (plan.steps.length === 0) {
        showActionFailureToast(copyText("accounts.repair.nothing"), plan.notes.join("\n") || copyText("accounts.repair.nothingBody"), {
          level: "info",
          durationMs: 4000,
        });
        return;
      }
      const msg = copyText("accounts.repair.confirm", { machine: machineName(origin), steps: stepList(plan) });
      if (!(await askConfirm(msg))) return;
      const done = await accountsRepair(origin, {});
      this.changed(copyText("accounts.repair.done"), done);
    } catch (e) {
      showActionFailureToast(copyText("accounts.repair.failed"), saidOfControl(e), { level: "error" });
    } finally {
      btn.disabled = false;
    }
  }

  /** 回滚最近一次改动（预演 → 确认 → 做）。 */
  private async runRollback(btn: HTMLButtonElement): Promise<void> {
    const origin = this.machineOrigin();
    btn.disabled = true;
    try {
      const plan = await accountsRollback(origin, { dryRun: true });
      const msg = copyText("accounts.rollback.confirm", {
        machine: machineName(origin),
        backup: plan.backup ?? "",
        steps: stepList(plan),
      });
      if (!(await askConfirm(msg))) return;
      const done = await accountsRollback(origin, plan.backup ? { backup: plan.backup } : {});
      this.changed(copyText("accounts.rollback.done"), done);
    } catch (e) {
      showActionFailureToast(copyText("accounts.rollback.failed"), saidOfControl(e), { level: "error" });
    } finally {
      btn.disabled = false;
    }
  }

  /** 一趟改动做完：说一句（改了几步 · 备份叫什么 · 别名那一步有话就带上）、清缓存、重读。 */
  private changed(title: string, c: AccountChange): void {
    const lines = [
      c.backup ? copyText("accounts.change.backup", { n: String(c.steps.length), backup: c.backup }) : copyText("accounts.change.nothing"),
      ...c.notes,
    ];
    if (c.aliases?.note) lines.push(c.aliases.note);
    showActionFailureToast(title, lines.join("\n"), { level: "info", durationMs: 6000 });
    invalidateAccountsCache(this.machineOrigin());
    void this.reload(true);
  }


  /**
   * 表里的一行。`apikey` 非空 ⇒ 这个号能配第三方 API key：操作列多一颗按钮，
   * 点开的那一格（[`renderApikeyEditor`]）作为**这一行紧后面的兄弟**挂进表里，
   * 不算这一行的列（`.accounts-row` 的子元素数 == grid 列数那条契约不动）。
   */
  private async accountRow(
    a: Account,
    isCurrent: boolean,
    apikey: ApikeyEditorAccount | null = null,
  ): Promise<{ row: HTMLElement; editor: HTMLElement | null }> {
    const model = await getModelForAccount(a.name); // F07：每账号默认模型偏好
    const row = document.createElement("div");
    row.className = "accounts-row";
    if (isCurrent) row.classList.add("current");

    const mark = document.createElement("span");
    mark.className = "accounts-row-mark";
    mark.textContent = isCurrent ? copyText("accounts.row.currentIcon") : "";
    mark.title = isCurrent ? copyText("accounts.row.current") : "";
    row.appendChild(mark);

    // account-ux U7：复用 U4 的账号头像——与状态栏 chip、tab 徽章同一套 hash 色，三处肉眼可对应。
    row.appendChild(accountAvatarEl(a.name, { size: 16 }));

    const name = document.createElement("span");
    name.className = "accounts-row-name";
    name.textContent = a.name;
    row.appendChild(name);

    const email = document.createElement("span");
    email.className = "accounts-row-email";
    email.textContent = a.email || copyText("accounts.row.none");
    row.appendChild(email);

    // K-A1：三态（逃生口 / api-key（未配置端点）/ 未登录 / 已登录）的取值住
    // `accounts.ts::accountStatusBadge` —— **这里不再自己判**。
    // 原因不是「抽一层好看」：状态栏 chip 的账号菜单（`account-chip.ts`）渲染的是同一个
    // 概念，两处各写一遍的结果是 `KA6a` 那段文案只改得动一处，而用户从另一处看到的
    // 还是「已登录」。
    const badge = document.createElement("span");
    badge.className = "accounts-row-badge";
    // `K-H2b` `KH2B7`：**这张表是远端专用的** —— `reload` 在 `this.origin` 为空时
    // 直接早退（「账号功能在远端 Linux 上」），所以这里渲染的每一行都来自远端那一半。
    // ⇒ 显式告诉徽章是哪一半：远端那一半本件明写不做（`§0e` 裁四），
    //   它的 hover 该说「只给本机配、远端这一半还不做」，而不是一句不分半边的全称。
    const status = accountStatusBadge(a, { scope: "remote" });
    badge.textContent = status.text;
    if (status.warn) badge.classList.add("warn");
    if (status.title) badge.title = status.title;
    row.appendChild(badge);

    const dir = document.createElement("span");
    dir.className = "accounts-row-dir";
    // Z01：账号 0 没有 config dir——它**就是**「不设 CLAUDE_CONFIG_DIR」这个状态。
    // 显示它的真实含义，别显示空白，更别显示一个空串路径。
    dir.textContent = a.configDir ?? copyText("accounts.row.baseDir");
    dir.title =
      a.configDir ??
      copyText("accounts.row.baseHint");
    row.appendChild(dir);

    const actions = document.createElement("span");
    actions.className = "accounts-row-actions";
    // F07：每账号默认模型偏好——自由文本（模型 ID 会随时间变化，不硬编码枚举）；空 = 跟随该
    // 账号自身默认，不下发 override。保存写本机 config.json，不碰远端/manifest（同 defaultName）。
    // Phase D 审计（UX）：此前保存无任何反馈（同文件其余动作都有 toast，这里是唯一的例外）+
    // 保存失败会真正无声消失（设置窗口没有主窗那个全局 unhandledrejection 兜底，见 main.ts）。
    // 已按 selectDefault 的既有模式补齐 try/catch + toast，且失败时保留原值只提示不落盘（配合
    // `setModelForAccount` 的写入点校验，防止非法值落盘后拖垮该账号往后**所有**会话拉起）。
    const modelInput = document.createElement("input");
    modelInput.type = "text";
    modelInput.className = "accounts-row-model";
    modelInput.placeholder = copyText("accounts.row.model");
    modelInput.title =
      copyText("accounts.row.modelHint");
    modelInput.value = model ?? "";
    let lastSaved = model ?? "";
    const saveModel = async (): Promise<void> => {
      const next = modelInput.value.trim();
      if (next === lastSaved) return; // 值未变，别在每次失焦都弹一次噪音 toast
      try {
        await setModelForAccount(a.name, next || null);
        lastSaved = next;
        showActionFailureToast(
          next ? copyText("accounts.model.saved") : copyText("accounts.model.cleared"),
          next ? copyText("accounts.model.savedBody", { name: a.name, next }) : copyText("accounts.model.clearedBody", { name: a.name }),
          { level: "info", durationMs: 3000 },
        );
      } catch (e) {
        // 校验失败（非法字符集）等——不落盘，保留用户已输入的文本以便就地修正。
        showActionFailureToast(copyText("accounts.model.failed"), String(e), { level: "error" });
      }
    };
    modelInput.addEventListener("blur", () => void saveModel());
    modelInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        modelInput.blur(); // 触发上面的 blur 保存，行为统一（不重复实现一遍保存逻辑）
      }
    });
    actions.appendChild(modelInput);
    if (isSelectable(a) && !isCurrent) {
      const setDef = document.createElement("button");
      setDef.type = "button";
      setDef.textContent = copyText("accounts.row.setDefault");
      setDef.addEventListener("click", () => void this.selectDefault(a));
      actions.appendChild(setDef);
    }
    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = copyText("accounts.row.copyPath");
    copy.addEventListener("click", () => {
      const text = a.configDir ?? "";
      if (!text) {
        showActionFailureToast(copyText("accounts.row.baseNoDir"), copyText("accounts.row.baseNoDirBody"), {
          level: "info",
          durationMs: 3000,
        });
        return;
      }
      void navigator.clipboard?.writeText(text).then(
        () => showActionFailureToast(copyText("accounts.copy.done"), text, { level: "info", durationMs: 2500 }),
        () => showActionFailureToast(copyText("accounts.copy.failed"), copyText("accounts.copy.noClipboard"), { level: "error" }),
      );
    });
    actions.appendChild(copy);
    // A6：在终端里起 claude 登录这个号（那一行由那台后端出）——对 in-place 逃生口与账号 0 不给（没有自己的目录）。
    if (a.mode !== "in-place" && a.configDir) {
      const login = document.createElement("button");
      login.type = "button";
      // K-A1：对 api-key 号说「去登录」是假话（它不需要 /login，/login 也修不了缺端点）。
      const action = accountLoginActionLabel(a);
      login.textContent = action.label;
      login.title = action.title;
      login.addEventListener("click", () => void this.loginAccount(a));
      actions.appendChild(login);
      // 删号：红色（`§4.3` ②：红色留给删账号），确认框里说清删的是哪台机器上的哪个号。
      const del = document.createElement("button");
      del.type = "button";
      del.className = "danger";
      del.textContent = copyText("accounts.row.remove");
      del.addEventListener("click", () => void this.removeAccount(a));
      actions.appendChild(del);
    }
    // 🔴 关键二：**「哪个账号」只问一次** —— 配 apikey 是这一行自己的一格。
    let editor: HTMLElement | null = null;
    if (apikey) {
      const ed = renderApikeyEditor(apikey, (key, configDir) =>
        this.writeApikey(key, configDir, a.name),
      );
      editor = ed.editor;
      actions.appendChild(ed.toggle);
    }
    row.appendChild(actions);
    return { row, editor };
  }

  private async selectDefault(a: Account): Promise<void> {
    try {
      await setDefaultName(a.name);
      // audit-fixes I5：`defaultName` 全局单值 → 切它清**所有 origin** 缓存（非当前 origin 否则 ≤30s 用旧账号）。
      invalidateAccountsCache();
      await this.reload(true);
      void emit(SETTINGS_APPLIED_EVENT); // 让主窗状态栏 chip 同步
      showActionFailureToast(
        copyText("accounts.setDefault.done"),
        copyText("accounts.setDefault.doneBody", { name: a.name }),
        { level: "info", durationMs: 4000 },
      );
    } catch (e) {
      showActionFailureToast(copyText("accounts.setDefault.failed"), String(e), { level: "error" });
    }
  }

  private info(text: string): void {
    const p = document.createElement("div");
    p.className = "accounts-info";
    p.textContent = text;
    this.body.appendChild(p);
  }
}

/** A6 向导用的按钮工厂（type=button，避免 form 默认提交）。 */
function mkBtn(text: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = text;
  return b;
}

/** 预演那几步 ⇒ 确认框里的一段（一行一步）。 */
function stepList(c: AccountChange): string {
  return [...c.steps, ...c.notes].map((l) => `· ${l}`).join("\n");
}
