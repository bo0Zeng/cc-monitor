// cc-bus 驾驶舱：读名单 · 查在线 · 收信 / 发消息。起会话走「开会话」，这里不起、不收会话。
//
// 三条约束决定了这个形状：
//  ① 不轮询：状态在跑着 cc-bus 的那台机器的 `~/.cc-bus/`（本机也读，后端同一条命令串、只是不包进 ssh），
//     只在用户点「读取」时按需问一次。本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务。
//  ② 登记 ≠ 在线：`agents.tsv` 只证明登记过；判在线是第二次往返，放在点某一行的「检查」上，不全量查。
//  ③ 脏数据不能把面板搞崩：后端解析器跳过坏行并计数，这里如实显示「N 条无法解析」。
import { setCurrentMachine, subscribeMachine } from "../machine-context";
import { remoteNames } from "../remote-config";
// 查在线 · 发消息 · 读面都经通道直接问那台机器的后端；形状由 `cc-bus-control.ts` 的解码器严格收。
import { agentOnline, readInbox, readRoster, sendMessage, type BusList } from "../cc-bus-control";
// 本机 ＝ `LOCAL_ORIGIN`（与 Rust 侧逐字相同，跨语言钉住）。
import { LOCAL_ORIGIN } from "../backend-policy";

import { copyText } from "../copy-table";
import { sayFailure } from "../kit/detail";
import { morph } from "../kit/morph";

export class CcBusSection {
  readonly element: HTMLElement;
  private originSel!: HTMLSelectElement;
  private readBtn!: HTMLButtonElement;
  private statusEl!: HTMLElement;
  private listBox!: HTMLElement;
  /** 已读过的状态；null ＝ 还没读过（不在构造时预取）。 */
  private state: BusList | null = null;

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
    return root;
  }

  /** 列远端（`remoteNames`：壳注册表里此刻要连的那几台）。 */
  private async loadOrigins(): Promise<void> {
    // 拿不到就当没有远端，不影响面板其余部分。
    const origins = await remoteNames().catch(() => [] as string[]);
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
    // 换机器 ⇒ 写进共用 store（订阅看到选择器已是那台会早退）。
    this.originSel.addEventListener("change", () => setCurrentMachine(this.originSel.value));
    // 跟随共用 store 切到那一台（含本机）。
    subscribeMachine((origin) => {
      const want = origin;
      if (![...this.originSel.options].some((o) => o.value === want)) return;
      if (this.originSel.value === want) return;
      this.originSel.value = want;
    });
  }

  private async reload(): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    this.readBtn.disabled = true;
    this.statusEl.textContent = copyText("kit.state.reading");
    try {
      this.state = await readRoster(origin);
      this.render();
    } catch (e) {
      this.state = null;
      morph(this.listBox, []);
      // 失败说清是哪一步，别留个空面板让人以为「没有 agent」
      sayFailure(this.statusEl, copyText("ccBus.reload.readFailed"), e);
    } finally {
      this.readBtn.disabled = false;
    }
  }

  private render(): void {
    const st = this.state;
    if (!st) return;
    this.statusEl.textContent = [
      copyText("ccBus.summary.registered", { n: st.agents.length }),
      copyText("ccBus.summary.notOnline"),
    ].join(copyText("ccBus.summary.sep"));
    if (st.agents.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = copyText("ccBus.render.empty");
      morph(this.listBox, [empty]);
      return;
    }
    // 重读时各行整份新画，正打着的那句消息（焦点 · 字）由 kit morph 搬过去。
    morph(this.listBox, st.agents.map((a) => this.buildRow(a)));
  }

  private buildRow(a: { id: string; target: string }): HTMLElement {
    const row = document.createElement("div");
    row.className = "cc-bus-row";
    row.dataset.busAgent = a.id; // 靠 dataset 认身份，不靠 textContent

    const idEl = document.createElement("span");
    idEl.className = "cc-bus-id";
    idEl.textContent = a.id;
    row.appendChild(idEl);

    const meta = document.createElement("span");
    meta.className = "cc-bus-meta";
    meta.textContent = a.target;
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
    msg.dataset.key = `msg:${this.originSel.value}:${a.id}`; // 重画时认同一格（kit morph）
    msg.placeholder = copyText("ccBus.row.messageHint");
    row.appendChild(msg);

    const sendBtn = document.createElement("button");
    sendBtn.type = "button";
    sendBtn.className = "settings-btn cc-bus-send";
    sendBtn.textContent = copyText("ccBus.row.send");
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
    stateEl.dataset.state = "checking";
    stateEl.textContent = copyText("ccBus.check.running");
    try {
      const online = await agentOnline(origin, id);
      stateEl.dataset.state = online ? "yes" : "no";
      stateEl.textContent = online ? copyText("ccBus.check.online") : copyText("ccBus.check.offline");
    } catch (e) {
      // 查失败 ≠ 不在线：要分开，否则网络抖一下就报成 agent 死了
      stateEl.dataset.state = "error";
      sayFailure(stateEl, copyText("ccBus.check.failed"), e);
    } finally {
      btn.disabled = false;
    }
  }

  private async loadInbox(id: string, box: HTMLElement, btn: HTMLButtonElement): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    btn.disabled = true;
    box.replaceChildren();
    box.textContent = copyText("kit.state.reading");
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
      sayFailure(box, copyText("ccBus.inbox.failed"), e);
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
      sayFailure(box, copyText("ccBus.send.failed"), e);
    } finally {
      btn.disabled = false;
    }
  }
}
