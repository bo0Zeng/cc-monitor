// A3：设置面板「账号」组（多账号 cc-acct-iso）。占用原「远端」空占位组。
//
// 展示某台远端的账号列表（名/邮箱/mode/登录态/configDir/默认）+ 设为默认 / 复制 configDir /
// 刷新。**只读 + 改本机默认账号**（写 config.json，不碰远端 manifest、不注入、不重启——A4/A5）。
// 部署引导（未启用时）留 A6 填；本组先给出"如何启用"的说明与 manifest 路径。
//
// 设置窗独立于主窗、拿不到活跃会话，故用远端选择器（多台时下拉）。改默认账号后
// emit(SETTINGS_APPLIED_EVENT) 让主窗状态栏 chip 同步。
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { emit } from "@tauri-apps/api/event";
import { commands } from "../ipc/commands";
import {
  fetchAccounts,
  deriveUi,
  currentWorkingAccount,
  selectableAccounts,
  isSelectable,
  setDefaultName,
  getModelForAccount,
  setModelForAccount,
  invalidateAccountsCache,
  type AccountsState,
  type Account,
} from "../accounts";
import { fetchAccountUsage, OK_USAGE_UNVERIFIED_CAVEAT, type AccountUsageOutcome } from "../account-usage";
import { pickPrimaryOrigin } from "../account-chip";
import { accountAvatarEl } from "../account-color";
import { readRemoteConfig, type RemoteHostConfig } from "../remote-config";
import { showActionFailureToast } from "../error-toast";
import { buildPasteBlock } from "../paste-block"; // T03：待贴文本统一组件（Z05 复用它）
import { SETTINGS_APPLIED_EVENT } from "./events";
// Phase G：这两格此前**没有任何生产者**，见下面 `note()` 的注释。
import { recordFacet } from "./machine-status";
import {
  buildAcctIsoCmd,
  validateAcctName,
  deriveAcctIsoDir,
  type AcctIsoStep,
} from "./acct-deploy";

