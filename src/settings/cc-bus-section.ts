// B03 批一：cc-bus 驾驶舱（**只读**）。
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
// `CollapsibleGroup` today 没有展开回调，而为一个消费者去改这个共享 UI 原语，正是
// R12/R15 反复拒绝的"为假想需求建抽象"。这里改成**用户点「读取」才发请求**——一次 30s
// 超时的远端往返，显式触发比"展开即偷偷发"更诚实，也天然满足"启动时不预取"。
import { invoke } from "@tauri-apps/api/core";

interface CcBusAgent {
  id: string;
  pane: string;
  registered_at: string;
}
interface CcBusSpawned {
  id: string;
  dir: string;
  spawned_at: string;
  task: string;
}
interface CcBusState {
  agents: CcBusAgent[];
  spawned: CcBusSpawned[];
  skipped: number;
}

export class CcBusSection {
  readonly element: HTMLElement;
  private originSel!: HTMLSelectElement;
  private readBtn!: HTMLButtonElement;
  private statusEl!: HTMLElement;
  private listBox!: HTMLElement;
  /** 已加载过的状态；null = 还没读过（**不在构造时预取**）。 */
  private state: CcBusState | null = null;

  constructor() {
    this.element = this.build();
    // **刻意不在这里 invoke**。见文件头「与计划措辞的一处偏离」。
    void this.loadOrigins();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless cc-bus-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "只读查看远端 cc-bus 上登记过的 agent（~/.cc-bus/agents.tsv + spawned.tsv）。" +
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

    return root;
  }

  /** 列远端。复用既有 `list_remote_mcp_origins`——它其实是通用的「列远端配置标签」，
   *  名字带 mcp 只是历史；为同一件事再加一条 IPC 是无谓重复。 */
  private async loadOrigins(): Promise<void> {
    let origins: string[] = [];
    try {
      origins = await invoke<string[]>("list_remote_mcp_origins");
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
      this.statusEl.textContent = "未配置远端。cc-bus 跑在远端机器上，先在「远端」分节配一台。";
      return;
    }
    for (const o of origins) {
      const opt = document.createElement("option");
      opt.value = o;
      opt.textContent = o;
      this.originSel.appendChild(opt);
    }
  }

  private async reload(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    this.readBtn.disabled = true;
    this.statusEl.textContent = "读取中…";
    this.listBox.replaceChildren();
    try {
      this.state = await invoke<CcBusState>("read_cc_bus_state", { origin });
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

    // **如实显示解析损耗**——不假装干净。实测 spawned.tsv 53% 的行是坏的。
    const parts = [`登记 ${st.agents.length} 个`, `其中 spawn 的 ${spawnedIds.size} 个`];
    if (st.skipped > 0) parts.push(`${st.skipped} 条无法解析（已跳过）`);
    parts.push("「登记」不等于「在线」");
    this.statusEl.textContent = parts.join(" · ");

    this.listBox.replaceChildren();
    if (st.agents.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = "这台机器上没有登记过的 cc-bus agent（或未装 cc-bus）。";
      this.listBox.appendChild(empty);
      return;
    }
    for (const a of st.agents) {
      this.listBox.appendChild(this.buildRow(a, spawnedIds.has(a.id), dirOf.get(a.id)));
    }
  }

  private buildRow(a: CcBusAgent, isSpawned: boolean, dir: string | undefined): HTMLElement {
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
      const online = await invoke<boolean>("check_cc_bus_agent_online", { origin, id });
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
}
