// B03：cc-bus 驾驶舱（批一只读 + 批二派活/收信/图形化 spawn）。
//
// 三条硬约束决定了这个形状：
//  ① **不新增轮询**（红线）。cc-bus 的状态全在远端本机 `~/.cc-bus/`，cc-monitor 跑在
//     Windows 只能经 SSH 看。复用 daemon 的 inotify watcher 要改 daemon（零改红线），
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
// L2：账号选择复用既有封装——`fetchAccounts` 带 TTL 缓存、`selectableAccounts` 是
// 「可选账号」的单一判据（`accounts.ts:130` 注释明写"别各处再 filter 一遍"）。
import { fetchAccounts, selectableAccounts } from "../accounts";

// C04d 批 5a：四个类型换成生成物（源 `cc_bus.rs`）。手写版与生成物**逐字等价** ⇒ 零漂移，
// 价值是防将来漂。`CcBusState.skipped` 在 Rust 侧是 `usize`
// ——**ts-rs 把它映射成 `number` 而不是 `bigint`**，所以 C03 那条大整数性质对它不适用。
import type { CcBusAgent } from "../generated/CcBusAgent";
import type { CcBusState } from "../generated/CcBusState";

export class CcBusSection {
  readonly element: HTMLElement;
  private originSel!: HTMLSelectElement;
  private readBtn!: HTMLButtonElement;
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
  /** 已加载过的状态；null = 还没读过（**不在构造时预取**）。 */
  private state: CcBusState | null = null;

  constructor() {
    this.element = this.build();
    // **刻意不在这里 invoke** 读状态。见文件头「与计划措辞的一处偏离」。
    void this.loadOrigins();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless cc-bus-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "只读查看远端 cc-bus 上登记过的 agent（~/.cc-bus/agents.tsv + spawned.tsv），可给某个 agent 发消息、看它的收件箱。" +
      "「登记」不等于「在线」——名单只说明它曾经登记过，要确认某个还活着请点那一行的「检查」。" +
      "本面板不做后台轮询，只在你点「读取」时发一次请求。";
    root.appendChild(hint);

    const row = document.createElement("div");
    row.className = "settings-row cc-bus-controls";

    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = "机器";
    row.appendChild(label);

    this.originSel = document.createElement("select");
    this.originSel.className = "settings-input cc-bus-origin";
    row.appendChild(this.originSel);

    this.readBtn = document.createElement("button");
    this.readBtn.type = "button";
    this.readBtn.className = "settings-btn settings-btn-secondary cc-bus-read";
    this.readBtn.textContent = "读取";
    this.readBtn.addEventListener("click", () => void this.reload());
    row.appendChild(this.readBtn);

    root.appendChild(row);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "settings-hint cc-bus-status";
    this.statusEl.textContent = "尚未读取。";
    root.appendChild(this.statusEl);

    this.listBox = document.createElement("div");
    this.listBox.className = "cc-bus-list";
    root.appendChild(this.listBox);

    root.appendChild(this.buildSpawnForm());
    return root;
  }

  /** 批二：图形化 spawn。**调收编后的 cc-spawn，不在这里重写起会话。** */
  private buildSpawnForm(): HTMLElement {
    const box = document.createElement("div");
    box.className = "cc-bus-spawn";

    const t = document.createElement("div");
    t.className = "settings-label";
    t.textContent = "派生新 agent";
    box.appendChild(t);

    const h = document.createElement("div");
    h.className = "settings-hint";
    h.textContent =
      "在远端某个目录开一个独立 agent（走远端的 cc-spawn：同目录已有活会话就复用，没有才新建）。" +
      "注意这会起一个真实的 agent 进程并消耗账号额度，所以要点两次确认。";
    box.appendChild(h);

    this.spawnDir = document.createElement("input");
    this.spawnDir.type = "text";
    this.spawnDir.className = "settings-input cc-bus-spawn-dir";
    this.spawnDir.placeholder = "工作目录（远端绝对路径）";
    box.appendChild(this.spawnDir);

    this.spawnTask = document.createElement("input");
    this.spawnTask.type = "text";
    this.spawnTask.className = "settings-input cc-bus-spawn-task";
    this.spawnTask.placeholder = "初始任务（可留空）";
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
    this.spawnBtn.className = "settings-btn settings-btn-secondary cc-bus-spawn-go";
    this.spawnBtn.textContent = "派生";
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
    if (origins.length === 0) {
      const opt = document.createElement("option");
      opt.textContent = "（未配置远端）";
      opt.value = "";
      this.originSel.appendChild(opt);
      this.originSel.disabled = true;
      this.readBtn.disabled = true;
      this.spawnBtn.disabled = true;
      this.statusEl.textContent = "未配置远端。cc-bus 跑在远端机器上，先在「远端」分节配一台。";
      return;
    }
    for (const o of origins) {
      const opt = document.createElement("option");
      opt.value = o;
      opt.textContent = o;
      this.originSel.appendChild(opt);
    }
    // 账号随机器变——换台机器，上一台的账号名多半不适用
    this.originSel.addEventListener("change", () => {
      // S4a：写进共用 store；实际切换由订阅统一处理。
      setCurrentMachine(this.originSel.value || null);
    });
    // S4a：跟随共用 store。本分节只列远端，收到 `null`（本机）就原地不动
    //（cc-bus 跑在远端机器上，本机这一格本来就没有意义）。
    subscribeMachine((origin) => {
      if (origin === null) return;
      if (![...this.originSel.options].some((o) => o.value === origin)) return;
      if (this.originSel.value === origin) return;
      this.originSel.value = origin;
      this.disarmSpawn();
      void this.loadAccounts(origin);
    });
    void this.loadAccounts(this.originSel.value);
  }

