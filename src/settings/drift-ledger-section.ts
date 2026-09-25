// U-CC1：**数据面漂移记账** —— 「Claude Code 变了，而我们看不懂的那些东西」。
//
// ## 它为什么存在（实测，不是预防性设计）
//
// `src/doc/INVARIANTS.md §18.1`（2026-07-16）记的是「7 个未知记录类型 / 8,774 条」。
// 2026-08-02 重新全量扫本机语料：**10 种 / 27,747 条 / 5.88%**，其中 3 种
// （`started` / `result` / `fork-context-ref`）是那之后新出现的 ——
// **而仓里没有任何东西知道这件事**，是靠人手工扫语料才发现的。
//
// ## 这一页不是「错误列表」
//
// 「看不懂就降级」和「不在白名单里就隐藏」都是**刻意的、正确的**行为
// （对未知记录类型 warn 会刷屏：实测 20,526 条 `mode`）。
// 这一页只回答一个问题：**降级发生了吗、发生在哪。**
// 所以每一行都要说清楚「这么降级之后会怎样」——只给数字不给后果，用户不知道该不该在意。
//
// ## 只读、按需
//
// 一次 `invoke` 读一个进程内的账本快照。**不轮询**（红线，同 `config-surface-section.ts`）。
// 计数是**本进程内**的，重启 cc-monitor 就归零 —— 这一点必须在页面上说，
// 否则用户会把它当成历史统计。
//
// ## 〔ST3〕按机器分
//
// 账本第一层键是 origin（`drift_ledger·rs` 头注）：远端的记录本来就在 monitor 里解析、在这个进程里记账，
// 缺的只是「这一行从哪台来」—— ST3 把它一路带到了写点。⇒ 每台机器子页的「足迹」栏问的就是**这一台**，
// 回包带回它答的是哪台，**相等才画**（回声，同足迹那一格）。本机与远端同一套 DOM、同一条路。
import { commands } from "../ipc/commands";
import { showActionFailureToast } from "../error-toast";
import { withPending } from "./pending";
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import type { DriftEntry } from "../generated/DriftEntry";
import type { DriftFace } from "../generated/DriftFace";
import type { DriftFaceReport } from "../generated/DriftFaceReport";
import type { DriftLedgerReport } from "../generated/DriftLedgerReport";

export type { DriftEntry, DriftFace, DriftFaceReport, DriftLedgerReport };

/** 面 → 给人看的标题。**后端加了新面而这里没跟 ⇒ 显示原始枚举名，不隐藏。** */
export function faceTitle(face: DriftFace): string {
  switch (face) {
    case "unknown_record_type":
      return "看不懂的记录类型";
    case "known_type_parse_failed":
      return "已知类型解析失败";
    case "unknown_session_kind":
      return "未登记的会话 kind";
    case "unknown_backend_token":
      return "远端后端声明了我们不认识的能力";
    default:
      // 后端加第五个面时**不许整页炸掉**，也不许静默吞掉 —— 显示原名。
      return `未命名的面（${String(face)}）`;
  }
}

/** 计数的量纲**每个面不一样**，横向比毫无意义 —— 所以逐面写清楚。 */
export function countUnit(face: DriftFace): string {
  switch (face) {
    case "unknown_record_type":
    case "known_type_parse_failed":
      return "条记录";
    case "unknown_session_kind":
      return "次观测（每次重扫一次，不是会话数）";
    case "unknown_backend_token":
      return "次握手";
    default:
      return "次";
  }
}

/** 一行的可读摘要（供复制诊断文本用，纯函数、可单测）。 */
export function formatEntry(face: DriftFace, e: DriftEntry): string {
  const sample = e.first_sample ? `\n      首见：${e.first_sample}` : "";
  return `  ${e.key} —— ${e.count} ${countUnit(face)}${sample}`;
}

/** 〔ST3〕诊断文本里那台机器怎么称呼（贴进 issue 时要分得清是哪台的账）。 */
function machineName(origin: Origin): string {
  return isLocalOrigin(origin) ? "本机" : origin;
}

