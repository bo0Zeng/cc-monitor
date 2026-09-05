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
 * # ⚠ 文案纪律（P2s-Y5 → K-P1 KPY4 翻面）：**按状态说实话**，不是「一律不许说」
 *
 * 翻面之前：daemon 是纯 stdio 子进程，monitor 一退读端就断，它 **153 毫秒**内自己退出
 * ⇒ 那时「关掉开关 = 继续在后台跑」是一句做不到的承诺，判据是一张**禁词表**。
 *
 * K-P1 之后它在 Linux 上**真脱离**了 ⇒ 那一支上「继续跑」是真的，
 * 而**没脱离**的那一支上它仍然是假的。⇒ 判据从「禁这几个词」翻成
 * 「**必须出现「无人监护」这一档，且它只在真脱离那一支出现**」。
 *
 * ★★ **本文件不许有自己的那几句文案** —— 它们的唯一一个家是 `../daemon-policy.ts`
 * 的 `EXIT_*` 四条，本文件只调 `describeExitBehavior`。
 * 理由是实测过的失效形态：现行那条判据 `readFileSync` 的**只有一个文件**、只剥整行注释
 * ⇒ 文案搬家 / 拼串 / 进一张 i18n 表就**零命中地绿**（与 `bind_guard` 头注自陈的
 * 「单独存在时是安慰剂」同族）。
 */

import { commands } from "../ipc/commands";

/** 起/停之后轮询状态的次数与间隔 —— 命令是「发出去就返回」的，不轮询看到的是操作前的状态。 */
const SETTLE_TRIES = 30;
const SETTLE_INTERVAL_MS = 100;
import { showActionFailureToast } from "../error-toast";
import {
  HEALTH_UNKNOWN,
  LOCAL_ORIGIN,
  describeDaemonHealth,
  describeExitBehavior,
  initDaemonPolicy,
  killOnExit,
  setKillOnExit,
  type DaemonHealth,
  type DaemonPolicy,
} from "../daemon-policy";

/**
 * 从 `daemon_status` 那份 JSON 里取死亡账读数。**缺席 / 形状不对 ⇒ `null`**。
 *
 * ⚠ 方向与 `detached` 缺席那一格一致：**答不出来就说答不出来**，不替后端补一个
 * 「四个 0」的读数 —— 那会被 `describeDaemonHealth` 说成「一次都没崩过」，
 * 而那两句话（「没崩过」与「没有任何东西在记」）正是 K-P3 §0-1 点名不许混用的。
 *
 * ⚠ 它住在这里而不是 `daemon-policy.ts`：那边是**文案与纯函数**的家，
 * 这一段是**wire 解析**（`daemon_status` 回的是 `Record<string, unknown>`），
 * 与同文件 `st.detached === true` 那一句同层。
 */
function readHealth(raw: unknown): DaemonHealth | null {
  if (typeof raw !== "object" || raw === null) return null;
  const h = raw as Record<string, unknown>;
  const num = (k: string): number | null => (typeof h[k] === "number" ? (h[k] as number) : null);
  const crashed = num("crashed");
  const refused = num("refused");
  const neverStarted = num("neverStarted");
  const misread = num("misread");
  if (crashed === null || refused === null || neverStarted === null || misread === null) {
    return null;
  }
  return {
    crashed,
    refused,
    neverStarted,
    misread,
    last: typeof h.last === "string" ? h.last : null,
  };
}

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
    // ★ 这一句**刻意不再承诺任何一种退出行为** —— 那句话今天是**按机器分档**的
    //   （同一台机上勾没勾、脱没脱离，四种组合各说各的），所以它住在每一行里，
    //   由 `describeExitBehavior` 从唯一的那个家取。
    //   ⚠ 原来这里那句「它仍会在 monitor 退出后很快自行退出」是**实测结论**，
    //   而 K-P1 之后它只对**没脱离**的那一支成立 —— 留在这里就成了一句半假的全称。
    hint.textContent = "每台机各一份。每一行下面写着这台机在 monitor 退出时会发生什么。";
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

    // ★★ `K-P1 KPY4`：**这台机退出时到底会发生什么**，按状态分档如实说。
    // 文案本体不在本文件（见头注）；这里只放它的位置。
    const exit = document.createElement("div");
    exit.className = "daemon-row-exit";
    row.appendChild(exit);

    // ★★ `K-P3b KP3W4`：**那句「无人监护」后面接的那个读数**，另起一行。
    //
    // ⚠ **不许接在上面那一行后面**：`describeExitBehavior` 的四张脸被
    // `daemon-section.vitest.ts` 用**等号**逐格钉着（它自己逐字写着「不是「包含」
    // 而是「等于」——「包含」会放过「在正确那句后面又加了一句错的」」）。
    // 接上去当场红那四格，而那**不是误报**：那两句话说的是两件事。
    const health = document.createElement("div");
    health.className = "daemon-row-health";
    row.appendChild(health);

    return row;
  }

  /**
   * 重画一行的「退出时会发生什么」。
   *
   * `detached` 的真相源是后端 `daemon_status` 的那一格，而它记的是
   * **起它的时候走没走脱离那条路**（不是拿 `channel`/`pid` 反推 —— 那是假信号）。
   * 远端恒 `null` ⇒ 按「没脱离」算，那对远端是**对的**：断流之后那个进程随管道破裂退出。
   */
  private paintExit(origin: string, detached: boolean): void {
    const el = this.rows.get(origin)?.querySelector<HTMLElement>(".daemon-row-exit");
    if (!el) return;
    el.textContent = describeExitBehavior({
      killOnExit: killOnExit(this.policy, origin),
      detached,
    });
    el.dataset.detached = String(detached);
  }

  /**
   * 重画一行的「上次崩没崩」。
   *
   * `health` **缺席**（旧后端，那份 JSON 里没有这一格）⇒ 画 `HEALTH_UNKNOWN`
   * —— 方向与 `detached` 缺席那一格一致：**答不出来就说答不出来**。
   * ⚠ 别在这里退回一个「四个 0」的读数：那会被说成「一次都没崩过」，
   * 而 K-P3 §0-1 逐字点名这两句话「差得很远，不许混用」。
   */
  private paintHealth(origin: string, raw: unknown): void {
    const el = this.rows.get(origin)?.querySelector<HTMLElement>(".daemon-row-health");
    if (!el) return;
    const h = readHealth(raw);
    el.textContent = h === null ? HEALTH_UNKNOWN : describeDaemonHealth(h);
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
      // 勾变了 ⇒ 那句「退出时会发生什么」也变了。**同一拍重画**，
      // 否则屏上那句话描述的是上一次的状态（与 A4 那条「画的是操作前的快照」同族）。
      void this.paintStatus(origin);
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
      // K-P1：`detached` 只认后端给的那一格。**缺席 / null ⇒ 按「没脱离」算**
      // （旧后端没有这一格；远端天然没有）—— 保守方向：不脱离那句话是今天一直在说的那句。
      this.paintExit(origin, st.detached === true);
      // K-P3b：**同一份 JSON**，另一个元素。不新开一次查询，也不接在上面那一行后面。
      this.paintHealth(origin, st.health);
      return on;
    } catch (e) {
      state.textContent = "状态查不到";
      console.warn(`[P2s] ${origin} 状态查询失败：${String(e)}`);
      return null;
    }
  }
}
