// cc-bus 驾驶舱：读名单 · 查在线 · 收信 / 发消息 · 广播 · 收掉 · 图形化派生。
//
// 三条约束决定了这个形状：
//  ① 不轮询：状态在跑着 cc-bus 的那台机器的 `~/.cc-bus/`（本机也读，后端同一条命令串、只是不包进 ssh），
//     只在用户点「读取」时按需问一次。本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务。
//  ② 登记 ≠ 在线：`agents.tsv` 只证明登记过；判在线是第二次往返，放在点某一行的「检查」上，不全量查。
//  ③ 脏数据不能把面板搞崩：后端解析器跳过坏行并计数，这里如实显示「N 条无法解析」。
//
// 派生调的是 `cc-spawn`（内部经 `ccm`），这里不碰建会话逻辑、不开标签页（远端 exec，发出即走）。
import { setCurrentMachine, subscribeMachine } from "./machine-context";
import { commands } from "../ipc/commands";
// 查在线 · 发消息 · 收掉 · 派生 · 广播 · 读面都经通道直接问那台机器的后端；形状由 `cc-bus-control.ts` 的解码器严格收。
import { agentOnline, broadcast, killAgent, readInbox, readState, sendMessage, spawnAgent, type BusState } from "../cc-bus-control";
import { saidOfControl } from "../control-said";
// 「可选账号」只有 `selectableAccounts` 一个判据；`fetchAccounts` 带缓存。
import { selectableAccounts } from "../accounts";
import { fetchAccounts } from "../account-reads";
// 本机 ＝ `LOCAL_ORIGIN`（与 Rust 侧逐字相同，跨语言钉住）。
import { LOCAL_ORIGIN } from "../backend-policy";

