// B03：cc-bus 驾驶舱（批一只读 + 批二派活/收信/图形化 spawn）。
//
// 三条硬约束决定了这个形状：
//  ① **不新增轮询**（红线）。cc-bus 的状态在**跑着 cc-bus 的那台机**的 `~/.cc-bus/`。
//     ⚠ 原文写「cc-monitor 跑在 Windows 只能经 SSH 看」——**那是把一种部署当成了全部**：
//     cc-monitor 也跑在 Linux 上，而那台机器上 `~/.cc-bus/` 就在本地（P4a 实测 86 行 agents）。
//     ⇒ P4a 起，读面三条支持 `<local>`（后端走同一条命令串、只是不包进 ssh）。复用后端的 inotify watcher 要改后端（零改红线），
//     所以只能按需读。**本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务。**
//  ② **登记 ≠ 在线**。`agents.tsv` 只证明它登记过——实测最早的条目是 10 天前的，进程早没了。
//     判在线要另查 `tmux has-session`，那是**第二次往返**，所以放在用户点某一行的「检查」上，
//     不默认全量查（一屏 37 个 agent 就是 37 次 tmux 调用）。
//  ③ **脏数据不能把面板搞崩**。实测 `spawned.tsv` 15 行里 8 行是坏的（53%），
//     后端解析器跳过并计数，这里**如实显示「N 条无法解析」**，不假装干净。
//
// **与计划措辞的一处偏离（更严格，非放宽）**：计划写「打开分节时才 invoke 一次」。
// `CollapsibleGroup` 没有展开回调，而为一个消费者去改这个共享 UI 原语，正是
// R12/R15 反复拒绝的"为假想需求建抽象"。这里改成**用户点「读取」才发请求**——一次 30s
// 超时的远端往返，显式触发比"展开即偷偷发"更诚实，也天然满足"启动时不预取"。
//
// **批二不重写起会话**：图形化 spawn 调的是收编后的 `cc-spawn`（它内部已改经 `ccm`），
// cc-monitor 侧不碰建会话逻辑——那正是本工作区消灭的病（账本 K8：再造第 N 套实现）。
// 也因此本文件**零引用 launch IR 模块**：spawn 是 fire-and-forget 的远端 exec，不开标签页。
import { setCurrentMachine, subscribeMachine } from "./machine-context";
import { commands } from "../ipc/commands";
import { installCcBus, readCcBusInstallState } from "../cc-bus-install-reads";
// 〔C4e · 第四波 4C〕查在线 · 发消息 · 收掉 · 派生 · 广播五件经通道直接问那台机器的后端（原是五条 Tauri 命令）。
import { agentOnline, broadcast, killAgent, readInbox, readState, sendMessage, spawnAgent, type BusState } from "../cc-bus-control";
import { saidOfControl } from "../control-said";
import { showActionFailureToast } from "../error-toast";
// L2：账号选择复用既有封装——`fetchAccounts` 带 TTL 缓存、`selectableAccounts` 是
// 「可选账号」的单一判据（`accounts.ts:130` 注释明写"别各处再 filter 一遍"）。
import { selectableAccounts } from "../accounts";
import { fetchAccounts } from "../account-reads";
// 本机 origin：`backend-policy.ts` 的 `"<local>"`（与 Rust 侧 `inbound_client::LOCAL_ORIGIN` 逐字相同，跨语言钉住）。
// 〔C4b〕`accounts.ts` 先前那个同名的 `"__local__"`（账号面的标记）已退役 —— 全仓只剩这一个本机表示。
import { LOCAL_ORIGIN } from "../backend-policy";

// 〔SH1 · V136〕读面经通道直接问后端（`bus-state` / `bus-inbox`），形状由 `cc-bus-control.ts` 的解码器严格收。
import { askConfirm } from "../ask-dialog";
import { copyText } from "../copy-table";

