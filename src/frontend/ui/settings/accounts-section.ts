// A3：设置面板「账号」组（多账号 cc-acct-iso）。占用原「远端」空占位组。
//
// 展示某台远端的账号列表（名/邮箱/mode/登录态/configDir/默认）+ 设为默认 / 复制 configDir /
// 刷新。**只读 + 改本机默认账号**（写 config.json，不碰远端 manifest、不注入、不重启——A4/A5）。
// 部署引导（未启用时）留 A6 填；本组先给出"如何启用"的说明与 manifest 路径。
//
// 设置窗独立于主窗、拿不到活跃会话，故用远端选择器（多台时下拉）。改默认账号后
// emit(SETTINGS_APPLIED_EVENT) 让主窗状态栏 chip 同步。
import { installAcctIso, readAcctIsoSnippet, readAcctIsoStatus, type AcctIsoInstalled } from "../acct-iso-reads";
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
import { buildPasteBlock } from "../paste-block"; // T03：待贴文本统一组件（Z05 复用它）
// 〔第三波 S3〕本机那一支新长的字全走文案表（`设计/91 §5.1`）：一处取文，判据按表逐条量。
import { copyText } from "../copy-table";
// 〔第三波 S3〕本机建号那一跳：后端那个本机串（〔C4b〕账号面那个 `"__local__"` 已退役，本机只剩这一个表示），
// 以及「本机刻意不开终端窗口」那句话的跨语言标记（唯一住址在 `remote-launch-run.ts`）。
import { LOCAL_ORIGIN as BACKEND_LOCAL_ORIGIN } from "../backend-policy";
import { isLocalOrigin, isRemoteOrigin, type Origin } from "../ipc/origin";
import { POSIX_NO_WINDOW_MARKER } from "../remote-launch-run";
// 〔AL1 · 2026-09-24〕别名那一块与用户级 PATH 那一格都搬去了机器页「本机 → 工具 → 别名」
// （`设计/70 §3.3` · `设计/71`）—— 两者是同一个问题（「这台机器的终端怎么找到 ccm」）的两条路。
// A2（`设计/70 §4.4`）：新建账号那张表单。
import { renderNewAccountForm, type NewAccountRequest } from "./account-new-form";
import { SETTINGS_APPLIED_EVENT } from "./events";
// Phase G：这两格此前**没有任何生产者**，见下面 `note()` 的注释。
// `N-F2`：本机那条路也要写进同一本账 ⇒ 连本机那个 key 一起取，别在这儿长第二个名字。
import { recordFacet, LOCAL_MACHINE_KEY } from "./machine-status";
import {
  askAcctIsoCmd,
  validateAcctName,
  type AcctIsoStep,
} from "./acct-deploy";
import { askConfirm } from "../ask-dialog";
import { saidOfControl } from "../control-said";

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
 * ⚠ 〔`K-R20` 订正 09-03〕上面两处原先都点着
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
 * 🔴 `设计/70 §4.3` ③ / `§4.4` 关键二：**这一块里不再有账号下拉。**
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
 * `设计/70 §4.4` 关键二：**某一个账号**的第三方 API key —— 账号那一行展开出来的一格。
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
  /**
   * A2：表单上选了「第三方 apikey」、终端已拉起、但这个号**还没出现在列表里**的那几把 key（名 → key）。
   * 只住内存，不落盘、不进 DOM；放弃 / 写成 / 写不成都会把它删掉。
   *
   * 〔RM1a〕每一把**记着它是给哪台机器建的**：key 从此写进那台机器上的那一份表，
   * 而这一页会换机器 —— 不记的话，远端 A 上建的号若与本机某个号同名，key 会被写到本机去。
   * ⇒ 只在**同一台**机器的列表里出现时才写。
   */
  // 〔第四波 ST2〕值多带一格 Base URL（表单 apikey 那一支的第二格；缺席 = 默认上游）。
  // 〔RM1a〕再带一格 origin：建它的那台机器（见上）。两格一起跟着那把 key 走到那台机器上。
  private readonly pendingKeys = new Map<string, { key: string; origin: Origin; baseUrl?: string }>();
  private pendingBox: HTMLElement | null = null;

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
      // 〔RESYNC · 主会话 09-27 裁〕本机远端都清（与账号 chip 同一条：缓存的键就是 origin，本机也一样）。
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
     * 下拉早删了（E59），本机那一支也早有了（`N-F1b`）⇒ 〔第三波 S3〕`null` 就切到本机那一支
     * （见 [`followMachine`]）。
     */
    subscribeMachine((origin) => this.followMachine(origin));
    // ST1「延后加载」：构造期不再 `void this.init()` —— 见 `loadNow()`。
  }

  /**
   * ST1「延后加载」（`设计/70 §5.3` 判据 2：**子页内容只在该子页可见时才发 I/O**）：
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
    // 🔴 〔第三波 S3 · 09-24〕**初值只认共用 store**：`null` 就是本机（`machine-context.ts` 头注）。
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
   * 〔第三波 S3〕`null`（本机）原先在这里**原地不动**（「本分节表示不了」）—— 那句话自 `N-F1b`
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
   * Phase G：给状态账本记一格。
   *
   * **这里此前是个洞**：`MACHINE_FACETS` 有 5 格，而全仓 `recordFacet` 的生产者只覆盖
   * 3 格（machine-card 的 connection/backend/ccm）—— `acctIso` 与 `accounts` **一个写点都没有**。
   * 后果不是「少两个格子」，而是**「还差什么」那张清单在任何真实安装上都清不空**：
   * 每台机器恒定产出 ≥2 条 `unknown` ⇒ `summarizeGaps` 恒非 null ⇒
   * `remote-section` 里「全绿就整块不出现」那一支是**死代码**。
   * 一张自称「还差什么、点哪里补齐」却既补不齐也消不掉的清单，比不做这个功能更糟。
   *
   * # `N-F2`（09-05）：那个洞**在本机这条路上一直还开着**，本件把它补上
   *
   * 上面那段说的是「全仓」，而这一行此前逐字写着 `if (!this.origin) return;`
   * ⇒ 远端补上了，**本机一格都没写过**（`N-F2` 开工时现打：全仓对 `LOCAL_MACHINE_KEY`
   * 的 `recordFacet` 写点 **0 个** —— ⚠ **这是那一刻的快照，而改掉它的正是下面这一行**：
   * 本件之后是 1 个，就是这里）。而 `readiness.notApplicable` 对本机只排掉
   * `backend` / `connection` 两格（`ccm` 另有一条，仅 Windows），
   * 于是本机的 `acctIso` / `accounts` 是**适用而恒 `unknown`** 的两格 ——
   * 上面那句「清单在任何真实安装上都清不空」在本机这一侧原封不动地仍然成立。
   *
   * ⚠ **那道守卫看不见这件事**：`facet-producer-guard.vitest.ts` 扫的是
   * 「源码里有没有 `note("acctIso", …)` 这个形状」，而它**一直是绿的** ——
   * 因为写点确实存在，只是被这一行早返回挡在本机之外。
   * 「代码库里有没有写点」与「某条路上写不写得到」是两个作用域。
   *
   * # 那句被撤掉的注释：「本机：这两格由 `L3b` 补，今天表示不了」
   *
   * `NF2D1` 现打核实，这句话**两头都不成立**（查证过程见件文件 `§3a`）：
   * - `L3b`（`planned-build/local-as-remote/MASTERPLAN.md:117`）是**本地账号管理·写**
   *   ——建 / 迁 / 删 / 改默认号，状态列逐字「待规划」、`STATUS.md:3` 逐字「L3b 未做」，
   *   代码仓里零实现。它是**写**那一摊，而这两格问的是**读得出来没有**
   *   ⇒ 就算 L3b 落地了，也**不是它**来补这两格：挂错了件。
   * - 「今天表示不了」也过期了：`N-F1b` / `N-F1c` 之后本机这条路
   *   （`reloadLocal` ⇒ `fetchLocalAccounts`）本来就有三档结局，那正是这两格要写的东西。
   *
   * # 本机用哪个 key
   *
   * `LOCAL_MACHINE_KEY` —— 与 `remote-section` 算那张清单时传的入参
   * （`origins: [LOCAL_MACHINE_KEY, ...]`）、以及本机那一行显状态格用的
   * `readStatus(LOCAL_MACHINE_KEY)` 是**同一个常量**，不在这里长第二个名字。
   * 远端那条路的 key 与写进去的值**一个字节不变**（`this.origin` 非空时走的还是它）。
   */
  private note(
    facet: "acctIso" | "accounts",
    state: { kind: "ok" | "fail" | "na"; detail?: string },
  ): void {
    recordFacet(isLocalOrigin(this.origin) ? LOCAL_MACHINE_KEY : this.origin, facet, state);
  }

  /** 〔VIS2 · `设计/15 §4.5` 缺口二〕空态里 `accounts` 那一格：没启用 ⇒ 缺；启用着（零个号）⇒ 读到了。 */
  private enabledFacet(enabled: boolean): { kind: "ok" | "fail"; detail: string } {
    return enabled
      ? { kind: "ok", detail: copyText("accounts.status.read") }
      : { kind: "fail", detail: copyText("accounts.status.multiOff") };
  }

  /** 〔VIS2 · `设计/15 §4.5` 缺口二〕`acctIso` 那一格 = cc-acct-iso 装没装（`null` = 问不出来）。 */
  private noteInstalled(installed: boolean | null): void {
    const detail =
      installed === true
        ? copyText("accounts.status.isoInstalled")
        : installed === false
          ? copyText("accounts.status.isoMissing")
          : copyText("accounts.status.isoUnknown");
    this.note("acctIso", { kind: installed === true ? "ok" : "fail", detail });
  }

  private async reload(force: boolean): Promise<void> {
    this.body.innerHTML = "";
    // 🔴 ST1「切机器 pending」（`设计/70 §6` #5）：一次切机器 = 这一块重读一趟（远端是一次 SSH 往返）。
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
      // 🔴 `K-R59`：这里原来还有一支 `case "hidden"`，把 `accounts`/`acctIso` 两格
      //    记成 `na`、理由「用户显式选的降级」。那一档（`daemonless`）整格没了 ⇒ 支也没了。
      case "needs-update":
        this.note("accounts", { kind: "fail", detail: copyText("accounts.status.backendOld") });
        this.info(copyText("accounts.status.backendOldBody", { reason: ui.reason }));
        return;
      // 〔WF2 · WIN3 读数 C〕没问出来 ≠ 要更新：照实说查询失败与原因。
      case "query-failed":
        this.note("accounts", { kind: "fail", detail: copyText("accounts.status.queryFailed") });
        this.info(copyText("accounts.status.queryFailedBody", { reason: ui.reason }));
        return;
      case "not-enabled":
        // 〔VIS2 · `设计/15 §4.5` 缺口二〕启用没启用记在 accounts（启用着只是零个号 ⇒ 读到了）；acctIso 只记装没装（`renderNotEnabledFlow` 里问）。
        this.note("accounts", this.enabledFacet(state.meta?.enabled === true));
        void this.renderNotEnabledFlow(ui.manifestPath, ui.reason);
        return;
      case "ready":
        this.note("accounts", { kind: "ok", detail: copyText("accounts.status.count", { n: ui.accounts.length }) });
        // 启用着 ⇒ 工具在（多账号只由它建）；不为这一格多开一趟 SSH。
        this.note("acctIso", { kind: "ok", detail: copyText("accounts.status.isoInstalled") });
        await this.renderTable(state, ui.accounts, ui.notice);
        return;
    }
  }

  /**
   * `N-F1b`：**没有配任何远端时，这一节讲的是这台机器。**
   *
   * # 它为什么是一条**自己的**渲染路，而不是把本机塞进 `renderTable`
   *
   * 远端那张表带着一整套只对远端成立的东西：维护区那几个按钮会**动远端目录**、
   * 「去登录」会去拉一个**远端终端**（`accountLoginActionLabel` 的两句文案都逐字带「远端」）、
   * apikey 那一块问的是「这几个 configDir 在**本机**的 apikey 表里有没有行」而表里显的是远端的号
   *（那一块当年的头注口径③ 自己记着这笔账；它今天拆成 `renderApikeyFileBlock` ＋ `renderApikeyEditor`）。把本机接进那条路，
   * 等于把「一台远端机器」这个概念套到本机头上 —— 那正是 `N2` 排除的那件事，
   * 也正是 `control-parity` 那个区花 41 件治过的病。
   * ⇒ 本机这一支只做它今天真做得到的事：**把清单列出来**。
   *
   * # 🔴 射程线：本件只换「面板走不走得到」，不换数据源
   *
   * 数据源是今天已经能用的本机读口 `fetchLocalAccounts`（直读磁盘）。
   * 用户 09-05 拍的板（`DECISIONS.md` `NR1` / `NR2`）说的终态是「每台机的账号由那台机的
   * **后端**管」，那是**下一件**（`N-F1c`），它压着两处现打的障碍：后端那份路径检查
   * 第一条是 `starts_with('/')`（Windows 列表会恒空）· 开发树里没有后端程序。
   * ⇒ 这一支将来是**换实现不换界面**，别在这里预支它。
   *
   * # 三个结局，一个都不许合并
   *
   * 读不出来 / 一个号都没有 / 有号 —— 前两个长得像但完全不是一回事：
   * 把「读不出来」渲染成「你没有账号」，用户会去装一个他已经装好的东西。
   * 由 `NF1bD1` 那一族的两条「诚实降级」判据钉着。
   *
   * # `N-F2`：这三个结局**每一个都要往账本里记一笔**
   *
   * 这一支此前渲染完就走，一格都不写 ⇒ 本机的 `acctIso` / `accounts` 恒 `unknown`
   *（详见 `note()` 的头注）。本件给每一档配一个**互不相同**的写法，
   * 因为「没测过 / 读不动 / 后端不在 / 读到了但没启用 / 都好」在
   *「还差什么」那张清单上要说的是五句不同的话。
   *
   * ⚠ **读失败那两档为什么 `acctIso` 也记 `fail`，而不是留空**：账本的词汇只有
   * `ok` / `fail` / `na` 三个，留空的含义是「**没测过**」。而这两档是**测过了**
   *（我们真去问了本机后端），结论是这台机器此刻**按账号隔离地起会话这件事做不到**
   * —— 那正是 `FACET_MEANING.acctIso.consequence` 逐字写的后果。
   * 记 `na`（不适用）与记空（没测过）在这里都是假话；`detail` 里带上是哪一档，
   * 用户才知道该去修后端还是去装工具。⇒ 这不是选出来的，是词汇表逼出来的。
   *
   * ⚠ **诚实边界**：前端分不出后端那三档里的 `NoBackend` 与 `Unreadable`
   *（`local_accounts.rs` 把两者一起塞进 `available:false` + 一句 `error` 文案）。
   * 要在格子上分开它们，得让后端多带一个字段回来 —— 那是 `src/frontend/shell` 那一侧的事，
   * 不在本件射程里。所以这里的档名说的是**面板看得见的那三档**，不是 Rust 那个枚举。
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
      this.note("acctIso", { kind: "fail", detail: copyText("accounts.local.unreadable") });
      this.localFail(box, String(e));
      return;
    }
    if (!state.available) {
      // 档二：**后端不在** —— 后端答了「不可用」，那句原因在 `state.error` 里。
      this.note("accounts", { kind: "fail", detail: copyText("accounts.local.noBackend") });
      this.note("acctIso", { kind: "fail", detail: copyText("accounts.local.noBackend") });
      this.localFail(box, state.error ?? LOCAL_ACCOUNTS_COPY.unknownReason);
      return;
    }
    if (!state.meta?.enabled || state.accounts.length === 0) {
      // 档一的空态：没启用 ⇒ accounts 缺（〔VIS2〕启用没启用住这一格）；启用着只是零个号 ⇒ 读到了。
      // acctIso 记装没装，由下面那一问写（与远端 `not-enabled` 一支同形）。
      this.note("accounts", this.enabledFacet(state.meta?.enabled === true));
      AccountsSection.line(
        box,
        "accounts-info accounts-local-empty-title",
        LOCAL_ACCOUNTS_COPY.emptyTitle,
      );
      await this.renderLocalAcctIsoProbe(box);
      return;
    }

    // 档一：**读出来了**，而且这台机真的启用着隔离账号 ⇒ 两格都绿。
    // 这是本机那两格唯一能变绿的一档 —— `NF2D3` 那条判据买的就是它。
    this.note("accounts", { kind: "ok", detail: copyText("accounts.status.count", { n: state.accounts.length }) });
    this.note("acctIso", { kind: "ok", detail: copyText("accounts.status.isoInstalled") });
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
    // 〔第三波 S3〕本机这一半的两格事实（apikey 表里有没有它那一行 · 本机中转在不在跑）问后端要：
    // `accountStatusBadge` 本机那三档从 `K-H2b` 起就「有实现、没接线」，这里接上。
    const routing = await this.readLocalRouting(state.accounts);
    for (const a of state.accounts) {
      table.appendChild(
        AccountsSection.localRow(a, cur?.name === a.name, routing ? localApikeyEndpointStateFor(a, routing) : undefined),
      );
    }
    box.appendChild(table);
    // 〔第三波 S3〕原先这里是 `LOCAL_ACCOUNTS_COPY.scopeHint`（「只读；在这里改不了它们」）——
    // 这一拍本机能新建账号了，那句话成了假话 ⇒ 换成文案表里本机那一句。
    AccountsSection.line(box, "accounts-hint accounts-local-hint", copyText("accountsLocal.list.scope"));
    box.appendChild(this.renderLocalRcSnippetBlock());
    // 〔第三波 S3〕本机也能新建账号：与远端同一张表单（同一套校验、同一条 `cc-acct-iso add` 命令），
    // 只是跑命令的那一跳走本机（[`launchLocalStep`]）。apikey 那一支在本机**真的有用**：
    // apikey 表与中转本来就是本机的，起本机会话时按账号换上那把 key。
    box.appendChild(
      renderNewAccountForm(BACKEND_LOCAL_ORIGIN, (req) => this.createAccount(req, "local"), {
        subscription: copyText("accountsLocal.new.subscriptionHint"),
        apikey: copyText("accountsLocal.new.apikeyHint"),
      }),
    );
    this.renderPendingKeys();
    await this.flushPendingKeys(state.accounts);
    // 〔AL1 · 2026-09-24〕这里原来挂着「按账号生成命令」那一块（`K-R49`）。它搬去了机器页
    // 「本机 → 工具 → 别名」，并且不再是「账号表的投影」—— 别名清单归用户（`设计/71 §8`）。
    // 〔AL1 · 2026-09-24〕`K-R135` 那一格（用户级 PATH）也跟着别名块搬去了机器页「别名」里
    // （Windows 本机上，第一次展开那一块时建）。
  }

  /**
   * 〔第三波 S3 · A3 接线〕本机空态的「下一步」：先问本机后端**这台机器装没装 cc-acct-iso**
   * （〔MIG-3a〕经通道问本机后端 `acct-iso-status`），再说下一步 —— 三个结局各说各的：
   *
   * | 问到的 | 这一格说什么 | 「下一步」那一行 |
   * |---|---|---|
   * | 装了 | 装在哪（路径是后端答的） | 在终端里跑 `init`（本机没有替你开终端的口，如实说「在终端里」） |
   * | 没装 | 还没装 | 原样用 `LOCAL_ACCOUNTS_COPY.emptyNext`（装 + 初始化，本机没有安装口） |
   * | 问不出来 | 查不出来 ＋ 原因 | 同上 —— 问不出来**不许**当成「装了」，也不许当成「没装」 |
   *
   * 〔VIS2 · `设计/15 §4.5` 缺口二〕问到的就是 `acctIso` 那一格（装没装）：装了 ok · 没装 fail · 问不出来 fail「查不出来」。
   */
  private async renderLocalAcctIsoProbe(box: HTMLElement): Promise<void> {
    const iso = AccountsSection.line(box, "accounts-hint accounts-local-iso", "");
    let installed: boolean | null = null;
    try {
      const st = await readAcctIsoStatus(BACKEND_LOCAL_ORIGIN);
      if (typeof st?.installed !== "boolean") throw new Error(String(st));
      installed = st.installed;
      this.noteInstalled(st.installed);
      iso.textContent = st.installed
        ? copyText("accountsLocal.acctIso.installed", { path: st.path ?? "cc-acct-iso" })
        : copyText("accountsLocal.acctIso.missing");
    } catch (e) {
      this.noteInstalled(null);
      iso.textContent = copyText("accountsLocal.acctIso.probeFailed", { reason: String(e) });
    }
    AccountsSection.line(
      box,
      "accounts-hint accounts-local-empty-next",
      installed === true ? copyText("accountsLocal.acctIso.initNext") : LOCAL_ACCOUNTS_COPY.emptyNext,
    );
  }

  /**
   * 〔第三波 S3 · A3 接线〕本机的 rc 片段：〔MIG-3a〕经通道问本机后端 `acct-iso-shellinit` → 待贴块。
   *
   * 与远端那颗「生成 rc 片段…」（[`renderRcSnippet`]）同一个形状、同一条纪律：
   * **只读、不代写**（`paste-block.ts` 模块头：本组件没有任何写入路径）。
   * 围栏已由那台后端校验过一次（〔MIG-3a〕`acct-iso-shellinit` 自己校验）；这里再校验一次，理由同远端那条：「能显示」与「能贴」是两件事。
   *
   * ⚠ 文案全走 `copyText`（`accountsLocal.rc.*`）：本机那一支上不许出现「远端」，
   * 远端那段话（「这台远端的 ~/.bashrc」「在远端跑一次」）不能照抄过来。
   */
  private renderLocalRcSnippetBlock(): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "accounts-local-rc";
    const btn = mkBtn(copyText("accountsLocal.rc.action"));
    btn.title = copyText("accountsLocal.rc.hover");
    const out = document.createElement("div");
    out.className = "accounts-maint-rc";
    btn.addEventListener("click", () => {
      void (async () => {
        btn.disabled = true;
        out.innerHTML = "";
        try {
          const snippet = await readAcctIsoSnippet(BACKEND_LOCAL_ORIGIN);
          out.appendChild(
            buildPasteBlock({
              text: () => snippet,
              target: copyText("accountsLocal.rc.target"),
              mergeNote: copyText("accountsLocal.rc.merge"),
              activation: copyText("accountsLocal.rc.activation"),
              invalidReason: (t) =>
                t.includes("# ===== BEGIN cc-acct-iso =====") &&
                t.includes("# ===== END cc-acct-iso =====")
                  ? null
                  : copyText("accountsLocal.rc.incomplete"),
              multiline: true,
              rows: 12,
              className: "accounts-rc-paste",
            }).element,
          );
        } catch (e) {
          showActionFailureToast(copyText("accountsLocal.rc.failed"), String(e), { level: "error" });
        } finally {
          btn.disabled = false;
        }
      })();
    });
    wrap.append(btn, out);
    return wrap;
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
   * 〔第三波 S3〕本机那两格事实：问本机后端（〔US1〕经通道 `apikey-routing`）。
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
   * 〔第三波 S3〕徽章的 `endpoint` 从这一拍起**传本机那一半**（`{ scope: "local", … }`，
   * 由 `localApikeyEndpointStateFor` 从后端答的两格事实摊出来）。原先这里不传，理由是「那要多一条 IPC，属下一件」
   * —— 那一问早在盘上了，只是这一支没去问（〔US1〕今天经通道问 `apikey-routing`）。
   * 问不到 / 账号 0（没有 configDir）⇒ 仍然不传，徽章照旧「不替它下判断」。
   * 🔴 **千万别顺手传 `{ scope: "remote" }`** —— 那会让一台本机的号被解释成远端那一半，
   * 文案里当场出现「远端」两个字；`NF1bD2` 那条判据正是钉这个的。
   */
  private static localRow(a: Account, isCurrent: boolean, relay?: ApikeyEndpointState): HTMLElement {
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
    return row;
  }

  /**
   * A6：在远端终端里跑一个部署/维护步骤——问那台后端要命令（〔DUP2 · J4〕`acct-iso-cmd`；它拒了 / 问不到 ⇒ 提示、不动手）
   * → danger 步二次确认 → `terminal-open.ts::openTerminal` 弹真实终端让用户看着跑（DESIGN §6，不代跑）。
   */
  private async launchStep(
    step: AcctIsoStep,
    opts: { danger?: boolean; confirmExtra?: string } = {},
  ): Promise<boolean> {
    if (isLocalOrigin(this.origin)) return false;
    let cmd: string;
    try {
      cmd = await askAcctIsoCmd(this.origin, step);
    } catch (e) {
      showActionFailureToast(copyText("accounts.launchStep.cmdInvalid"), saidOfControl(e), { level: "error" });
      return false;
    }
    if (opts.danger) {
      const msg =
        copyText("accounts.launchStep.confirm", { machine: this.origin, cmd, extra: (opts.confirmExtra ? `${opts.confirmExtra}

` : "") });
      if (!(await askConfirm(msg))) return false;
    }
    try {
      await openTerminal(this.origin, cmd);
      showActionFailureToast(copyText("accounts.launchStep.launched"), copyText("accounts.launchStep.launchedNext"), {
        level: "info",
        durationMs: 5000,
      });
      return true;
    } catch (e) {
      showActionFailureToast(copyText("accounts.launchStep.failed"), String(e), { level: "error" });
      return false;
    }
  }

  /**
   * 〔第三波 S3〕在**本机**跑一个账号步骤（今天只有新建账号用它）。
   *
   * 走的是既有那条 `terminal-open.ts::openTerminal`，`origin` 给本机串 —— 本机那一支原串交 monitor 开窗
   * （`launch.rs::open_terminal_window`：Windows 开一个 PowerShell 窗口；别的系统**刻意不开窗口**，
   * 回一句带 `POSIX_NO_WINDOW_MARKER` 的话，让前端把命令交给用户在自己的 bash 里跑）。
   * ⇒ 本机建号**不需要新命令**。
   *
   * 三个结局，返回值说「这条命令会不会被跑」（apikey 那一支据此决定留不留那把 key）：
   * - 开了窗口 ⇒ `true`；
   * - 刻意不开窗口（按后端自己的声明判，不按 OS 猜）⇒ 命令复制好，`true` —— 这一支就是 Linux 上的正路；
   * - 真失败 ⇒ 命令照样复制给用户，但返回 `false`：与远端那条「终端没拉起来就不留 key」同一个口径。
   */
  private async launchLocalStep(step: AcctIsoStep): Promise<boolean> {
    // 〔DUP2 · J4〕命令由本机后端出（与远端同一条 `acct-iso-cmd`，`origin` = 本机）。
    let cmd: string;
    try {
      cmd = await askAcctIsoCmd(BACKEND_LOCAL_ORIGIN, step);
    } catch (e) {
      showActionFailureToast(copyText("accountsLocal.new.cmdInvalid"), saidOfControl(e), { level: "error" });
      return false;
    }
    try {
      await openTerminal(BACKEND_LOCAL_ORIGIN, cmd);
      showActionFailureToast(
        copyText("accountsLocal.new.launched"),
        copyText("accountsLocal.new.launchedNext"),
        { level: "info", durationMs: 5000 },
      );
      return true;
    } catch (err) {
      let copied = true;
      try {
        await navigator.clipboard.writeText(cmd);
      } catch {
        copied = false; // 命令在提示里照样看得见，可以手动复制
      }
      const byDesign = String(err).includes(POSIX_NO_WINDOW_MARKER);
      const headline = byDesign
        ? copied
          ? copyText("accountsLocal.new.noWindowCopied")
          : copyText("accountsLocal.new.noWindowNotCopied")
        : copied
          ? copyText("accountsLocal.new.failedCopied")
          : copyText("accountsLocal.new.failedNotCopied");
      showActionFailureToast(
        headline,
        copyText("accountsLocal.new.pasteBody", { reason: String(err), cmd: cmd }),
        { level: byDesign ? "info" : "error", durationMs: 10000 },
      );
      return byDesign;
    }
  }

  /** 当前选中远端对应的 host 配置（多账号 IPC 要传 cfg=RemoteHostConfig）。 */
  private currentHost(): RemoteHostConfig | null {
    if (isLocalOrigin(this.origin)) return null;
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
        const status = await readAcctIsoStatus(this.origin);
        this.noteInstalled(status.installed);
        if (!status.installed) {
          this.renderNeedsDeploy();
          return;
        }
      } catch (e) {
        this.noteInstalled(null);
        console.warn("acct-iso-status failed, fall through to wizard:", e);
      }
    }
    this.renderNotEnabled(manifestPath, reason);
  }

  /**
   * F5：远端没装 cc-acct-iso → 一键装（那台后端自己带着那份字节、落点它自己算、链接 ＋ 配置 ＋ 记账，不碰 rc）。
   * 〔MIG-3a · 主会话 09-28 预裁〕从前是 monitor 推字节（`deploy_remote_acct_iso`〔散文墓碑〕，落点由这里按用户名推）再问那台落进用户目录；
   * 今天只问那台一次 `acct-iso-install`。
   */
  private renderNeedsDeploy(): void {
    const box = document.createElement("div");
    box.className = "accounts-needs-deploy";

    const h = document.createElement("div");
    h.className = "accounts-ne-title";
    h.textContent = copyText("accounts.needsDeploy.title");
    box.appendChild(h);

    const p = document.createElement("div");
    p.className = "accounts-ne-desc";
    p.textContent = copyText("accounts.needsDeploy.intro");
    box.appendChild(p);

    const btn = mkBtn(copyText("accounts.needsDeploy.deploy"));
    btn.addEventListener("click", () => {
      btn.disabled = true;
      const prev = btn.textContent;
      btn.textContent = copyText("accounts.needsDeploy.deploying");
      // 〔MIG-3a · 09-28 预裁〕一步：问那台后端 `acct-iso-install`（字节它自己带着；幂等：一致的不写、已在的不动）。
      void installAcctIso(this.origin)
        .then(installedLine)
        .then(
          (msg) => {
            showActionFailureToast(copyText("accounts.needsDeploy.done"), msg, {
              level: "info",
              durationMs: 6000,
            });
            void this.reload(true);
          },
          (e) => {
            showActionFailureToast(copyText("accounts.needsDeploy.failed"), String(e), { level: "error" });
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
    h.textContent = copyText("accounts.notEnabled.title");
    box.appendChild(h);

    const p = document.createElement("div");
    p.className = "accounts-ne-body";
    p.innerHTML =
      copyText("accounts.notEnabled.intro", { reason: escapeHtml(reason), path: escapeHtml(manifestPath ?? copyText("accounts.notEnabled.noPath")) });
    box.appendChild(p);

    const wiz = document.createElement("div");
    wiz.className = "accounts-wizard";

    // 默认账号名输入 + 实时校验。
    const field = document.createElement("div");
    field.className = "accounts-wiz-field";
    const label = document.createElement("label");
    label.textContent = copyText("accounts.notEnabled.defaultName");
    const input = document.createElement("input");
    input.type = "text";
    input.className = "accounts-wiz-name";
    input.placeholder = copyText("accounts.notEnabled.defaultNameHint");
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
    const copyBtn = mkBtn(copyText("accounts.notEnabled.copy"));
    copyBtn.addEventListener("click", () => {
      void navigator.clipboard?.writeText(preview.textContent ?? "").then(
        () => showActionFailureToast(copyText("accounts.notEnabled.copied"), copyText("accounts.notEnabled.copiedNext"), { level: "info", durationMs: 2500 }),
        () => showActionFailureToast(copyText("accounts.copy.failed"), copyText("accounts.copy.noClipboard"), { level: "error" }),
      );
    });
    copyRow.appendChild(copyBtn);
    wiz.appendChild(copyRow);

    // 分步按钮。
    const btns = document.createElement("div");
    btns.className = "accounts-wiz-btns";
    const bPreview = mkBtn(copyText("accounts.notEnabled.step1"));
    const bApply = mkBtn(copyText("accounts.notEnabled.step2"));
    bApply.classList.add("danger");
    const bVerify = mkBtn(copyText("accounts.notEnabled.step3"));
    const bShellinit = mkBtn(copyText("accounts.notEnabled.step4"));
    btns.append(bPreview, bApply, bVerify, bShellinit);
    wiz.appendChild(btns);

    const note = document.createElement("div");
    note.className = "accounts-wiz-note";
    note.innerHTML =
      copyText("accounts.notEnabled.steps");
    wiz.appendChild(note);

    // —— 校验驱动的启用/禁用 + 预览 ——
    // 〔DUP2 · J4〕两行命令由那台后端出（`acct-iso-cmd`，两问）：输入一变就问，只认最后一次的答案（序号，零定时器）。
    let asked = 0;
    const sync = (): void => {
      const name = input.value.trim();
      const v = validateAcctName(name);
      const valid = v.ok;
      err.textContent = name && !v.ok ? v.reason : "";
      for (const b of [bPreview, bApply, bShellinit]) b.disabled = !valid;
      // verify 不依赖名字（自检当前状态），恒可点。
      const my = ++asked;
      if (!valid) {
        preview.textContent = copyText("accounts.sync.empty");
        return;
      }
      void Promise.all([
        askAcctIsoCmd(this.origin, { kind: "init-preview", name }),
        askAcctIsoCmd(this.origin, { kind: "init-apply", name }),
      ]).then(
        ([cmd, cmd2]) => {
          if (my === asked) preview.textContent = copyText("accounts.sync.script", { cmd, cmd2 });
        },
        (e: unknown) => {
          if (my !== asked) return;
          preview.textContent = copyText("accounts.sync.empty");
          err.textContent = saidOfControl(e);
        },
      );
    };
    input.addEventListener("input", sync);
    bPreview.addEventListener("click", () =>
      void this.launchStep({ kind: "init-preview", name: input.value.trim() }),
    );
    bApply.addEventListener("click", () =>
      void this.launchStep(
        { kind: "init-apply", name: input.value.trim() },
        { danger: true, confirmExtra: copyText("accounts.notEnabled.moveNote") },
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
    // Z01：**能用但有缺**（远端 backend / cc-acct-iso 旧到看不见账号 0）。列表本身是好的，
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
    // 🔴 `设计/70 §4.4` 关键二：apikey 是**账号那一行自己的一格** ⇒ 画表之前先问清
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

    // 🔴 `设计/70 §4.4`（A2）：**新建账号是一张常驻的表单**，不再藏在「维护」折叠组里、
    //    也不再是红色按钮。岔口（订阅 / 第三方 apikey）在表单里问。
    this.body.appendChild(renderNewAccountForm(this.origin, (req) => this.createAccount(req)));
    this.renderPendingKeys();
    // 表单交过 apikey、而这个号这一趟已经出现在列表里了 ⇒ 接着把 key 写进去。
    await this.flushPendingKeys(accounts);

    this.body.appendChild(this.renderMaintenance());
  }

  /**
   * A2：表单交上来一个新账号。两支共用同一条终端命令（`cc-acct-iso add <名> --apply`）；
   * apikey 那一支另把 key 记在 [`pendingKeys`] 里，等这个号在列表里出现再写。
   *
   * ⚠ **终端没拉起来就不留 key** —— 那个号不会出现，留着就是一把在内存里永远等不到主人的明文。
   */
  private async createAccount(
    req: NewAccountRequest,
    where: "remote" | "local" = "remote",
  ): Promise<void> {
    const step: AcctIsoStep = {
      kind: "add-apply",
      name: req.name,
      credFile: req.access === "subscription" ? req.credFile : undefined,
    };
    const launched =
      where === "local" ? await this.launchLocalStep(step) : await this.launchStep(step);
    if (launched && req.access === "apikey") {
      const origin = where === "local" ? BACKEND_LOCAL_ORIGIN : this.machineOrigin();
      this.pendingKeys.set(req.name, { key: req.key, origin, baseUrl: req.baseUrl });
    }
    this.renderPendingKeys();
  }

  /**
   * A2：「建好后自动写 key」还在等的那几个号 —— 常驻一行，并且可以放弃（放弃 = 把内存里那把 key 丢掉）。
   * 这一行不说 key 的任何一个字符。
   */
  private renderPendingKeys(): void {
    this.pendingBox?.remove();
    if (this.pendingKeys.size === 0) {
      this.pendingBox = null;
      return;
    }
    const box = document.createElement("div");
    box.className = "accounts-new-pending";
    for (const name of this.pendingKeys.keys()) {
      const line = document.createElement("div");
      line.className = "accounts-hint";
      line.textContent = copyText("accounts.pending.waiting", { name });
      const drop = mkBtn(copyText("accounts.pending.drop"));
      drop.addEventListener("click", () => {
        this.pendingKeys.delete(name);
        this.renderPendingKeys();
      });
      line.appendChild(drop);
      box.appendChild(line);
    }
    const form = this.body.querySelector(".accounts-new");
    if (form) form.after(box);
    else this.body.appendChild(box);
    this.pendingBox = box;
  }

  /**
   * A2：把表单上收下的 key 写给**已经出现**的那几个号。
   *
   * ⚠ configDir 取自**这一趟列表里那个号自己的字段**（不透明串，`KH2C1`），前端不从名字推。
   * ⚠ 出现了但没有 configDir（账号 0 那种）⇒ 配了也不会被用上：丢掉 key 并说出来，不假装写成了。
   */
  private async flushPendingKeys(accounts: Account[]): Promise<void> {
    for (const [name, { key, origin, baseUrl }] of [...this.pendingKeys]) {
      // 〔RM1a〕只在建它的那台机器的列表里认领（见 `pendingKeys` 头注）。
      if (origin !== this.machineOrigin()) continue;
      const a = accounts.find((x) => x.name === name);
      if (!a) continue;
      this.pendingKeys.delete(name);
      if (!a.configDir) {
        showActionFailureToast(
          copyText("accounts.pending.notWritten"),
          copyText("accounts.pending.noDir", { name }),
          { level: "error" },
        );
        continue;
      }
      await this.writeApikey(key, a.configDir, name, baseUrl);
    }
    this.renderPendingKeys();
  }

  /**
   * 这一页此刻显的是哪台机器 —— apikey 那几条命令按它定目标（本机逐字送 `"<local>"`）。
   * 〔RM1a〕先前那三条命令不收 origin，远端页配的 key 落在本机。
   */
  private machineOrigin(): Origin {
    return this.origin ?? BACKEND_LOCAL_ORIGIN;
  }

  /** 唯一那一处把 key 交给后端的地方（行上的「保存」与表单的「建好后写」都走它）。 */
  private async writeApikey(
    key: string,
    configDir: string,
    name: string,
    // 〔ST2〕只有表单「建好后写」那一路带它；行上的「保存」只配 key（后端那一格不碰）。
    baseUrl?: string,
  ): Promise<void> {
    try {
      // 〔HX2 · 4D〕经通道交那台机器的后端（`apikey-key-set`，账号 id 由后端推）；先前是 Tauri 命令 `write_apikey_credentials_key`〔散文墓碑〕。
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
   * ③ 〔RM1a · 第四波〕这一页显的是 `this.origin` 那台机器的账号，读写那份文件的两条命令
   *    （〔US1〕读：经通道 `apikey-read`；〔HX2〕写：经通道 `apikey-key-set`）**按同一台机器**去
   *    （[`machineOrigin`]）—— 远端页读写的是那台机器上那一份，不再是本机的。
   *    「有没有行」（〔US1〕经通道 `apikey-routing`）同样问这一页那台机器。
   */
  private async readApikeyState(
    accounts: Account[],
  ): Promise<{ entries: ApikeyEditorAccount[]; fileBlock: HTMLElement }> {
    const dirs = accounts.map((a) => a.configDir).filter((d): d is string => !!d);
    let routed: string[] = [];
    try {
      // 〔RM1a〕问**这一页那台机器**（远端由那台的后端答），不再问本机。
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

  /** A6：已启用态的「维护」区——自检 / 补链 / rc 片段，均弹终端或只读。
   *  account-ux U7：整块收进 `<details>` **默认折叠**——都低频，补链还会改软链。
   *  🔴 `设计/70 §4.3` ①（A2）：**加账号已经搬出去了**（常驻的「新建账号」表单）——
   *  它是最常用的账号操作，不是维护。于是原来那条「只有 1 个号时默认展开，好让人看见加账号」
   *  的理由也跟着没了 ⇒ 默认一律折叠。 */
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

    // 自检 / 补链。
    const ops = document.createElement("div");
    ops.className = "accounts-maint-ops";
    const verifyBtn = mkBtn(copyText("accounts.maintenance.verify"));
    verifyBtn.addEventListener("click", () => void this.launchStep({ kind: "verify" }));
    const syncBtn = mkBtn(copyText("accounts.maintenance.sync"));
    syncBtn.addEventListener("click", () =>
      void this.launchStep(
        { kind: "sync-apply" },
        { danger: true, confirmExtra: copyText("accounts.maintenance.syncHint") },
      ),
    );
    const rcBtn = mkBtn(copyText("accounts.maintenance.rc"));
    rcBtn.title =
      copyText("accounts.maintenance.rcHint");
    const rcBox = document.createElement("div");
    rcBox.className = "accounts-maint-rc";
    rcBtn.addEventListener("click", () => void this.renderRcSnippet(rcBtn, rcBox));
    ops.append(verifyBtn, syncBtn, rcBtn);
    box.appendChild(ops);
    box.appendChild(rcBox);
    // 🔴 `K-R49`：**这一条路上刻意不把「按账号生成命令」那一块搬过来。**
    // 这张表显的是**远端那台**的账号，而别名是给**本机 shell** 用的
    // （`ccm --account <名>` 在这台机器上跑）⇒ 在这里挂那一块就得去读本机账号，
    // 而 `NF1bD3` 那条判据逐字断的正是「配了远端时，本机那条读口一次都不该被调」。
    // 它守的是「远端页上不许渲染本机的账号」，那条性质是对的 —— 所以这里让路。
    // 〔A2 · `70 §4.3` ⑤〕原先那句「加完之后到『设置 → 行为 → …』里写入」的散文导航已撤：
    // 命令名改成新建表单里的一行提示（`aliasHintFor`），管理器归 `设计/71 §13` 那一路。
    wrap.appendChild(box);
    return wrap;
  }

  /**
   * Z05（销 BACKLOG F14）：rc 片段一键生成。
   *
   * **单一来源留在 bash**：片段由远端 `cc-acct-iso shellinit` 产出，本文件**不重新生成一份**
   * ——那会多一个跨语言双写点（本工作区反复在治的病）。抓到什么贴什么。
   *
   * **这一块绝不代写**：只产出文本 + 复制按钮（`paste-block.ts` 的模块头也写死了
   * 「本文件没有、也不得有任何写入路径」）。
   *
   * 🔴 `K-R49`（09-10）订正这一段原先那句全称 —— 它逐字写着「**写 `~/.bashrc` 是用户明令
   * 的红线**」，而那句话今天只对**这一块**成立，别拿它去撤别处的活：
   * - 仍然成立的是「**不问自取**」那一半：没有用户当次手势就写他的 shell 配置，禁。
   * - 用户 09-10 逐字要的是反过来的事：「**我现在添加了一个账号但是没法直接添加命令,
   *   还得手动去改**」⇒ 按账号生成命令那一块（`buildAccountAliasBlock`〔散文墓碑〕，〔AL1〕今天是机器页的「别名」）**会真落盘**，
   *   而它把代价压到最小：重写的是 cc-monitor 自己那份文件，用户的 rc 最多多一行 `source`，
   *   且那份 rc 由他在下拉里自己选。整条推理住 `launcher-diagnostics.ts` 的模块头注。
   * - **这一块为什么仍然不代写**：它抓的是**远端** `cc-acct-iso shellinit` 的输出，
   *   而落盘那一侧今天在远端没有主人（`parity_ledger` 的 `alias.account-commands` 那行
   *   逐条记着欠什么）—— 是**还没做**，不是「不许做」。
   */
  private async renderRcSnippet(btn: HTMLButtonElement, box: HTMLElement): Promise<void> {
    const host = this.currentHost();
    if (!host) {
      showActionFailureToast(copyText("accounts.rc.noConfig"), copyText("accounts.rc.noConfigBody"), { level: "error" });
      return;
    }
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = copyText("accounts.rc.fetching");
    box.innerHTML = "";
    try {
      const snippet = await readAcctIsoSnippet(this.origin);
      box.appendChild(
        buildPasteBlock({
          text: () => snippet,
          target: copyText("accounts.rc.target"),
          mergeNote:
            copyText("accounts.rc.merge"),
          activation: copyText("accounts.rc.activation"),
          // 围栏在 Rust 侧已经校验过一次（拿不到就直接 Err）；这里再校验一次是因为
          // 「能显示」与「能贴」是两件事——半截片段贴进 rc 会让登录 shell 报错。
          invalidReason: (t) =>
            t.includes("# ===== BEGIN cc-acct-iso =====") &&
            t.includes("# ===== END cc-acct-iso =====")
              ? null
              : copyText("accounts.rc.incomplete"),
          multiline: true,
          rows: 12,
          className: "accounts-rc-paste",
        }).element,
      );
    } catch (e) {
      showActionFailureToast(copyText("accounts.rc.failed"), String(e), { level: "error" });
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
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
    // A6：打开该账号的终端（去 /login / 修复登录）——对 in-place 逃生口不给（不支持切号）。
    if (a.mode !== "in-place") {
      const login = document.createElement("button");
      login.type = "button";
      // K-A1：对 api-key 号说「去登录」是假话（它不需要 /login，/login 也修不了缺端点）。
      const action = accountLoginActionLabel(a);
      login.textContent = action.label;
      login.title = action.title;
      login.addEventListener("click", () => void this.launchStep({ kind: "login", name: a.name }));
      actions.appendChild(login);
    }
    // 🔴 `设计/70 §4.4` 关键二：**「哪个账号」只问一次** —— 配 apikey 是这一行自己的一格。
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

/** 〔MIG-3a · 09-28 裁 2〕`acct-iso-install` 的成品 ⇒ 给人读的一两句（已在的不动，如实说）。 */
function installedLine(r: AcctIsoInstalled): string {
  const parts = [
    r.written > 0
      ? copyText("accounts.needsDeploy.landed", { version: r.version, dest: r.dest, written: String(r.written) })
      : copyText("accounts.needsDeploy.current", { version: r.version, dest: r.dest }),
    r.linked
      ? copyText("accounts.needsDeploy.linked", { link: r.link })
      : copyText("accounts.needsDeploy.linkKept", { link: r.link }),
    r.configWritten
      ? copyText("accounts.needsDeploy.configWritten", { config: r.config })
      : copyText("accounts.needsDeploy.configKept", { config: r.config }),
  ];
  if (r.recordFailed) parts.push(r.recordFailed);
  return parts.join("");
}