import { confirmDialog } from "../kit/dialog";
import { copyText } from "../copy-table";
import { DEFAULT_AGENT, listAgents } from "../agent-profile";
import { detailOf, sayWithDetail } from "../kit/detail";

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
  /** 派生的两步确认：记住确认的是哪一组参数（不是一个布尔），点第二次时比对，不一致就重新武装 —— 否则改了参数再点会用新值执行。 */
  private armedFor: string | null = null;
  /** 「收掉」的两步确认（与派生的 `armedFor` 各自武装）。 */
  private killArmedFor: string | null = null;
  /** 武装中的那颗按钮本身：只记 id 复位不了它。 */
  private killArmedBtn: HTMLButtonElement | null = null;
  private broadcastInput!: HTMLInputElement;
  private broadcastBtn!: HTMLButtonElement;
  /** 已读过的状态；null ＝ 还没读过（不在构造时预取）。 */
  private state: BusState | null = null;

  constructor() {
    this.element = this.build();
    // 不在这里读 cc-bus 状态：用户点「读取」才发（约束 ①）。
    void this.loadOrigins();
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

    // 广播：一次发给这台总线上的全部收件人。
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

    // 装 cc-bus 不在这里：它是扩展页里的一行（装到哪台、钩子装了没都在那儿）。
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

  /** 图形化派生：调 `cc-spawn`，不在这里重写起会话。 */
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
    // 只给后端注册表里有的那几家（生成物 `AGENT_PROFILE_TABLE`），起手选默认那一家。
    for (const v of listAgents()) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = v;
      this.spawnTool.appendChild(o);
    }
    this.spawnTool.value = DEFAULT_AGENT;
    box.appendChild(this.spawnTool);

    // 必须让用户表态用哪个账号；默认项是「基座」，不替用户默认花掉某个号的额度。
    this.spawnAcct = document.createElement("select");
    this.spawnAcct.className = "settings-input cc-bus-spawn-acct";
    box.appendChild(this.spawnAcct);
    this.renderAccountOptions([]);

    // 任一参数变化立刻解除武装（文案承诺了「参数改动要重新确认」）。
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

  /** 列远端（`list_remote_mcp_origins`：通用的「列远端配置标签」，名字带 mcp 是历史）。 */
  private async loadOrigins(): Promise<void> {
    let origins: string[] = [];
    try {
      // 不只防 reject：也可能 resolve 成非数组，不校验形状的话下一行就抛。
      const got = await commands.list_remote_mcp_origins();
      if (Array.isArray(got)) origins = got;
    } catch {
      /* 拿不到就当没有远端，不影响面板其余部分 */
    }
    this.originSel.replaceChildren();
    // 本机永远在列表里（本机的 `~/.cc-bus/` 就在本地）。
    for (const o of origins) {
      const opt = document.createElement("option");
      opt.value = o;
      opt.textContent = o;
      this.originSel.appendChild(opt);
    }
    // 本机追加在末尾：`select` 默认第一项，配了远端的人打开面板照旧先看远端。
    {
      const opt = document.createElement("option");
      opt.value = LOCAL_ORIGIN;
      opt.textContent = copyText("ccBus.machine.local");
      this.originSel.appendChild(opt);
    }
    if (origins.length === 0) {
      this.statusEl.textContent = copyText("ccBus.machine.noRemote");
    }
    // 换机器 ⇒ 写进共用 store、换账号下拉（订阅看到选择器已是那台会早退，所以这里自己换）。
    this.originSel.addEventListener("change", () => {
      const v = this.originSel.value;
      setCurrentMachine(v);
      this.syncLocalAffordances();
      void this.loadAccounts(v);
    });
    // 跟随共用 store 切到那一台（含本机）。
    subscribeMachine((origin) => {
      const want = origin;
      if (![...this.originSel.options].some((o) => o.value === want)) return;
      if (this.originSel.value === want) return;
      this.originSel.value = want;
      this.disarmSpawn();
      this.disarmKill(); // 切了机器，上一台那颗武装中的「收掉」失效
      this.syncLocalAffordances();
      void this.loadAccounts(want);
    });
    this.syncLocalAffordances();
    // 本机也拉账号列表：`fetchAccounts` 认 `LOCAL_ORIGIN`，这里不另写本机分支。
    void this.loadAccounts(this.originSel.value);
  }

  /** 渲染账号下拉。第一项恒为「基座」：不替用户默认选一个会花钱的号。 */
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

  /** 派生按钮今天哪台都能用（本机与远端同一条后端原语）；哪天真有「这台机器做不了」的，写在这里。 */
  private syncLocalAffordances(): void {
    this.spawnBtn.disabled = false;
    this.spawnBtn.title = "";
  }

  /** 这台机器的可选账号；拿不到就只留「基座」（宁可少一个选项，也不让人以为选了某个号）。 */
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
      // 失败说清是哪一步，别留个空面板让人以为「没有 agent」
      sayWithDetail(this.statusEl, copyText("ccBus.reload.readFailed", { e: saidOfControl(e) }), detailOf(e));
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

    // 只在 spawned 里的条目也渲染（两份名单交集可能很小）：它们有目录和时间，藏起来会让头条的数与可见行数对不上、
    // 还让人以为看全了。
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
    // 未登记的派生记录：明确标注它没在总线上
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
    row.dataset.busAgent = a.id; // 靠 dataset 认身份，不靠 textContent

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

    // 在线状态默认「未知」：名单证明不了在线，全量查是 N 次往返；想知道哪一个就点哪一个。
    const stateEl = document.createElement("span");
    // 状态用 `data-state`，不用类名。
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

    // 收信 ＋ 发消息：都是按需一次往返，不订阅、不轮询。
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

    // 收掉这个 agent：杀会话 ＋ 进程树、不可撤销 ⇒ 两步确认，第二步的文案带上 id（回显真名，别杀错）。
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
      // 查失败 ≠ 不在线：要分开，否则网络抖一下就报成 agent 死了
      stateEl.dataset.state = "error";
      sayWithDetail(stateEl, copyText("ccBus.check.failed", { e: saidOfControl(e) }), detailOf(e));
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
      // 只渲染尾部若干条（后端已限 200 行，这里再收一次）
      for (const m of msgs.slice(-20)) {
        const line = document.createElement("div");
        line.className = "cc-bus-msg-line";
        const none = copyText("ccBus.inbox.missing");
        const [ts, from] = [m.ts || none, m.from || none];
        line.textContent = m.class
          ? copyText("ccBus.inbox.lineClass", { ts, from, cls: m.class, text: m.text })
          : copyText("ccBus.inbox.line", { ts, from, text: m.text });
        box.appendChild(line);
      }
    } catch (e) {
      sayWithDetail(box, copyText("ccBus.inbox.failed", { e: saidOfControl(e) }), detailOf(e));
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
      // 那一句由成品的三态说（在线 / 不在线 / 名字没登记过 / 问不到）。
      box.textContent = await sendMessage(origin, id, text);
      input.value = "";
    } catch (e) {
      sayWithDetail(box, copyText("ccBus.send.failed", { e: saidOfControl(e) }), detailOf(e));
    } finally {
      btn.disabled = false;
    }
  }

  /** 当前表单参数的指纹：确认的必须正好是执行的那一组。 */
  private spawnFingerprint(): string {
    return JSON.stringify([
      this.originSel.value,
      this.spawnDir.value.trim(),
      this.spawnTask.value,
      this.spawnTool.value,
      this.spawnAcct.value,
    ]);
  }

  /** 确认文案里点名账号：不说清是哪个号的额度等于没说。 */
  private acctLabel(): string {
    return this.spawnAcct.value ? copyText("ccBus.acctLabel.named", { value: this.spawnAcct.value }) : copyText("ccBus.acctLabel.none");
  }

  /** 收掉一个 agent：第一次点只武装、一条命令都不发（判据钉的是这个），第二次才真发。 */
  private async killOne(id: string, detail: HTMLElement, btn: HTMLButtonElement): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    if (this.killArmedFor !== id) {
      // 先把上一颗复位：只记 id 的话，武装 A 再点 B，A 那颗还显示「确认收掉 A」，两颗都像武装着。
      this.disarmKill();
      this.killArmedFor = id;
      this.killArmedBtn = btn;
      btn.textContent = copyText("ccBus.kill.confirm", { id });
      return;
    }
    this.disarmKill();
    btn.disabled = true;
    try {
      detail.textContent = await killAgent(origin, id);
      await this.reload();
    } catch (e) {
      sayWithDetail(detail, copyText("ccBus.kill.failed", { e: saidOfControl(e) }), detailOf(e));
      btn.disabled = false;
    }
  }

  /** 广播：确认必须带数字（会有多少人收到），不带数字的「确定吗」等于没问。 */
  private async doBroadcast(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    const text = this.broadcastInput.value.trim();
    if (!text) return;
    const n = this.state?.agents.length ?? 0;
    const confirm = {
      title: copyText("ccBus.broadcast.title", { n }),
      action: copyText("ccBus.broadcast.action"),
      body: copyText("ccBus.broadcast.confirm", { n, text }),
    };
    if (!(await confirmDialog(confirm))) return;
    this.broadcastBtn.disabled = true;
    try {
      this.statusEl.textContent = await broadcast(origin, text);
      this.broadcastInput.value = "";
    } catch (e) {
      sayWithDetail(this.statusEl, copyText("ccBus.broadcast.failed", { e: saidOfControl(e) }), detailOf(e));
    } finally {
      this.broadcastBtn.disabled = false;
    }
  }

  /** 把武装中的「收掉」复位：切机器 / 重载 / 武装另一颗时都要调。 */
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
      // 先解除武装再返回：否则「武装 → 清空目录 → 点 → 填新目录 → 点」一次点击就起了 agent、没见过确认文案。
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
        // 空串 ＝ 显式基座（发 `base:true`）；没有「什么都不传」这一档。
        account: this.spawnAcct.value,
      });
      // 派生完刷新名单（用户动作触发的一次读，不是轮询）
      await this.reload();
    } catch (e) {
      sayWithDetail(this.spawnOut, copyText("ccBus.spawn.failed", { e: saidOfControl(e) }), detailOf(e));
    } finally {
      this.spawnBtn.disabled = false;
    }
  }
}