export class CcBusSection {
  readonly element: HTMLElement;
  private originSel!: HTMLSelectElement;
  private readBtn!: HTMLButtonElement;
  private deployBtn?: HTMLButtonElement;
  private statusEl!: HTMLElement;
  private listBox!: HTMLElement;
  private spawnDir!: HTMLInputElement;
  private spawnTask!: HTMLInputElement;
  private spawnTool!: HTMLSelectElement;
  private spawnAcct!: HTMLSelectElement;
  private spawnBtn!: HTMLButtonElement;
  private spawnOut!: HTMLElement;
  /** spawn 二次确认。**记住"确认的是哪一组参数"而不只是一个布尔**（B03 审计重要-1）：
   *  原实现只有 `spawnArmed: boolean`，且只在成功执行时复位，于是
   *  「武装 → 改目录/改 tool → 再点」会**用新值执行**，用户确认过的那句话描述的是一个
   *  从未发生的操作。这里改成存下确认时的参数快照，点第二次时比对，不一致就重新武装。 */
  private armedFor: string | null = null;
  /** P4c：收掉那颗按钮的两步确认状态（与 spawn 的 `armedFor` 分开 —— 两件事各自武装）。 */
  private killArmedFor: string | null = null;
  /** 武装中的那颗按钮本身 —— 只记 id 复位不了它（见 `killOne` 的 D 阶段补审）。 */
  private killArmedBtn: HTMLButtonElement | null = null;
  private broadcastInput!: HTMLInputElement;
  private broadcastBtn!: HTMLButtonElement;
  /** 已加载过的状态；null = 还没读过（**不在构造时预取**）。 */
  private state: BusState | null = null;

  constructor() {
    this.element = this.build();
    // **刻意不在这里 invoke** 读 cc-bus 状态。见文件头「与计划措辞的一处偏离」。
    void this.loadOrigins();
    // ★ `PS2` 例外，且是**按那条纪律自己的理由**开的：它禁的是「展开即偷偷发**一次 30s
    //   的远端往返**」。而本机安装状态是**纯本地文件比对**（17 个文件、无 SSH、毫秒级）——
    //   理由不适用。⇒ 构造时算一次，否则打开面板看到的是默认文案，
    //   而「装着旧版却显示『装到本机』」正是本件要消灭的那种骗人形态。
    void this.refreshInstallState();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless cc-bus-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      copyText("ccBus.build.intro");
    root.appendChild(hint);

    const row = document.createElement("div");
    row.className = "settings-row cc-bus-controls";

    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = copyText("ccBus.build.machine");
    row.appendChild(label);

    this.originSel = document.createElement("select");
    this.originSel.className = "settings-input cc-bus-origin";
    row.appendChild(this.originSel);

    // P4c（#77/#78）：广播 —— 面板此前只能给**单个**收件人发。
    const bcast = document.createElement("input");
    bcast.type = "text";
    bcast.className = "settings-input cc-bus-broadcast-input";
    bcast.placeholder = copyText("ccBus.build.broadcastHint");
    this.broadcastInput = bcast;
    row.appendChild(bcast);
    const bcastBtn = document.createElement("button");
    bcastBtn.type = "button";
    bcastBtn.className = "settings-btn cc-bus-broadcast";
    bcastBtn.textContent = copyText("ccBus.build.broadcast");
    bcastBtn.addEventListener("click", () => void this.doBroadcast());
    this.broadcastBtn = bcastBtn;
    row.appendChild(bcastBtn);

    this.readBtn = document.createElement("button");
    this.readBtn.type = "button";
    this.readBtn.className = "settings-btn cc-bus-read";
    this.readBtn.textContent = copyText("ccBus.build.read");
    this.readBtn.addEventListener("click", () => void this.reload());
    row.appendChild(this.readBtn);

    // ★ `PS1`：把内嵌的 cc-bus 装到 `<claude_dir>/skills/cc-bus/`。
    //
    // ⚠⚠ 这是**只读铁律的第 7 条例外**（`U10b` 用@08-13 裁「开」）——本仓唯一往
    // `<claude_dir>` 写的口子。四个配套里的**「用户显式动作」就是这颗按钮**：
    // 它绝不能被放进启动 / 刷新 / 任何自动路径。改动本段前先读 `cc_bus_deploy.rs` 的头注。
    // ⚠ 不做两步确认：它**幂等且可撤销**（覆盖前留 `cc-bus.bak-<ts>`），
    //   与 `cc-kill` 那种不可撤销的破坏性动作不是一档 —— 那里两步是必需的，这里不是。
    const deployBtn = document.createElement("button");
    deployBtn.type = "button";
    deployBtn.className = "settings-btn cc-bus-deploy";
    deployBtn.textContent = copyText("ccBus.build.install");
    deployBtn.title =
      copyText("ccBus.build.installHint");
    deployBtn.addEventListener("click", () => void this.doDeploy(deployBtn));
    this.deployBtn = deployBtn;
    row.appendChild(deployBtn);

    root.appendChild(row);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "settings-hint cc-bus-status";
    this.statusEl.textContent = copyText("ccBus.build.notRead");
    root.appendChild(this.statusEl);

    this.listBox = document.createElement("div");
    this.listBox.className = "cc-bus-list";
    root.appendChild(this.listBox);

    root.appendChild(this.buildSpawnForm());
    return root;
  }