/** 整份报告 → 可粘贴的纯文本（提 issue 时直接贴）。〔ST3〕首行带上是哪台机器的。 */
export function formatReport(report: DriftFaceReport[], origin: Origin): string {
  const where = machineName(origin);
  if (report.length === 0) {
    return `数据面漂移记账（${where}）：本次运行期间没有遇到任何看不懂的东西。`;
  }
  const parts = report.map((f) => {
    const head = `${faceTitle(f.face)}（${f.entries.length} 种${f.overflowed ? "，已触顶" : ""}）`;
    const why = `  后果：${f.consequence}`;
    const rows = f.entries.map((e) => formatEntry(f.face, e)).join("\n");
    return `${head}\n${why}\n${rows}`;
  });
  return `数据面漂移记账（${where} · 本进程内，重启归零）\n\n${parts.join("\n\n")}`;
}

/**
 * 〔ST3〕回声：回包里的 `origin` 与所问**相等**才算这台的答复。
 * 读口是 monitor 自己的命令、今天不会答错台 —— 这一格防的是**以后**有人把它改回一本不分机器的账，
 * 界面还照画不误。ST2 那一拍账不分机器，只能在界面上照实说两句「分不开」；ST3 把账分开、那两句删了，
 * 「不拿一台的账冒充另一台」这件事由这一格接着守。
 */
export function answersFor(r: DriftLedgerReport, origin: Origin): boolean {
  return r.origin === origin;
}

export class DriftLedgerSection {
  readonly element: HTMLElement;
  private body!: HTMLElement;
  private copyBtn!: HTMLButtonElement;
  private last: DriftFaceReport[] = [];
  /** 〔ST3〕`last` 是哪台的（复制诊断文本时写进首行）。 */
  private lastOrigin: Origin = getCurrentMachine();
  /** 宿主放过第一发没有（放过之后切机器才由订阅重读）。 */
  private started = false;
  /** 〔ST3〕切机器快过答复时，晚到的那一份不许盖掉当前这台的（同足迹那一格）。 */
  private seq = 0;

  constructor() {
    this.element = this.build();
    // 〔ST2〕跟着「当前在看哪台机器」走（它住机器子页的「足迹」栏，是 per-machine 那一批单例之一）。
    subscribeMachine(() => this.onMachineChanged());
    // 🔴 步 2（`70 §1.3 B` · `§10.4`）：**构造期不再发 I/O。**
    // 这一块原住「改动足迹」页（〔ST2〕今天在每台机器子页的「足迹」栏里，跟 per-machine 那一批一起放），
    // 而落地页是「机器」⇒ 原来那句 `void this.refresh()`
    // 是每次打开设置都白发的一趟 `drift_ledger_report`。
    // `§10.4` 那一行逐字点了它：判据 #3「非落地页零 I/O」今天正是被那三块
    // **外加 `drift-ledger`** 打破的。
  }

  /**
   * 步 2：宿主在「这一页首次可见」时调它。
   * ⚠ **幂等由宿主保证**（`panel.ts::pagesLoaded`）。
   */
  loadNow(): void {
    this.started = true;
    // 〔ST3〕本机、远端同一条路：问的就是当前这台。
    void this.refresh();
  }

  private onMachineChanged(): void {
    // 切了机器 ⇒ 这一块讲的是另一台了 ⇒ 重读（只在已经放过第一发之后；第一发归宿主）。
    if (this.started) void this.refresh();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless drift-ledger-section";
    const host = root;

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    // 〔ST2〕顶层「改动足迹」页删了，这一块搬到机器列表页 ⇒ 不再说「这一页」；
    //   「只读、按需读一次，不后台轮询」是**我们的设计承诺**（`70 §10.1` 差项 3 同一种病）⇒ 拿掉。
    //   「计数在本进程内，重启归零」留着 —— 头注逐字：这一点必须在页面上说，否则会被当成历史统计。
    // 〔ST3〕账按机器分了 ⇒「遇到的」→「从这台机器读到的」（ST2 那两句「今天不分机器」随之删掉）。
    hint.textContent =
      "cc-monitor 这次运行里从这台机器读到的、没认出来的数据（多半是 Claude Code 出了新格式）。" +
      "没认出来的部分照常降级显示，这里把它们列出来。计数只算这次运行，重启 monitor 就归零。";
    host.appendChild(hint);

    const bar = document.createElement("div");
    bar.className = "settings-row";
    const refreshBtn = document.createElement("button");
    refreshBtn.className = "btn";
    refreshBtn.textContent = "重新读取";
    // 步 4·E（`70 §1.3 E`）：读一趟账本是一次真往返，期间按住。
    refreshBtn.addEventListener("click", () =>
      void withPending(refreshBtn, "读取中…", () => this.refresh()),
    );
    bar.appendChild(refreshBtn);

    this.copyBtn = document.createElement("button");
    this.copyBtn.className = "btn";
    this.copyBtn.textContent = "复制诊断文本";
    this.copyBtn.addEventListener("click", () =>
      void withPending(this.copyBtn, "复制中…", () => this.copy()),
    );
    bar.appendChild(this.copyBtn);
    host.appendChild(bar);

    this.body = document.createElement("div");
    this.body.className = "drift-ledger-body";
    host.appendChild(this.body);
    return root;
  }