  /** 渲染账号下拉。第一项恒为「基座」——**不替用户默认选一个会花钱的号**。 */
  private renderAccountOptions(names: string[]): void {
    this.spawnAcct.replaceChildren();
    const base = document.createElement("option");
    base.value = ""; // 空串 → 后端转发 `--base`（显式不注入）
    base.textContent = "账号：不指定（不注入任何账号）";
    this.spawnAcct.appendChild(base);
    for (const n of names) {
      const o = document.createElement("option");
      o.value = n;
      o.textContent = `账号：${n}`;
      this.spawnAcct.appendChild(o);
    }
  }

  /** 取该远端的可选账号。**拿不到就只留「基座」**——宁可少一个选项，
   *  也不能让用户以为选了某个号而其实没生效。 */
  private async loadAccounts(origin: string): Promise<void> {
    try {
      const st = await fetchAccounts(origin);
      const names = Array.isArray(st?.accounts) ? selectableAccounts(st).map((a) => a.name) : [];
      this.renderAccountOptions(names);
    } catch {
      this.renderAccountOptions([]);
    }
  }

  private async reload(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    this.readBtn.disabled = true;
    this.statusEl.textContent = "读取中…";
    this.listBox.replaceChildren();
    try {
      this.state = await commands.read_cc_bus_state({ origin });
      this.render();
    } catch (e) {
      this.state = null;
      // 失败要说清是哪一步失败，而不是留个空面板让人以为"没有 agent"
      this.statusEl.textContent = `读取失败：${String(e)}`;
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

    const parts = [`登记 ${st.agents.length} 个`];
    if (bothCount > 0) parts.push(`其中 spawn 派生 ${bothCount} 个`);
    if (extra.length > 0) parts.push(`另有 ${extra.length} 个 spawn 过但未登记`);
    if (st.skipped > 0) parts.push(`${st.skipped} 条无法解析（已跳过）`);
    parts.push("「登记」不等于「在线」");
    this.statusEl.textContent = parts.join(" · ");

    this.listBox.replaceChildren();
    if (st.agents.length === 0 && extra.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = "这台机器上没有登记过的 cc-bus agent（或未装 cc-bus）。";
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
          { id: sp.id, pane: "", registered_at: sp.spawned_at },
          true,
          sp.dir,
          false,
        ),
      );
    }
  }

  private buildRow(
    a: CcBusAgent,
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
    bits.push(a.registered_at || "时间未知");
    bits.push(isSpawned ? "cc-spawn 派生" : "自行登记");
    if (!registered) bits.push("未在 agents.tsv 登记");
    meta.textContent = bits.join(" · ");
    row.appendChild(meta);

    // **在线状态默认「未知」**——这是本设计的要点，不是偷懒：名单证明不了在线，
    // 而全量查是 N 次往返。用户想知道哪一个，就点哪一个。
    const stateEl = document.createElement("span");
    stateEl.className = "cc-bus-online cc-bus-online-unknown";
    stateEl.textContent = "在线未知";
    row.appendChild(stateEl);

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "settings-btn settings-btn-secondary cc-bus-check";
    btn.textContent = "检查";
    btn.addEventListener("click", () => void this.checkOne(a.id, stateEl, btn));
    row.appendChild(btn);

    // 批二：收信 + 发消息。两者都是按需一次往返，不订阅、不轮询。
    const detail = document.createElement("div");
    detail.className = "cc-bus-detail";
    row.appendChild(detail);

    const inboxBtn = document.createElement("button");
    inboxBtn.type = "button";
    inboxBtn.className = "settings-btn settings-btn-secondary cc-bus-inbox";
    inboxBtn.textContent = "收件箱";
    inboxBtn.addEventListener("click", () => void this.loadInbox(a.id, detail, inboxBtn));
    row.appendChild(inboxBtn);

    const msg = document.createElement("input");
    msg.type = "text";
    msg.className = "settings-input cc-bus-msg";
    msg.placeholder = "发给它一条消息…";
    row.appendChild(msg);

    const sendBtn = document.createElement("button");
    sendBtn.type = "button";
    sendBtn.className = "settings-btn settings-btn-secondary cc-bus-send";
    sendBtn.textContent = "发送";
    sendBtn.addEventListener("click", () => void this.sendTo(a.id, msg, detail, sendBtn));
    row.appendChild(sendBtn);

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
    stateEl.className = "cc-bus-online cc-bus-online-checking";
    stateEl.textContent = "检查中…";
    try {
      const online = await commands.check_cc_bus_agent_online({ origin, id });
      stateEl.className = `cc-bus-online cc-bus-online-${online ? "yes" : "no"}`;
      stateEl.textContent = online ? "在线" : "不在线";
    } catch (e) {
      // 查失败 ≠ 不在线，必须区分开，否则会把"网络抖了一下"报成"agent 死了"
      stateEl.className = "cc-bus-online cc-bus-online-error";
      stateEl.textContent = `查不到（${String(e)}）`;
    } finally {
      btn.disabled = false;
    }
  }

  private async loadInbox(id: string, box: HTMLElement, btn: HTMLButtonElement): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    btn.disabled = true;
    box.replaceChildren();
    box.textContent = "读取中…";
    try {
      const msgs = await commands.read_cc_bus_inbox({ origin, id });
      box.replaceChildren();
      if (msgs.length === 0) {
        box.textContent = "收件箱是空的。";
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
      box.textContent = `读收件箱失败：${String(e)}`;
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
      await commands.cc_bus_send({ origin, id, text });
      input.value = "";
      box.textContent = "已发送（对方空闲会被敲门，在忙则靠它的 Stop 钩子兜底）。";
    } catch (e) {
      box.textContent = `发送失败：${String(e)}`;
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
    return this.spawnAcct.value ? `账号 ${this.spawnAcct.value}` : "不指定账号";
  }

  private disarmSpawn(): void {
    this.armedFor = null;
    this.spawnBtn.textContent = "派生";
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
      this.spawnOut.textContent = "请先填工作目录。";
      return;
    }
    const fp = this.spawnFingerprint();
    if (this.armedFor !== fp) {
      // 未武装，或武装后参数被改过 → （重新）武装，把要做的事原样说清楚
      const changed = this.armedFor !== null;
      this.armedFor = fp;
      this.spawnBtn.textContent = "确认派生";
      this.spawnOut.textContent =
        (changed ? "参数已改动，请重新确认：" : "") +
        `将在 ${origin} 的 ${dir} 上用${this.acctLabel()}派生一个 ${this.spawnTool.value}——` +
        "这会起一个真实 agent 进程并**消耗额度**。再点一次「确认派生」执行。";
      return;
    }
    this.disarmSpawn();
    this.spawnBtn.disabled = true;
    this.spawnOut.textContent = "派生中…";
    try {
      const out = await commands.cc_bus_spawn({
        origin,
        dir,
        task: this.spawnTask.value,
        tool: this.spawnTool.value,
        // 空串 = 显式基座。后端把它翻成 `--base`，**不存在"什么都不传"这一档**。
        account: this.spawnAcct.value,
      });
      this.spawnOut.textContent = out || "已派生。";
      // 派生完顺手刷新名单——这是**用户动作触发**的一次读，不是后台轮询
      await this.reload();
    } catch (e) {
      this.spawnOut.textContent = `派生失败：${String(e)}`;
    } finally {
      this.spawnBtn.disabled = false;
    }
  }
}