  /**
   * `PS1`：装到本机。
   *
   * ⚠ 结果**分三种说法**，不合并 —— 用户点一次得知道到底动没动盘：
   * · 写了 N 个 ⇒ 说写了几个、备份在哪；
   * · 一个没写（幂等命中）⇒ 明说「已是最新」，**不假装干了活**；
   * · 失败 ⇒ 原样把错误摆出来（围栏拒收的理由是逐字的，别吞）。
   */
  /**
   * `PS2`：按**三态**说话，并让按钮的**文案跟着状态变**。
   *
   * ⚠ 三态刻意不合并（理由见 `src/cc-bus-install-reads.ts` 与后端 `cc_bus_install.rs::state_at` 的头注）：
   * 把「装了旧版」说成「已装」，正是 `P4b` 那一刀卡了两天的形态 ——
   * 装着的是旧的，而界面说已装，于是**没人会去点那颗按钮**。
   * ⚠ 读失败**说读失败**，不退化成「未装」（本仓一路在收的那一族）。
   */
  private async refreshInstallState(): Promise<void> {
    if (!this.deployBtn) return;
    try {
      const st = await readCcBusInstallState();
      if (st.state === "not_installed") {
        this.deployBtn.textContent = copyText("ccBus.refreshInstallState.install");
        this.deployBtn.title = copyText("ccBus.install.missing");
      } else if (st.state === "up_to_date") {
        this.deployBtn.textContent = copyText("ccBus.install.upToDate");
        this.deployBtn.title = copyText("ccBus.install.upToDateHint");
      } else {
        const n = st.differing + st.missing;
        this.deployBtn.textContent = copyText("ccBus.install.update", { n });
        this.deployBtn.title =
          copyText("ccBus.install.stale", { differing: st.differing, missing: st.missing });
      }
    } catch (e) {
      // 读不到就说读不到 —— **不许**退化成「未装」，那会让用户以为要装。
      this.deployBtn.textContent = copyText("ccBus.refreshInstallState.install");
      this.deployBtn.title = copyText("ccBus.install.unknown", { e: String(e) });
    }
  }

  private async doDeploy(btn: HTMLButtonElement): Promise<void> {
    const prev = btn.textContent;
    btn.disabled = true;
    btn.textContent = copyText("ccBus.deploy.running");
    try {
      // 〔MIG-3a · 子步 3〕判 · 写 · 记在本机后端（`cc-bus-install`）；「ccm 够不够新」是 monitor 探本机 ccm 的事，另问一次。
      const r = await installCcBus();
      const warning = await commands.cc_bus_ccm_precheck();
      if (r.written === 0) {
        this.statusEl.textContent = copyText("ccBus.deploy.unchanged", { dest: r.dest, unchanged: r.unchanged });
      } else {
        const bak = r.backup ? copyText("ccBus.deploy.backup", { backup: r.backup }) : "";
        this.statusEl.textContent = copyText("ccBus.deploy.done", { dest: r.dest, written: r.written, bak });
      }
      // ★★ 装成功了、但装出来的东西现在跑不起来 —— 这句必须**显示出来**〔08-13〕。
      //
      // `C15` 之后 cc-spawn 硬依赖新 `ccm`（开头能力协商，缺一条就 exit 2）。
      // 只装 cc-bus、不同步 ccm ⇒ 这一步一切正常，用户的 `cc-spawn` 当场不能用。
      // ⚠ 后端那侧同时也写了日志 —— 但**用户不会去翻日志**：「成功 + 一句日志」
      //   在他眼里就是纯成功，正是本仓一路在治的「假成功比失败更坏」。
      // ⚠ 它**不是错误**（装本身做完了），所以接在成功文案后面，而不是走失败 toast。
      if (r.recordFailed) this.statusEl.textContent += ` ⚠ ${r.recordFailed}`;
      if (warning) {
        // ⚠ 分隔符用普通空格：全角空格会被 `no-irregular-whitespace` 判错，
        //   而 `eslint-baseline` 那条判据是**等号**（全仓错误数就是基线那个数）—— 它当场逮住了。
        this.statusEl.textContent += ` ⚠ ${warning}`;
      }
    } catch (e) {
      showActionFailureToast(copyText("ccBus.deploy.failed"), String(e));
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
      // 装完立刻重算状态 —— 否则按钮还写着「更新（差 N 个）」，而盘上已经是最新的。
      await this.refreshInstallState();
    }
  }