export class AccountsSection {
  readonly element: HTMLElement;
  private body: HTMLElement;
  private hosts: RemoteHostConfig[] = [];
  private origin: string | null = null;
  /** U7：维护区展开态。null=用户还没表态（按账号数给默认）；true/false=用户手动开合过，reload 后保持。 */
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
    refresh.textContent = "刷新";
    refresh.addEventListener("click", () => {
      if (this.origin) invalidateAccountsCache(this.origin);
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
     * **不能表示「本机」时怎么办**：本分节的下拉只列远端。收到 `null`（本机）就**原地不动**，
     * 不乱选一台。这是已知的半截状态，S4b 的机器详情页会从根上解决它
     * （本机那一页压根不会包含只对远端有意义的分节）。
     */
    subscribeMachine((origin) => this.followMachine(origin));
    void this.init();
  }

  private async init(): Promise<void> {
    try {
      const cfg = await readRemoteConfig();
      this.hosts = cfg.enabled ? cfg.hosts : [];
    } catch {
      this.hosts = [];
    }
    // 填远端下拉（含 daemonless，但标注）
    // E59：初值仍取「主 origin」作为**兜底落点**（`RemoteSection` 抛异常时这几块会留在
    // 列表页上，那儿没有页上下文）。正常路径上，`subscribeMachine` 立刻会把它改成页头那台。
    this.origin =
      getCurrentMachine() ??
      pickPrimaryOrigin(this.hosts) ??
      (this.hosts[0]?.label || this.hosts[0]?.host || null);
    await this.reload(false);
  }

  /** S4a：跟随共用 store 切机器。见构造里那段注释。 */
  private followMachine(origin: string | null): void {
    if (origin === null) return; // 本机：本分节表示不了，原地不动
    // E59：判据从「在不在我自己的下拉里」改成「在不在已加载的主机清单里」——
    // 下拉没了，而这条判据本来问的就是「这台我认不认得」。
    if (!this.hosts.some((h) => (h.label || h.host) === origin)) return;
    if (this.origin === origin) return;
    this.origin = origin;
    void this.reload(true);
  }

  /**
   * Phase G：给状态账本记一格。
   *
   * **这里此前是个洞**：`MACHINE_FACETS` 有 5 格，而全仓 `recordFacet` 的生产者只覆盖
   * 3 格（machine-card 的 connection/daemon/ccm）—— `acctIso` 与 `accounts` **一个写点都没有**。
   * 后果不是「少两个格子」，而是**「还差什么」那张清单在任何真实安装上都清不空**：
   * 每台机器恒定产出 ≥2 条 `unknown` ⇒ `summarizeGaps` 恒非 null ⇒
   * `remote-section` 里「全绿就整块不出现」那一支是**死代码**。
   * 一张自称「还差什么、点哪里补齐」却既补不齐也消不掉的清单，比不做这个功能更糟。
   */
  private note(
    facet: "acctIso" | "accounts",
    state: { kind: "ok" | "fail" | "na"; detail?: string },
  ): void {
    if (!this.origin) return; // 本机：这两格由 L3b 补，今天表示不了
    recordFacet(this.origin, facet, state);
  }

  private async reload(force: boolean): Promise<void> {
    this.body.innerHTML = "";
    if (!this.origin) {
      this.info("没有已配置的远端。账号功能在远端 Linux 上——先在「连接」组配一台远端。");
      return;
    }
    let state: AccountsState;
    try {
      state = await fetchAccounts(this.origin, force);
    } catch (e) {
      this.note("accounts", { kind: "fail", detail: "拉取失败" });
      this.info(`拉取账号失败：${String(e)}`);
      return;
    }
    const ui = deriveUi(state);
    switch (ui.kind) {
      case "hidden":
        // daemonless **不是缺**：用户显式选的降级，读不到账号是它的定义而非故障。
        this.note("accounts", { kind: "na", detail: "daemonless" });
        this.note("acctIso", { kind: "na", detail: "daemonless" });
        this.info("该远端配置为 daemonless，无法读取账号。");
        return;
      case "needs-update":
        this.note("accounts", { kind: "fail", detail: "daemon 需更新" });
        this.info(`远端 daemon 需要更新才能用多账号：${ui.reason}`);
        return;
      case "not-enabled":
        // 读得到、但多账号管线没启用 ⇒ accounts 这一格算读到了，acctIso 那格是真的缺。
        this.note("accounts", { kind: "ok", detail: "已读取" });
        this.note("acctIso", { kind: "fail", detail: "未启用" });
        void this.renderNotEnabledFlow(ui.manifestPath, ui.reason);
        return;
      case "ready":
        this.note("accounts", { kind: "ok", detail: `${ui.accounts.length} 个` });
        this.note("acctIso", { kind: "ok", detail: "已启用" });
        await this.renderTable(state, ui.accounts, ui.notice);
        return;
    }
  }

  /**
   * A6：在远端终端里跑一个部署/维护步骤——构建命令（校验失败即提示不动手）→ danger 步二次确认 →
   * `launch_remote_terminal` 弹真实终端让用户看着跑（DESIGN §6，不经 daemon、不代跑）。
   */
  private async launchStep(
    step: AcctIsoStep,
    opts: { danger?: boolean; confirmExtra?: string } = {},
  ): Promise<void> {
    if (!this.origin) return;
    const built = buildAcctIsoCmd(step);
    if (!built.ok) {
      showActionFailureToast("命令无法生成", built.reason, { level: "error" });
      return;
    }
    if (opts.danger) {
      const msg =
        `将在远端「${this.origin}」的终端里运行：\n\n${built.cmd}\n\n` +
        (opts.confirmExtra ? `${opts.confirmExtra}\n\n` : "") +
        `命令在你看得见的终端里执行、需你亲手确认；工具自带备份，可 rollback。继续？`;
      if (!window.confirm(msg)) return;
    }
    try {
      await commands.launch_remote_terminal({ origin: this.origin, remoteCmd: built.cmd });
      showActionFailureToast("已在终端里运行", "完成后点「刷新」更新账号列表。", {
        level: "info",
        durationMs: 5000,
      });
    } catch (e) {
      showActionFailureToast("拉起终端失败", String(e), { level: "error" });
    }
  }

  /** 当前选中远端对应的 host 配置（多账号 IPC 要传 cfg=RemoteHostConfig）。 */
  private currentHost(): RemoteHostConfig | null {
    if (!this.origin) return null;
    return (
      this.hosts.find((h) => (h.label || h.host) === this.origin) ?? this.hosts[0] ?? null
    );
  }

  /**
   * F5：未启用态先探测远端有没有装 cc-acct-iso。没装 → 显「一键部署」（而非直接甩 init 命令让它
   * command not found）；装了（或探测失败，别把用户堵死）→ 走现有 init 向导。
   */
  private async renderNotEnabledFlow(
    manifestPath: string | null,
    reason: string,
  ): Promise<void> {
    const host = this.currentHost();
    if (host) {
      try {
        // 探测不依赖 dest（D 审计 S2/S5：只 command -v 一次 exec，任何配置下都能判 installed）。
        const status = await commands.check_remote_acct_iso({ cfg: host });
        if (!status.installed) {
          const dest = deriveAcctIsoDir(host.daemonPath, host.user);
          this.renderNeedsDeploy(host, dest);
          return;
        }
      } catch (e) {
        console.warn("check_remote_acct_iso failed, fall through to wizard:", e);
      }
    }
    this.renderNotEnabled(manifestPath, reason);
  }

  /** F5：远端没装 cc-acct-iso → 一键部署（vendored 内嵌 → sftp 推 → 装软链，不碰 rc）。 */
  private renderNeedsDeploy(host: RemoteHostConfig, dest: string | null): void {
    const box = document.createElement("div");
    box.className = "accounts-needs-deploy";

    const h = document.createElement("div");
    h.className = "accounts-ne-title";
    h.textContent = "该远端还没装多账号管线（cc-acct-iso）";
    box.appendChild(h);

    // dest 推不出（缺 daemonPath 且 user 缺失/非法）→ 给不出一键部署落点，退回文字指引，不留死角。
    if (!dest) {
      const p = document.createElement("div");
      p.className = "accounts-ne-desc";
      p.textContent =
        "多账号靠 cc-acct-iso（每账号一个隔离配置目录、数据共享）。这台远端缺 daemonPath / 用户名，" +
        "自动推不出部署目录——请先在「连接」组填好远端 user / daemonPath，再回来一键部署。";
      box.appendChild(p);
      this.body.appendChild(box);
      return;
    }

    const p = document.createElement("div");
    p.className = "accounts-ne-desc";
    p.textContent =
      `多账号靠 cc-acct-iso（每账号一个隔离配置目录、数据共享）。点下面一键把它部署到远端 ` +
      `${dest}（只软链到 ~/.local/bin，不改你的 ~/.bashrc）。装完再回来启用。`;
    box.appendChild(p);

    const btn = mkBtn("一键部署 cc-acct-iso");
    btn.addEventListener("click", () => {
      btn.disabled = true;
      const prev = btn.textContent;
      btn.textContent = "部署中…";
      void commands
        .deploy_remote_acct_iso({ cfg: host, destDir: dest })
        .then(
          (msg) => {
            showActionFailureToast("已部署 cc-acct-iso", msg, {
              level: "info",
              durationMs: 6000,
            });
            void this.reload(true);
          },
          (e) => {
            showActionFailureToast("部署 cc-acct-iso 失败", String(e), { level: "error" });
            btn.disabled = false;
            btn.textContent = prev;
          },
        );
    });
    box.appendChild(btn);
    this.body.appendChild(box);
  }

  /** A6：未启用 → 内联「启用多账号」向导（无 modal）：填默认账号名 → 预览命令 → 分步弹终端。 */
  private renderNotEnabled(manifestPath: string | null, reason: string): void {
    const box = document.createElement("div");
    box.className = "accounts-not-enabled";

    const h = document.createElement("div");
    h.className = "accounts-ne-title";
    h.textContent = "该远端尚未启用多账号";
    box.appendChild(h);

    const p = document.createElement("div");
    p.className = "accounts-ne-body";
    p.innerHTML =
      `在远端跑 cc-acct-iso 迁移管线即可启用（各账号独立凭据、skills/记忆/历史/设置实时共享）。<br>` +
      `原因：${escapeHtml(reason)}<br>manifest 查找位置：<code>${escapeHtml(manifestPath ?? "—")}</code>`;
    box.appendChild(p);

    const wiz = document.createElement("div");
    wiz.className = "accounts-wizard";

    // 默认账号名输入 + 实时校验。
    const field = document.createElement("div");
    field.className = "accounts-wiz-field";
    const label = document.createElement("label");
    label.textContent = "默认账号名（迁移现有默认账号进来）：";
    const input = document.createElement("input");
    input.type = "text";
    input.className = "accounts-wiz-name";
    input.placeholder = "例如 z";
    field.appendChild(label);
    field.appendChild(input);
    const err = document.createElement("div");
    err.className = "accounts-wiz-err";
    field.appendChild(err);
    wiz.appendChild(field);

    // 命令预览（只读，可复制）。
    const preview = document.createElement("pre");
    preview.className = "accounts-wiz-preview";
    wiz.appendChild(preview);
    const copyRow = document.createElement("div");
    copyRow.className = "accounts-wiz-copyrow";
    const copyBtn = mkBtn("复制命令");
    copyBtn.addEventListener("click", () => {
      void navigator.clipboard?.writeText(preview.textContent ?? "").then(
        () => showActionFailureToast("已复制命令", "可粘到远端终端里跑。", { level: "info", durationMs: 2500 }),
        () => showActionFailureToast("复制失败", "剪贴板不可用", { level: "error" }),
      );
    });
    copyRow.appendChild(copyBtn);
    wiz.appendChild(copyRow);

    // 分步按钮。
    const btns = document.createElement("div");
    btns.className = "accounts-wiz-btns";
    const bPreview = mkBtn("① 预览计划（dry-run）");
    const bApply = mkBtn("② 执行迁移（--apply）");
    bApply.classList.add("danger");
    const bVerify = mkBtn("③ 自检 verify");
    const bShellinit = mkBtn("④ 打印 rc 片段");
    btns.append(bPreview, bApply, bVerify, bShellinit);
    wiz.appendChild(btns);

    const note = document.createElement("div");
    note.className = "accounts-wiz-note";
    note.innerHTML =
      "<small>顺序：① 看计划（什么都不动）→ ② 执行迁移 → ③ 自检 → ④ 把打印的片段贴进 " +
      "<code>~/.bashrc</code>（工具不改你的 rc）→ 回来点「刷新」。第二个账号可在启用后用「加账号」导入。</small>";
    wiz.appendChild(note);

    // —— 校验驱动的启用/禁用 + 预览 ——
    const sync = (): void => {
      const name = input.value.trim();
      const v = validateAcctName(name);
      const valid = v.ok;
      err.textContent = name && !v.ok ? v.reason : "";
      for (const b of [bPreview, bApply, bShellinit]) b.disabled = !valid;
      // verify 不依赖名字（自检当前状态），恒可点。
      const pv = valid ? buildAcctIsoCmd({ kind: "init-preview", name }) : null;
      const ap = valid ? buildAcctIsoCmd({ kind: "init-apply", name }) : null;
      preview.textContent =
        pv && pv.ok && ap && ap.ok
          ? `# 预览计划（零落盘）\n${pv.cmd}\n\n# 执行迁移\n${ap.cmd}\n\n# 打印 rc 片段\ncc-acct-iso shellinit`
          : "（填入合法账号名后显示将运行的命令）";
    };
    input.addEventListener("input", sync);
    bPreview.addEventListener("click", () =>
      void this.launchStep({ kind: "init-preview", name: input.value.trim() }),
    );
    bApply.addEventListener("click", () =>
      void this.launchStep(
        { kind: "init-apply", name: input.value.trim() },
        { danger: true, confirmExtra: "这会把现有默认账号的凭据与 .claude.json 搬进账号库、其余项建 symlink。" },
      ),
    );
    bVerify.addEventListener("click", () => void this.launchStep({ kind: "verify" }));
    bShellinit.addEventListener("click", () => void this.launchStep({ kind: "shellinit" }));
    sync();

    box.appendChild(wiz);
    this.body.appendChild(box);
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
    name.textContent = def ? def.name : "未设当前账号";
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
      ? "新会话 · 没指定过账号的 resume 用它；正在跑的会话不受影响"
      : def
        ? "该账号当前不可用（未登录 / 逃生口 / 目录缺失）——先修好它，账号徽章与对齐才会生效"
        : "下面选一个账号「设为当前账号」";
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
    // Z01：**能用但有缺**（远端 daemon / cc-acct-iso 旧到看不见账号 0）。列表本身是好的，
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
      info.textContent = `已启用 · ${accounts.length} 个账号 · manifest ${meta.manifestPath}${meta.updatedAt ? ` · 更新于 ${meta.updatedAt}` : ""}`;
      this.body.appendChild(info);
    }
    const table = document.createElement("div");
    table.className = "accounts-table";
    for (const a of accounts) {
      table.appendChild(await this.accountRow(a, def?.name === a.name));
    }
    this.body.appendChild(table);

    const hint = document.createElement("div");
    hint.className = "accounts-hint";
    // 管辖范围那句已由上方横幅说了（U7 前这里是唯一出处）——这里只留横幅**没说**的部分，
    // 别在同一屏里把同一句话逐字重复两遍。
    hint.textContent =
      "切换当前账号只改本机设置：不动远端、不碰凭据、不重启任何东西。未登录的账号可点它那行的「去登录」在终端里 /login。";
    this.body.appendChild(hint);

    // F09 Phase D 审计（UX，建议）：本仓库没有任何 changelog/首次运行提示机制，批量对齐这个
    // 能力随 F09 整体删除后没有任何地方告诉老用户它去哪了——用户在 Ctrl+K 里搜不到会以为是
    // bug。加一行最低成本的静态提示，别为这一件事新建一整套提示基础设施。
    const removedHint = document.createElement("div");
    removedHint.className = "accounts-hint";
    removedHint.textContent =
      "提示：批量对齐（曾经的「⚠k」「⇄」和命令面板里的对齐命令）已下线，请在会话右键菜单的「Restart」里逐个切换账号。";
    this.body.appendChild(removedHint);

    // U8：数的是**可选**账号数,不是总数——1 个 isolated + 1 个 in-place 逃生口时总数=2 但
    // 你其实还只有一个能用的号,此刻"加第二个账号"仍是正路。与 accountColorsActive 同源判据。
    this.body.appendChild(this.renderMaintenance(selectableAccounts(state).length));
  }

