/**
 * 设置面板「daemon 开关」区（P2s，定框 C8）。
 *
 * 每台机各一行：**本机 + 每台远端**。一行给三件事 —— 状态 · 起/停 · 「monitor 退出时结束它」。
 *
 * # 为什么独立一区，不塞进远端机器卡片
 *
 * 机器卡片那块是 **SSH 配置面**（地址/端口/密钥/指纹）。daemon 开关是**运行期**的事，
 * 而且**本机也有一份** —— 本机没有 SSH 配置卡片可挂。
 * 挂在一起会逼出「本机那行长得和别人不一样」的特例，正是 `C1` 要避免的形状。
 *
 * # ⚠ 文案纪律（P2s-Y5）：不许承诺做不到的事
 *
 * 实测（08-11，P2s §0a）：daemon 是纯 stdio 子进程，monitor 一退读端就断，
 * 它 **153 毫秒**内自己 broken-pipe 退出。
 * ⇒ 关掉这个开关**不等于**「daemon 继续在后台跑」，只等于「monitor 不主动结束它」。
 * 本文件的用户可见文案由 `daemon-section.vitest.ts` 的措辞判据守着：
 * 「后台常驻 / 继续运行 / 一直跑」这类话一个都不许出现。
 * 真要常驻见待决 `U7`（那要先给 daemon 一个监听口）。
 */

import { commands } from "../ipc/commands";

/** 起/停之后轮询状态的次数与间隔 —— 命令是「发出去就返回」的，不轮询看到的是操作前的状态。 */
const SETTLE_TRIES = 30;
const SETTLE_INTERVAL_MS = 100;
import { showActionFailureToast } from "../error-toast";
import {
  LOCAL_ORIGIN,
  initDaemonPolicy,
  killOnExit,
  setKillOnExit,
  type DaemonPolicy,
} from "../daemon-policy";

/** 一台机在这一区里的身份。`origin` 是唯一键，`title` 只给人看。 */
interface Machine {
  origin: string;
  title: string;
}

export class DaemonSection {
  readonly element: HTMLElement;
  private policy: DaemonPolicy = {};
  private rows = new Map<string, HTMLElement>();

  constructor(opts: { headless?: boolean } = {}) {
    this.element = document.createElement("div");
    this.element.className = "settings-section daemon-section";
    if (!opts.headless) {
      const h = document.createElement("h3");
      h.textContent = "daemon 开关";
      this.element.appendChild(h);
    }
    const hint = document.createElement("p");
    hint.className = "settings-hint";
    // ★ 这句话是**实测结论**，不是免责声明：勾掉它 daemon 也不会常驻。
    hint.textContent =
      "每台机各一份。关掉「退出时结束它」只表示 monitor 不主动结束它；它仍会在 monitor 退出后很快自行退出。";
    this.element.appendChild(hint);
    this.list = document.createElement("div");
    this.list.className = "daemon-list";
    this.element.appendChild(this.list);
    void this.refresh();
  }

  private list: HTMLElement;

  /** 重新拉一遍机器清单 + 每台的状态。 */
  async refresh(): Promise<void> {
    try {
      this.policy = await initDaemonPolicy();
    } catch (e) {
      console.warn(`[P2s] 读 daemon 策略失败，按缺省渲染：${String(e)}`);
      this.policy = {};
    }
    const machines = await this.machines();
    this.list.innerHTML = "";
    this.rows.clear();
    for (const m of machines) {
      const row = this.buildRow(m);
      this.rows.set(m.origin, row);
      this.list.appendChild(row);
      void this.paintStatus(m.origin);
    }
  }

  /**
   * 机器清单**问后端要**，不自己算〔D 阶段补审 08-11，A5〕。
   *
   * 原版自己拼（那个 helper 叫 hostKey，**这里刻意不带括号写** —— 下面那条判据扫的就是
   * 「带括号的调用形态」，写全会把这句解释算成一次调用；本轮已被自己的散文绊到四次）：
   * 它是 `h.label.trim() || h.host`，与 Rust 的 origin 分叉四处：
   * ① 前端 `trim()` 而 `origin_label()` 不 trim；② Rust 对**重复 label 做后缀化**
   * （`"pi" → "pi (#2)"`）并按后缀化后的名字注册 ⇒ 第二台起/停恒回「没有这台机的把手」；
   * ③ 前端忽略 `cfg.enabled`，远端总开关关着时一个都没注册而 UI 照样列全部；
   * ④ `register_remote` 只在启动时跑一次，之后新增的机器永远不在注册表里。
   *
   * ⇒ 注册表就是真相源。本机永远在第一行 —— 它不是「另一种机器」，
   * 只是不走 ssh 的那一台（§40 / C1）。
   */
  private async machines(): Promise<Machine[]> {
    try {
      const origins = await commands.daemon_machines();
      return origins.map((origin) => ({
        origin,
        title: origin === LOCAL_ORIGIN ? "本机" : origin,
      }));
    } catch (e) {
      console.warn(`[P2s] 问后端要机器清单失败，只显示本机：${String(e)}`);
      return [{ origin: LOCAL_ORIGIN, title: "本机" }];
    }
  }