  /** 批二：图形化 spawn。**调收编后的 cc-spawn，不在这里重写起会话。** */
  private buildSpawnForm(): HTMLElement {
    const box = document.createElement("div");
    box.className = "cc-bus-spawn";

    const t = document.createElement("div");
    t.className = "settings-label";
    t.textContent = copyText("ccBus.spawn.title");
    box.appendChild(t);

    const h = document.createElement("div");
    h.className = "settings-hint";
    h.textContent =
      copyText("ccBus.spawn.intro");
    box.appendChild(h);

    this.spawnDir = document.createElement("input");
    this.spawnDir.type = "text";
    this.spawnDir.className = "settings-input cc-bus-spawn-dir";
    this.spawnDir.placeholder = copyText("ccBus.spawn.dir");
    box.appendChild(this.spawnDir);

    this.spawnTask = document.createElement("input");
    this.spawnTask.type = "text";
    this.spawnTask.className = "settings-input cc-bus-spawn-task";
    this.spawnTask.placeholder = copyText("ccBus.spawn.task");
    box.appendChild(this.spawnTask);

    this.spawnTool = document.createElement("select");
    this.spawnTool.className = "settings-input cc-bus-spawn-tool";
    for (const v of ["claude", "codex"]) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = v;
      this.spawnTool.appendChild(o);
    }
    box.appendChild(this.spawnTool);

    // L2（B03 审计重要-5）：**必须让用户表态用哪个账号**。原实现没有这个控件，于是
    // 点两下就在 manifest 默认号上起真 agent 烧额度——用户既没选过，也不知道用了哪个号。
    // 默认项是「基座」而不是某个具体账号：**不替用户默认花掉某个号的额度**。
    this.spawnAcct = document.createElement("select");
    this.spawnAcct.className = "settings-input cc-bus-spawn-acct";
    box.appendChild(this.spawnAcct);
    this.renderAccountOptions([]);

    // 任一参数变化立刻解除武装——文案承诺了"参数改动要重新确认"，代码就得兑现。
    // （原实现的文案还写着"点别处不算"，而代码里根本没有任何"点别处"的处理；
    //  对用户做代码不兑现的承诺，比不做承诺更坏。那句话已删。）
    for (const el of [this.spawnDir, this.spawnTask, this.spawnTool, this.spawnAcct] as HTMLElement[]) {
      el.addEventListener("input", () => this.disarmSpawn());
      el.addEventListener("change", () => this.disarmSpawn());
    }

    this.spawnBtn = document.createElement("button");
    this.spawnBtn.type = "button";
    this.spawnBtn.className = "settings-btn cc-bus-spawn-go";
    this.spawnBtn.textContent = copyText("ccBus.spawn.go");
    this.spawnBtn.addEventListener("click", () => void this.doSpawn());
    box.appendChild(this.spawnBtn);

    this.spawnOut = document.createElement("div");
    this.spawnOut.className = "settings-hint cc-bus-spawn-out";
    box.appendChild(this.spawnOut);

