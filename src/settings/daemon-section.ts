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
import { showActionFailureToast } from "../error-toast";
import { readRemoteConfig, hostKey } from "../remote-config";
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

  /** 本机永远在第一行 —— 它不是「另一种机器」，只是不走 ssh 的那一台（§40 / C1）。 */
  private async machines(): Promise<Machine[]> {
    const out: Machine[] = [{ origin: LOCAL_ORIGIN, title: "本机" }];
    try {
      const cfg = await readRemoteConfig();
      for (const h of cfg.hosts) {
        const origin = hostKey(h);
        if (origin) out.push({ origin, title: origin });
      }
    } catch (e) {
      console.warn(`[P2s] 读远端清单失败，只显示本机：${String(e)}`);
    }
    return out;
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

  private async act(origin: string, what: "start" | "stop"): Promise<void> {
    try {
      const msg =
        what === "start"
          ? await commands.daemon_start({ origin })
          : await commands.daemon_stop({ origin });
      console.info(`[P2s] ${origin} ${what}: ${msg}`);
    } catch (e) {
      showActionFailureToast(what === "start" ? "起 daemon 失败" : "停 daemon 失败", String(e));
    }
    await this.paintStatus(origin);
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

  private async paintStatus(origin: string): Promise<void> {
    const row = this.rows.get(origin);
    if (!row) return;
    const state = row.querySelector<HTMLElement>(".daemon-row-state");
    if (!state) return;
    try {
      const st = await commands.daemon_status({ origin });
      const on = st.channel === true;
      const pid = typeof st.pid === "number" ? `（pid ${st.pid}）` : "";
      state.textContent = on ? `已连上${pid}` : "未连上";
      state.dataset.on = String(on);
    } catch (e) {
      state.textContent = "状态查不到";
      console.warn(`[P2s] ${origin} 状态查询失败：${String(e)}`);
    }
  }
}