  private buildRow(m: Machine): HTMLElement {
    const row = document.createElement("div");
    row.className = "daemon-row";
    row.dataset.origin = m.origin;

    const name = document.createElement("span");
    name.className = "daemon-row-name";
    name.textContent = m.title;
    row.appendChild(name);

    const state = document.createElement("span");
    state.className = "daemon-row-state";
    state.textContent = "查询中…";
    row.appendChild(state);

    const start = document.createElement("button");
    start.textContent = "起";
    start.onclick = () => void this.act(m.origin, "start");
    row.appendChild(start);

    const stop = document.createElement("button");
    stop.textContent = "停";
    stop.onclick = () => void this.act(m.origin, "stop");
    row.appendChild(stop);

    const label = document.createElement("label");
    label.className = "daemon-row-kill";
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = killOnExit(this.policy, m.origin);
    box.onchange = () => void this.toggleKill(m.origin, box);
    label.appendChild(box);
    label.appendChild(document.createTextNode("monitor 退出时结束它"));
    row.appendChild(label);

    return row;
  }

  /**
   * 起 / 停一台机〔D 阶段补审 08-11 重写，A4〕。
   *
   * # 原来错在哪
   *
   * 命令返回后**立刻**重绘 —— 而两个命令都是「发出去就返回」：
   * `daemon_start` 只是 spawn 了监护线程（子进程还没起、hello 更没到）⇒ 屏上写「未连上」；
   * `daemon_stop` 只发 SIGKILL 就返回（消费者要等 EOF 才 `unregister`）
   * ⇒ `channel` 多半仍是 true 而 `pid` 因句柄已被 take 而是 null ⇒ 屏上写「**已连上**（无 pid）」。
   *
   * **即每次操作后看到的都是操作前的状态。** 而且按钮全程不 disable，
   * 直接喂给「双起」那个窗口（补审 A1）。
   *
   * ⇒ 现在：操作期间**禁用本行按钮**，然后**轮询到状态落定**（或超时）再放开。
   * ⚠ 超时不是失败：远端断流后对面进程什么时候退，我们在本机看不见（诚实边界 11c）。
   */
  private async act(origin: string, what: "start" | "stop"): Promise<void> {
    const row = this.rows.get(origin);
    const btns = row ? [...row.querySelectorAll("button")] : [];
    for (const b of btns) b.disabled = true;
    try {
      const msg =
        what === "start"
          ? await commands.daemon_start({ origin })
          : await commands.daemon_stop({ origin });
      console.info(`[P2s] ${origin} ${what}: ${msg}`);
    } catch (e) {
      showActionFailureToast(what === "start" ? "起 daemon 失败" : "停 daemon 失败", String(e));
    }
    await this.settleStatus(origin, what === "start");
    for (const b of btns) b.disabled = false;
  }

  /** 轮询到「通道在不在」与期望一致，或超时。每次都重绘，用户看得到中间态。 */
  private async settleStatus(origin: string, want: boolean): Promise<void> {
    for (let i = 0; i < SETTLE_TRIES; i++) {
      if ((await this.paintStatus(origin)) === want) return;
      await new Promise((r) => setTimeout(r, SETTLE_INTERVAL_MS));
    }
  }

  private async toggleKill(origin: string, box: HTMLInputElement): Promise<void> {
    const want = box.checked;
    try {
      await setKillOnExit(origin, want);
      this.policy[origin] = want;
    } catch (e) {
      // 存不下就**把勾回退**——否则屏上写着 A 而实际是 B，比报错更坏。
      box.checked = !want;
      showActionFailureToast("保存 daemon 策略失败", String(e));
    }
  }

  /** 画一次状态，并把「通道在不在」返回给 `settleStatus` 判落定。查不到回 `null`。 */
  private async paintStatus(origin: string): Promise<boolean | null> {
    const row = this.rows.get(origin);
    if (!row) return null;
    const state = row.querySelector<HTMLElement>(".daemon-row-state");
    if (!state) return null;
    try {
      const st = await commands.daemon_status({ origin });
      const on = st.channel === true;
      const pid = typeof st.pid === "number" ? `（pid ${st.pid}）` : "";
      state.textContent = on ? `已连上${pid}` : "未连上";
      state.dataset.on = String(on);
      return on;
    } catch (e) {
      state.textContent = "状态查不到";
      console.warn(`[P2s] ${origin} 状态查询失败：${String(e)}`);
      return null;
    }
  }
}