  /** A6：已启用态的「维护」区——加账号 / 自检 / 补链，均弹终端。
   *  account-ux U7：整块收进 `<details>` **默认折叠**——三项都低频且带 danger（加账号会动远端
   *  目录、补链会改软链），常驻展开既占版面又把危险操作摆在手边。内部结构一行未改。 */
  private renderMaintenance(selectableCount: number): HTMLElement {
    const wrap = document.createElement("details");
    wrap.className = "accounts-maint-wrap";
    // 默认展开态**按状态给**，不是常量：刚跑完 A6 部署向导回来正好是「ready + 只有 1 个账号」，
    // 此刻用户唯一该做的下一步就是"加第二个账号"（否则多账号隔离白装了），把唯一的正路藏进
    // 折叠等于死胡同。稳态（≥2 个账号）仍默认折叠——那三项都低频且带 danger。
    // 用户手动开合过就以用户的选择为准（reload 会重建 DOM，不记住的话展开态和输入会被吞掉）。
    wrap.open = this.maintOpen ?? selectableCount < 2;
    wrap.addEventListener("toggle", () => {
      this.maintOpen = wrap.open;
    });
    const summary = document.createElement("summary");
    summary.textContent = "维护（加账号 / 自检 / 补链）";
    wrap.appendChild(summary);

    const box = document.createElement("div");
    box.className = "accounts-maint";

    // 加账号：内联小表单（名 + 可选凭据快照路径）→ 弹终端 add --apply。
    const addForm = document.createElement("div");
    addForm.className = "accounts-maint-add";
    const nameIn = document.createElement("input");
    nameIn.type = "text";
    nameIn.placeholder = "新账号名（如 b）";
    nameIn.className = "accounts-maint-name";
    const credIn = document.createElement("input");
    credIn.type = "text";
    credIn.placeholder = "可选：旧凭据快照路径（免重登）";
    credIn.className = "accounts-maint-cred";
    const addBtn = mkBtn("加账号…");
    addBtn.classList.add("danger");
    const addErr = document.createElement("span");
    addErr.className = "accounts-maint-err";
    const syncAdd = (): void => {
      const v = validateAcctName(nameIn.value.trim());
      addBtn.disabled = !v.ok;
      addErr.textContent = nameIn.value.trim() && !v.ok ? v.reason : "";
    };
    nameIn.addEventListener("input", syncAdd);
    addBtn.addEventListener("click", () => {
      const name = nameIn.value.trim();
      const credFile = credIn.value.trim() || undefined;
      void this.launchStep(
        { kind: "add-apply", name, credFile },
        {
          danger: true,
          confirmExtra: credFile
            ? "将新建该账号 config-dir 并从指定快照导入凭据（免重登）。"
            : "将新建该账号 config-dir；随后在弹出的终端里用它 /login。",
        },
      );
    });
    addForm.append(nameIn, credIn, addBtn, addErr);
    box.appendChild(addForm);
    syncAdd();

    // 自检 / 补链。
    const ops = document.createElement("div");
    ops.className = "accounts-maint-ops";
    const verifyBtn = mkBtn("自检 verify");
    verifyBtn.addEventListener("click", () => void this.launchStep({ kind: "verify" }));
    const syncBtn = mkBtn("补链 sync");
    syncBtn.addEventListener("click", () =>
      void this.launchStep(
        { kind: "sync-apply" },
        { danger: true, confirmExtra: "补齐/修复共享库软链、修权限、刷新 manifest 邮箱（幂等）。" },
      ),
    );
    const rcBtn = mkBtn("生成 rc 片段…");
    rcBtn.title =
      "抓远端 `cc-acct-iso shellinit` 的输出，给你一段可贴的 rc 片段（贴了之后裸 claude 走默认账号，"
      + "每个账号有 <名>cc 函数，账号 0 有 0cc 逃生口）。**只读，不会替你写任何文件。**";
    const rcBox = document.createElement("div");
    rcBox.className = "accounts-maint-rc";
    rcBtn.addEventListener("click", () => void this.renderRcSnippet(rcBtn, rcBox));
    ops.append(verifyBtn, syncBtn, rcBtn);
    box.appendChild(ops);
    box.appendChild(rcBox);
    wrap.appendChild(box);
    return wrap;
  }