  private async refresh(): Promise<void> {
    const origin = getCurrentMachine();
    const my = ++this.seq;
    // 换了台 ⇒ 上一台的账先撤下（答复到之前不许顶着别台的数）。
    if (origin !== this.lastOrigin) {
      this.last = [];
      this.lastOrigin = origin;
      this.body.textContent = "";
    }
    try {
      const r = await commands.drift_ledger_report({ origin });
      if (my !== this.seq) return;
      if (!r || !Array.isArray(r.faces)) throw new Error("返回的形状不对（faces 不是数组）");
      // 〔ST3〕回声对不上 ⇒ 当读不到（拿另一台的账冒充这台，比读不到更糟）。
      if (!answersFor(r, origin)) throw new Error(`答的不是这台机器（答的是 ${String(r.origin)}）`);
      this.last = r.faces;
      this.lastOrigin = origin;
    } catch (e) {
      if (my !== this.seq) return;
      // 读不到就说读不到 —— **不显示成「没有漂移」**（那是对用户撒谎）。
      this.last = [];
      this.body.textContent = "";
      const err = document.createElement("div");
      err.className = "settings-hint";
      err.textContent = `读不到漂移账本：${String(e)}（这不等于「没有漂移」）`;
      this.body.appendChild(err);
      return;
    }
    this.render();
  }

  private render(): void {
    this.body.textContent = "";
    if (this.last.length === 0) {
      const ok = document.createElement("div");
      ok.className = "settings-hint";
      ok.textContent =
        "本次运行期间没有遇到看不懂的东西。（不代表历史上没有 —— 计数重启归零。）";
      this.body.appendChild(ok);
      return;
    }
    for (const f of this.last) {
      const box = document.createElement("div");
      box.className = "drift-face";

      const h = document.createElement("div");
      h.className = "drift-face-title";
      h.textContent = `${faceTitle(f.face)}（${f.entries.length} 种${f.overflowed ? "，已触顶" : ""}）`;
      box.appendChild(h);

      const why = document.createElement("div");
      why.className = "settings-hint drift-face-consequence";
      why.textContent = `后果：${f.consequence}`;
      box.appendChild(why);

      for (const e of f.entries) {
        const row = document.createElement("div");
        row.className = "drift-entry";
        const k = document.createElement("span");
        k.className = "drift-entry-key";
        k.textContent = e.key;
        const c = document.createElement("span");
        c.className = "drift-entry-count";
        c.textContent = `${e.count} ${countUnit(f.face)}`;
        row.append(k, c);
        if (e.first_sample) {
          const s = document.createElement("pre");
          s.className = "drift-entry-sample";
          s.textContent = e.first_sample;
          row.appendChild(s);
        }
        box.appendChild(row);
      }
      this.body.appendChild(box);
    }
  }

  private async copy(): Promise<void> {
    const text = formatReport(this.last, this.lastOrigin);
    try {
      await navigator.clipboard.writeText(text);
      this.copyBtn.textContent = "已复制";
      setTimeout(() => (this.copyBtn.textContent = "复制诊断文本"), 1500);
    } catch (e) {
      showActionFailureToast("复制诊断文本失败", String(e));
    }
  }
}