    return box;
  }

  /** 列远端。复用既有 `list_remote_mcp_origins`——它其实是通用的「列远端配置标签」，
   *  名字带 mcp 只是历史；为同一件事再加一条 IPC 是无谓重复。 */
  private async loadOrigins(): Promise<void> {
    let origins: string[] = [];
    try {
      // **别只防 reject**：invoke 也可能 resolve 成 undefined/非数组（桥接层异常、命令改了
      // 返回类型）。只 catch 不校验形状的话，下一行 `.length` 会直接抛 —— 这正是本工作区
      // 一路在守的「脏数据不能把面板搞崩」，对自己的 IPC 返回值同样适用。
      const got = await commands.list_remote_mcp_origins();
      if (Array.isArray(got)) origins = got;
    } catch {
      /* 拿不到就当没有远端，不影响面板其余部分 */
    }
    this.originSel.replaceChildren();
    // P4a：**本机永远在列表里**。cc-monitor 跑在哪台机器上，那台机器的 `~/.cc-bus/`
    // 就在本地 —— 后端读面已经支持 `<local>`（同一条命令串，不包进 ssh）。
    // ⇒ 「没有可选项」这种状态不再存在，那条 `origins.length === 0` 的死路去掉了。
    for (const o of origins) {
      const opt = document.createElement("option");
      opt.value = o;
      opt.textContent = o;
      this.originSel.appendChild(opt);
    }
    // ⚠ **追加在末尾，不是插在开头**：`select` 的默认值是第一项 ——
    // 放开头会把「配了远端的人打开面板默认看哪台」这件事一起改了，
    // 而那不是本件要动的东西（P4a 的正题是「本机也能看」，不是「默认改看本机」）。
    {
      const opt = document.createElement("option");
      opt.value = LOCAL_ORIGIN;
      opt.textContent = copyText("ccBus.machine.local");
      this.originSel.appendChild(opt);
    }
    if (origins.length === 0) {
      this.statusEl.textContent = copyText("ccBus.machine.noRemote");
    }
    // 账号随机器变——换台机器，上一台的账号名多半不适用
    this.originSel.addEventListener("change", () => {
      // S4a：写进共用 store；实际切换由订阅统一处理。
      // 〔C4a〕共用 store 与本选择器现在是**同一个表示**（本机 = `LOCAL_ORIGIN`）——原先那处换算没有了。
      const v = this.originSel.value;
      setCurrentMachine(v);
      this.syncLocalAffordances();
      // 〔第三波 S3〕在这里换账号下拉。原先只靠下面那条订阅去换 —— 而订阅看到选择器**已经**是那台
      // （就是这一行刚选的）就早退，于是在下拉里换机器，账号下拉一直停在上一台的名单上。
      void this.loadAccounts(v);
    });
    // S4a：跟随共用 store。本机（`LOCAL_ORIGIN`）—— **P4a 起它不再是「原地不动」**：
    // 本机这一格今天有意义了（读面已通），所以跟着切到「本机」那一项。
    subscribeMachine((origin) => {
      const want = origin;
      if (![...this.originSel.options].some((o) => o.value === want)) return;
      if (this.originSel.value === want) return;
      this.originSel.value = want;
      this.disarmSpawn();
      this.disarmKill(); // 切了机器，上一台那颗武装中的「收掉」必须失效
      this.syncLocalAffordances();
      void this.loadAccounts(want);
    });
    this.syncLocalAffordances();
    // 〔第三波 S3〕本机也拉账号列表（BS1b 留下的：原先这两处对本机跳过 ⇒ 本机派生只能选「不指定」）。
    // `fetchAccounts` 收到后端那个本机串（`backend-policy` 的 `LOCAL_ORIGIN`）自己走本机那条读口
    // （`accounts.ts` 的 `fetchAccounts` 第一行，A3 接的）—— 这里不另写一条本机分支。
    void this.loadAccounts(this.originSel.value);
  }

  /** 渲染账号下拉。第一项恒为「基座」——**不替用户默认选一个会花钱的号**。 */
  private renderAccountOptions(names: string[]): void {
    this.spawnAcct.replaceChildren();
    const base = document.createElement("option");
    base.value = ""; // 空串 → 后端转发 `--base`（显式不注入）
    base.textContent = copyText("ccBus.spawn.accountNone");
    this.spawnAcct.appendChild(base);
    for (const n of names) {
      const o = document.createElement("option");
      o.value = n;
      o.textContent = copyText("ccBus.spawn.accountNamed", { n });
      this.spawnAcct.appendChild(o);
    }
  }

  /** 取这台机器（远端或本机）的可选账号。**拿不到就只留「基座」**——宁可少一个选项，
   *  也不能让用户以为选了某个号而其实没生效。 */
  /** 〔BS1b 09-24〕本机派生今天走后端原语 `bus-spawn`，与远端**同一条路** ⇒ 不再按本机禁用。
   *  （原先这里对本机禁用派生按钮并挂一句「本机还不能派生」—— 那句话等的原语长出来了。）
   *  三处切机器的调用点仍调它：哪天真有「这台机器做不了」的东西，写在这里。 */
  private syncLocalAffordances(): void {
    this.spawnBtn.disabled = false;
    this.spawnBtn.title = "";
  }

  private async loadAccounts(origin: string): Promise<void> {
    let names: string[] = [];
    try {
      const st = await fetchAccounts(origin);
      names = Array.isArray(st?.accounts) ? selectableAccounts(st).map((a) => a.name) : [];
    } catch {
      /* 拿不到 ⇒ 只留「不指定」 */
    }
    // 读在路上时用户又换了一台 ⇒ 这一趟的名单是上一台的，不许盖上去。
    if (this.originSel.value !== origin) return;
    this.renderAccountOptions(names);
  }

  private async reload(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    this.readBtn.disabled = true;
    this.statusEl.textContent = copyText("ccBus.reload.reading");
    this.listBox.replaceChildren();
    try {
      this.state = await readState(origin);
      this.render();
    } catch (e) {
      this.state = null;
      // 失败要说清是哪一步失败，而不是留个空面板让人以为"没有 agent"
      this.statusEl.textContent = copyText("ccBus.reload.readFailed", { e: saidOfControl(e) });
    } finally {
      this.readBtn.disabled = false;
    }
  }

  private render(): void {
    const st = this.state;
    if (!st) return;
    const spawnedIds = new Set(st.spawned.map((s) => s.id));
    const dirOf = new Map(st.spawned.map((s) => [s.id, s.dir]));
    const registered = new Set(st.agents.map((a) => a.id));

    // **spawned-only 的条目也要渲染**（B03 审计阻塞-1，用真实数据复现）：
    // 盘上实测 agents=37 / spawned=7 / **交集只有 2**——原实现只遍历 `agents`，于是另外
    // 5 个 cc-spawn 派生的 agent **连同它们的工作目录一行都不显示**，而头条却写着
    // 「其中 spawn 的 7 个」（`其中` 蕴含子集关系，我却拿 spawned 全集去数）。
    // 数字与可见行数差 3.5 倍，且差的方向是**让人以为看全了**——这个分节唯一的职责
    // 就是如实呈现，这是最不该犯的错。
    // 修法取"并进列表"而非"只改计数"：spawned-only 的条目有 dir 和时间，
    // 信息量比 agents.tsv 还大，藏起来没有道理。
    const extra = st.spawned.filter((sp) => !registered.has(sp.id));
    const bothCount = st.agents.filter((a) => spawnedIds.has(a.id)).length;

    const parts = [copyText("ccBus.summary.registered", { n: st.agents.length })];
    if (bothCount > 0) parts.push(copyText("ccBus.summary.spawned", { n: bothCount }));
    if (extra.length > 0) parts.push(copyText("ccBus.summary.unregistered", { n: extra.length }));
    if (st.skipped > 0) parts.push(copyText("ccBus.summary.skipped", { skipped: st.skipped }));
    parts.push(copyText("ccBus.summary.notOnline"));
    this.statusEl.textContent = parts.join(copyText("ccBus.summary.sep"));

    this.listBox.replaceChildren();
    if (st.agents.length === 0 && extra.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = copyText("ccBus.render.empty");
      this.listBox.appendChild(empty);
      return;
    }
    for (const a of st.agents) {
      this.listBox.appendChild(this.buildRow(a, spawnedIds.has(a.id), dirOf.get(a.id), true));
    }
    // 未登记的 spawn 记录：**明确标注它没在总线上**，别让用户以为它是个正常 agent
    for (const sp of extra) {
      this.listBox.appendChild(
        this.buildRow(
          { id: sp.id, registered_at: sp.spawned_at },
          true,
          sp.dir,
          false,
        ),
      );
    }
  }

  private buildRow(
    a: { id: string; registered_at: string },
    isSpawned: boolean,
    dir: string | undefined,
    registered: boolean,
  ): HTMLElement {
    const row = document.createElement("div");
    row.className = "cc-bus-row";
    row.dataset.agentId = a.id; // 靠 dataset 认身份，不靠 textContent

    const idEl = document.createElement("span");
    idEl.className = "cc-bus-id";
    idEl.textContent = a.id;
    row.appendChild(idEl);

    const meta = document.createElement("span");
    meta.className = "cc-bus-meta";
    const bits: string[] = [];
    if (dir) bits.push(dir);
    bits.push(a.registered_at || copyText("ccBus.row.timeUnknown"));
    bits.push(isSpawned ? copyText("ccBus.row.spawned") : copyText("ccBus.row.selfRegistered"));
    if (!registered) bits.push(copyText("ccBus.row.unregistered"));
    meta.textContent = bits.join(copyText("ccBus.row.sep"));
    row.appendChild(meta);

    // **在线状态默认「未知」**——这是本设计的要点，不是偷懒：名单证明不了在线，
    // 而全量查是 N 次往返。用户想知道哪一个，就点哪一个。
    const stateEl = document.createElement("span");
    // 〔AR1 · `设计/41 §7` 约定 3：状态用 `data-*`，不用类名〕此前是 `cc-bus-online-<态>` 五个类名，
    //   CSS 里一条规则都没有（git 史里也从来没有过）⇒ 悬空类；改成 `data-state`，界面上只说那几个字。
    stateEl.className = "cc-bus-online";
    stateEl.dataset.state = "unknown";
    stateEl.textContent = copyText("ccBus.row.onlineUnknown");
    row.appendChild(stateEl);

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "settings-btn cc-bus-check";
    btn.textContent = copyText("ccBus.row.check");
    btn.addEventListener("click", () => void this.checkOne(a.id, stateEl, btn));
    row.appendChild(btn);

    // 批二：收信 + 发消息。两者都是按需一次往返，不订阅、不轮询。
    const detail = document.createElement("div");
    detail.className = "cc-bus-detail";
    row.appendChild(detail);

    const inboxBtn = document.createElement("button");
    inboxBtn.type = "button";
    inboxBtn.className = "settings-btn cc-bus-inbox";
    inboxBtn.textContent = copyText("ccBus.row.inbox");
    inboxBtn.addEventListener("click", () => void this.loadInbox(a.id, detail, inboxBtn));
    row.appendChild(inboxBtn);

    const msg = document.createElement("input");
    msg.type = "text";
    msg.className = "settings-input cc-bus-msg";
    msg.placeholder = copyText("ccBus.row.messageHint");
    row.appendChild(msg);

    const sendBtn = document.createElement("button");
    sendBtn.type = "button";
    sendBtn.className = "settings-btn cc-bus-send";
    sendBtn.textContent = copyText("ccBus.row.send");
    sendBtn.addEventListener("click", () => void this.sendTo(a.id, msg, detail, sendBtn));
    row.appendChild(sendBtn);

    // P4c（#77/#78）：收掉这个 agent。**破坏性且不可撤销**（cc-kill 头注逐字：杀会话+进程树）
    // ⇒ 两步确认，抄 spawn 那条先例；第二步的文案**逐字带上 id**（回显真名，别让人杀错）。
    const killBtn = document.createElement("button");
    killBtn.type = "button";
    killBtn.className = "settings-btn cc-bus-kill";
    killBtn.textContent = copyText("ccBus.row.kill");
    killBtn.title = copyText("ccBus.row.killHint");
    killBtn.addEventListener("click", () => void this.killOne(a.id, detail, killBtn));
    row.appendChild(killBtn);

    return row;
  }

  private async checkOne(
    id: string,
    stateEl: HTMLElement,
    btn: HTMLButtonElement,
  ): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    btn.disabled = true;
    stateEl.dataset.state = "checking";
    stateEl.textContent = copyText("ccBus.check.running");
    try {
      const online = await agentOnline(origin, id);
      stateEl.dataset.state = online ? "yes" : "no";
      stateEl.textContent = online ? copyText("ccBus.check.online") : copyText("ccBus.check.offline");
    } catch (e) {
      // 查失败 ≠ 不在线，必须区分开，否则会把"网络抖了一下"报成"agent 死了"
      stateEl.dataset.state = "error";
      stateEl.textContent = copyText("ccBus.check.failed", { e: saidOfControl(e) });
    } finally {
      btn.disabled = false;
    }
  }

  private async loadInbox(id: string, box: HTMLElement, btn: HTMLButtonElement): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    btn.disabled = true;
    box.replaceChildren();
    box.textContent = copyText("ccBus.loadInbox.reading");
    try {
      const msgs = (await readInbox(origin, id)).messages;
      box.replaceChildren();
      if (msgs.length === 0) {
        box.textContent = copyText("ccBus.inbox.empty");
        return;
      }
      // 只渲染尾部若干条：后端已限 200 行，这里再收一次，面板不该被一屏刷爆
      for (const m of msgs.slice(-20)) {
        const line = document.createElement("div");
        line.className = "cc-bus-msg-line";
        line.textContent = `[${m.ts || "?"}] ${m.from || "?"}${m.class ? `(${m.class})` : ""}: ${m.text}`;
        box.appendChild(line);
      }
    } catch (e) {
      box.textContent = copyText("ccBus.inbox.failed", { e: saidOfControl(e) });
    } finally {
      btn.disabled = false;
    }
  }

  private async sendTo(
    id: string,
    input: HTMLInputElement,
    box: HTMLElement,
    btn: HTMLButtonElement,
  ): Promise<void> {
    const origin = this.originSel.value;
    const text = input.value;
    if (!origin || !text.trim()) return;
    btn.disabled = true;
    try {
      // 〔C4e〕那一句由成品的三态说（在线 / 不在线 / 名字没登记过 / 问不到），不再一律「已发送」。
      box.textContent = await sendMessage(origin, id, text);
      input.value = "";
    } catch (e) {
      box.textContent = copyText("ccBus.send.failed", { e: saidOfControl(e) });
    } finally {
      btn.disabled = false;
    }
  }

  /** 当前表单参数的指纹——确认的必须**正好**是执行的那一组。 */
  private spawnFingerprint(): string {
    return JSON.stringify([
      this.originSel.value,
      this.spawnDir.value.trim(),
      this.spawnTask.value,
      this.spawnTool.value,
      this.spawnAcct.value,
    ]);
  }

  /** 确认文案里要点名账号——「消耗额度」不说清是哪个号的额度等于没说。 */
  private acctLabel(): string {
    return this.spawnAcct.value ? copyText("ccBus.acctLabel.named", { value: this.spawnAcct.value }) : copyText("ccBus.acctLabel.none");
  }

  /**
   * P4c：收掉一个 agent。**两步确认** —— 第一次点只武装，第二次才真发。
   *
   * ⚠ 第一次点**一条命令都不许发出去**：发出去的那条才是杀人的，
   * 「弹了确认」和「没发命令」是两件事（判据钉的是后者）。
   */
  private async killOne(id: string, detail: HTMLElement, btn: HTMLButtonElement): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    if (this.killArmedFor !== id) {
      // ★ **先把上一颗复位**〔D 阶段补审〕：只记 id 不记按钮的话，
      // 武装 A 之后再点 B，A 那颗仍显示「确认收掉 A」—— **两颗都像武装着**，面板在骗人。
      // 本文件 `:53` 的注释记过同型病：「原实现只有 `spawnArmed: boolean`，且只在成功执行时复位」。
      this.disarmKill();
      this.killArmedFor = id;
      this.killArmedBtn = btn;
      // 回显真名 —— 一屏几十个 agent，不带名字的「确认」很容易杀错那一个。
      btn.textContent = copyText("ccBus.kill.confirm", { id });
      return;
    }
    this.disarmKill();
    btn.disabled = true;
    try {
      detail.textContent = await killAgent(origin, id);
      await this.reload();
    } catch (e) {
      detail.textContent = copyText("ccBus.kill.failed", { e: saidOfControl(e) });
      btn.disabled = false;
    }
  }

  /**
   * P4c：广播。**确认必须带数字** —— 一个不带数字的「确定吗」等于没问：
   * 用户点确认时并不知道有多少人会收到（实测本机 `agents.tsv` 86 行）。
   */
  private async doBroadcast(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    const text = this.broadcastInput.value.trim();
    if (!text) return;
    const n = this.state?.agents.length ?? 0;
    if (!(await askConfirm(copyText("ccBus.broadcast.confirm", { n, text })))) return;
    this.broadcastBtn.disabled = true;
    try {
      this.statusEl.textContent = await broadcast(origin, text);
      this.broadcastInput.value = "";
    } catch (e) {
      this.statusEl.textContent = copyText("ccBus.broadcast.failed", { e: saidOfControl(e) });
    } finally {
      this.broadcastBtn.disabled = false;
    }
  }

  /** P4c D 补审：把武装中的「收掉」复位。切机器 / 重载 / 武装另一颗时都要调。 */
  private disarmKill(): void {
    if (this.killArmedBtn) this.killArmedBtn.textContent = copyText("ccBus.row.kill");
    this.killArmedBtn = null;
    this.killArmedFor = null;
  }

  private disarmSpawn(): void {
    this.armedFor = null;
    this.spawnBtn.textContent = copyText("ccBus.spawn.go");
  }

  /** 两步确认：起一个真 agent 会消耗额度，一键就走太危险。 */
  private async doSpawn(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    const dir = this.spawnDir.value.trim();
    if (!dir) {
      // **先解除武装再返回**（审计重要-1）：原实现这条 return 在武装判断**之前**，于是
      // 「武装 → 清空 dir → 点击（只提示请填目录，**仍处武装态**）→ 填新 dir → 点一次」
      // = 一次点击就起 agent，全程没出现过确认文案。
      this.disarmSpawn();
      this.spawnOut.textContent = copyText("ccBus.spawn.needDir");
      return;
    }
    const fp = this.spawnFingerprint();
    if (this.armedFor !== fp) {
      // 未武装，或武装后参数被改过 → （重新）武装，把要做的事原样说清楚
      const changed = this.armedFor !== null;
      this.armedFor = fp;
      this.spawnBtn.textContent = copyText("ccBus.spawn.confirm");
      this.spawnOut.textContent =
        copyText("ccBus.spawn.confirmBody", { changed: (changed ? copyText("ccBus.spawn.changed") : ""), machine: origin, dir, account: this.acctLabel(), tool: this.spawnTool.value });
      return;
    }
    this.disarmSpawn();
    this.spawnBtn.disabled = true;
    this.spawnOut.textContent = copyText("ccBus.spawn.running");
    try {
      this.spawnOut.textContent = await spawnAgent(origin, {
        dir,
        task: this.spawnTask.value,
        tool: this.spawnTool.value,
        // 空串 = 显式基座（发 `base:true`）。**不存在"什么都不传"这一档**。
        account: this.spawnAcct.value,
      });
      // 派生完顺手刷新名单——这是**用户动作触发**的一次读，不是后台轮询
      await this.reload();
    } catch (e) {
      this.spawnOut.textContent = copyText("ccBus.spawn.failed", { e: saidOfControl(e) });
    } finally {
      this.spawnBtn.disabled = false;
    }
  }
}