  /**
   * Z05（销 BACKLOG F14）：rc 片段一键生成。
   *
   * **单一来源留在 bash**：片段由远端 `cc-acct-iso shellinit` 产出，本文件**不重新生成一份**
   * ——那会多一个跨语言双写点（本工作区反复在治的病）。抓到什么贴什么。
   *
   * **绝不代写**：只产出文本 + 复制按钮，写 `~/.bashrc` 是用户明令的红线（`paste-block.ts`
   * 的模块头也写死了「本文件没有、也不得有任何写入路径」）。
   */
  private async renderRcSnippet(btn: HTMLButtonElement, box: HTMLElement): Promise<void> {
    const host = this.currentHost();
    if (!host) {
      showActionFailureToast("拿不到这台远端的配置", "请先在「远端」里配好它", { level: "error" });
      return;
    }
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = "抓取中…";
    box.innerHTML = "";
    try {
      const snippet = await commands.remote_acct_iso_shellinit({ cfg: host });
      box.appendChild(
        buildPasteBlock({
          text: () => snippet,
          target: "这台远端的 ~/.bashrc（zsh 用户贴 ~/.zshrc；它是登录 shell 的配置文件）",
          mergeNote:
            "**追加**到文件末尾，并删掉你以前手写的 swap 式切号块——片段自带 BEGIN/END 围栏，"
            + "重新生成时替换围栏之间那一段即可，别贴成两份。",
          activation: "`source` 它，或在该远端开一个新的登录 shell（已经开着的 shell 不受影响）。",
          // 围栏在 Rust 侧已经校验过一次（拿不到就直接 Err）；这里再校验一次是因为
          // 「能显示」与「能贴」是两件事——半截片段贴进 rc 会让登录 shell 报错。
          invalidReason: (t) =>
            t.includes("# ===== BEGIN cc-acct-iso =====") &&
            t.includes("# ===== END cc-acct-iso =====")
              ? null
              : "片段不完整（缺 BEGIN/END 围栏），先别贴 —— 在远端跑一次 `cc-acct-iso verify` 看是什么状况。",
          multiline: true,
          rows: 12,
          className: "accounts-rc-paste",
        }).element,
      );
    } catch (e) {
      showActionFailureToast("生成 rc 片段失败", String(e), { level: "error" });
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  }

  private async accountRow(a: Account, isCurrent: boolean): Promise<HTMLElement> {
    const model = await getModelForAccount(a.name); // F07：每账号默认模型偏好
    const row = document.createElement("div");
    row.className = "accounts-row";
    if (isCurrent) row.classList.add("current");

    const mark = document.createElement("span");
    mark.className = "accounts-row-mark";
    mark.textContent = isCurrent ? "★" : "";
    mark.title = isCurrent ? "当前账号" : "";
    row.appendChild(mark);

    // account-ux U7：复用 U4 的账号头像——与状态栏 chip、tab 徽章同一套 hash 色，三处肉眼可对应。
    row.appendChild(accountAvatarEl(a.name, { size: 16 }));

    const name = document.createElement("span");
    name.className = "accounts-row-name";
    name.textContent = a.name;
    row.appendChild(name);

    const email = document.createElement("span");
    email.className = "accounts-row-email";
    email.textContent = a.email || "—";
    row.appendChild(email);

    const badge = document.createElement("span");
    badge.className = "accounts-row-badge";
    if (a.mode === "in-place") {
      badge.textContent = "逃生口";
      badge.classList.add("warn");
      badge.title = "in-place 模式：cc-monitor 不支持对它按会话切号";
    } else if (!a.loggedIn) {
      badge.textContent = "未登录";
      badge.classList.add("warn");
    } else {
      badge.textContent = "已登录";
    }
    row.appendChild(badge);

    // F10：plan 用量窗口%——懒加载（点击才探测,不是面板打开就对全部账号并发起隐藏会话,
    // 那是较重操作:起会话+网络查询）。与登录态放一起（都是"状态类"信息），机械操作
    // （复制路径/登录终端）留在 actions 里靠后。
    const usage = document.createElement("span");
    usage.className = "accounts-row-usage";
    this.renderUsageCell(usage, a);
    row.appendChild(usage);

    const dir = document.createElement("span");
    dir.className = "accounts-row-dir";
    // Z01：账号 0 没有 config dir——它**就是**「不设 CLAUDE_CONFIG_DIR」这个状态。
    // 显示它的真实含义，别显示空白，更别显示一个空串路径。
    dir.textContent = a.configDir ?? "（不设 CLAUDE_CONFIG_DIR）";
    dir.title =
      a.configDir ??
      "账号 0：起它就是什么都不设。凭据在共享库（~/.claude），.claude.json 在 $HOME。";
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
    modelInput.placeholder = "默认模型";
    modelInput.title =
      "该账号起会话时默认使用的模型（如 opus/sonnet），留空则跟随账号自身默认。" +
      "仅对本 app 发起的会话生效——终端里手敲 ccm 暂不识别这条偏好（见 unify-launch F08）。";
    modelInput.value = model ?? "";
    let lastSaved = model ?? "";
    const saveModel = async (): Promise<void> => {
      const next = modelInput.value.trim();
      if (next === lastSaved) return; // 值未变，别在每次失焦都弹一次噪音 toast
      try {
        await setModelForAccount(a.name, next || null);
        lastSaved = next;
        showActionFailureToast(
          next ? "已保存默认模型" : "已清除默认模型",
          next ? `${a.name} 起会话默认用 ${next}。` : `${a.name} 恢复跟随账号自身默认模型。`,
          { level: "info", durationMs: 3000 },
        );
      } catch (e) {
        // 校验失败（非法字符集）等——不落盘，保留用户已输入的文本以便就地修正。
        showActionFailureToast("保存模型偏好失败", String(e), { level: "error" });
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
      setDef.textContent = "设为当前账号";
      setDef.addEventListener("click", () => void this.selectDefault(a));
      actions.appendChild(setDef);
    }
    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = "复制路径";
    copy.addEventListener("click", () => {
      const text = a.configDir ?? "";
      if (!text) {
        showActionFailureToast("账号 0 没有 config dir", "起它就是不设 CLAUDE_CONFIG_DIR", {
          level: "info",
          durationMs: 3000,
        });
        return;
      }
      void navigator.clipboard?.writeText(text).then(
        () => showActionFailureToast("已复制", text, { level: "info", durationMs: 2500 }),
        () => showActionFailureToast("复制失败", "剪贴板不可用", { level: "error" }),
      );
    });
    actions.appendChild(copy);
    // A6：打开该账号的终端（去 /login / 修复登录）——对 in-place 逃生口不给（不支持切号）。
    if (a.mode !== "in-place") {
      const login = document.createElement("button");
      login.type = "button";
      login.textContent = a.loggedIn ? "登录终端" : "去登录";
      login.title = "用该账号打开一个远端终端（在里面 /login）";
      login.addEventListener("click", () => void this.launchStep({ kind: "login", name: a.name }));
      actions.appendChild(login);
    }
    row.appendChild(actions);
    return row;
  }

  /**
   * F10：用量单元格渲染——初始态是一个"查看用量"按钮（不自动探测）；点击后走五种状态之一
   * （查询中 / ok / unrecognized / not-logged-in / cli-missing / probe-failed），每种都是
   * 明确的短句，不是空白（DoD"诚实留白+说明为何"）。`force` 为真时忽略去抖缓存重新探测
   * （"刷新用量"用）。
   */
  private renderUsageCell(container: HTMLElement, a: Account, force = false): void {
    container.innerHTML = "";
    if (!force) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "accounts-usage-btn";
      btn.textContent = "查看用量";
      btn.title = "起一次隐藏会话跑 /usage 读取该账号的 plan 额度窗口（较重操作，几秒钟）";
      btn.addEventListener("click", () => this.renderUsageCell(container, a, true));
      container.appendChild(btn);
      return;
    }
    const pending = document.createElement("span");
    pending.className = "accounts-usage-pending";
    pending.textContent = "查询中…";
    container.appendChild(pending);
    if (!this.origin) return; // 理论不可达（accountRow 只在 origin 非空时被调），防御性早退
    // Z03：账号 0（`configDir === null`）也能探，载荷走 `unset CLAUDE_CONFIG_DIR; `。
    // **原样传 `null`，别 `?? ""`**——空串是坏数据，会被 fail-closed 拒掉。
    void fetchAccountUsage(this.origin, a.name, a.configDir, { force: true }).then((outcome) => {
      container.innerHTML = "";
      container.appendChild(this.buildUsageOutcomeEl(a, outcome));
    });
  }

  /** F10：把 `AccountUsageOutcome` 渲染成一个短句 span（+ 未识别态附一个"复制诊断文本"链接，
   *  方便用户报告；+ 一个"刷新"小按钮，复用 `renderUsageCell(force=true)`）。 */
  private buildUsageOutcomeEl(a: Account, outcome: AccountUsageOutcome): HTMLElement {
    const wrap = document.createElement("span");
    wrap.className = "accounts-usage-outcome";
    const text = document.createElement("span");
    switch (outcome.status) {
      case "ok":
        text.textContent = outcome.buckets
          .map((b) => `${b.label} ${b.usedPercent}%${b.resetIn ? ` · 重置${b.resetIn}` : ""}`)
          .join("；");
        // F10 Phase D 审计（后端架构+UX 均指出，重要）：解析成功≠格式已验证——这条 UI 分支是
        // "parse 成功但语义假设未验证"的隐蔽伪装成功（跟 unrecognized/not-logged-in 等诚实
        // 降级分支不是一回事）：真机验证前，"已用%"这个方向本身也是训练知识猜测，可能整体
        // 颠倒。hover 提示这一点，不新增视觉噪音（不影响默认可读性），真机验证完成后可摘掉。
        text.title = OK_USAGE_UNVERIFIED_CAVEAT;
        break;
      case "unrecognized":
        text.textContent = `暂时读不到（${outcome.reason}）`;
        text.title = outcome.raw ?? "";
        break;
      case "not-logged-in":
        text.textContent = "该账号未登录，无法读取用量";
        break;
      case "cli-missing":
        text.textContent = "该账号环境里没有 claude 命令";
        break;
      case "probe-failed":
        text.textContent = outcome.error;
        break;
    }
    wrap.appendChild(text);
    // F10 Phase D 审计（UX，重要）：此前只有 unrecognized 分支给"复制诊断文本"，但
    // not-logged-in/cli-missing 的判定同样基于训练知识猜测的正则（`NOT_LOGGED_IN_RE`/
    // `CLI_MISSING_RE`），误判风险不比 unrecognized 低——真机上完全可能出现"其实已登录，但
    // 屏幕上恰好有个欢迎语含 sign in 字样"这类误判，用户应该有办法把当时抓到的原始文本导出
    // 来自证/求助。放宽成"任意分支只要带 raw 就给"，不再局限于 unrecognized 这一支。
    if ("raw" in outcome && outcome.raw) {
      const copyBtn = document.createElement("button");
      copyBtn.type = "button";
      copyBtn.className = "accounts-usage-copy-raw";
      copyBtn.textContent = "复制诊断文本";
      copyBtn.title =
        "复制这次抓到的原始屏幕文字（可能含界面画框符号，不好看但对排查有用）——如果这个功能" +
        "读不出你的用量，可以把这段贴到项目的 GitHub issue 里帮忙定位。";
      copyBtn.addEventListener("click", () => {
        void navigator.clipboard?.writeText(outcome.raw ?? "").then(
          () =>
            showActionFailureToast(
              "已复制诊断文本",
              "这是探测抓到的原始屏幕内容（非隐私信息，只是终端画面文字）。如果这个功能一直读不出" +
                "用量，可以把它贴到 cc-monitor 的 GitHub issue 里，帮助定位是不是 Claude Code 改了" +
                " /usage 的显示格式。",
              { level: "info", durationMs: 4000 },
            ),
          () => showActionFailureToast("复制失败", "剪贴板不可用", { level: "error" }),
        );
      });
      wrap.appendChild(copyBtn);
    }
    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "accounts-usage-refresh";
    refreshBtn.textContent = "刷新";
    refreshBtn.addEventListener("click", () => {
      const container = wrap.parentElement;
      if (container) this.renderUsageCell(container, a, true);
    });
    wrap.appendChild(refreshBtn);
    return wrap;
  }

  private async selectDefault(a: Account): Promise<void> {
    try {
      await setDefaultName(a.name);
      // audit-fixes I5：`defaultName` 全局单值 → 切它清**所有 origin** 缓存（非当前 origin 否则 ≤30s 用旧账号）。
      invalidateAccountsCache();
      await this.reload(true);
      void emit(SETTINGS_APPLIED_EVENT); // 让主窗状态栏 chip 同步
      showActionFailureToast(
        "已设为当前账号",
        `以后新会话 / resume 默认用 ${a.name}；正在跑的会话不受影响。`,
        { level: "info", durationMs: 4000 },
      );
    } catch (e) {
      showActionFailureToast("设为当前账号失败", String(e), { level: "error" });
    }
  }

  private info(text: string): void {
    const p = document.createElement("div");
    p.className = "accounts-info";
    p.textContent = text;
    this.body.appendChild(p);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);
}

/** A6 向导用的按钮工厂（type=button，避免 form 默认提交）。 */
function mkBtn(text: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = text;
  return b;
}
