// U-CC1：**数据面漂移记账** —— 「Claude Code 变了，而我们看不懂的那些东西」。
//
// ## 它为什么存在（实测，不是预防性设计）
//
// `doc/INVARIANTS.md §18.1`（2026-07-16）记的是「7 个未知记录类型 / 8,774 条」。
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
import { commands } from "../ipc/commands";
import { showActionFailureToast } from "../error-toast";
import type { DriftEntry } from "../generated/DriftEntry";
import type { DriftFace } from "../generated/DriftFace";
import type { DriftFaceReport } from "../generated/DriftFaceReport";

export type { DriftEntry, DriftFace, DriftFaceReport };

/** 面 → 给人看的标题。**后端加了新面而这里没跟 ⇒ 显示原始枚举名，不隐藏。** */
export function faceTitle(face: DriftFace): string {
  switch (face) {
    case "unknown_record_type":
      return "看不懂的记录类型";
    case "known_type_parse_failed":
      return "已知类型解析失败";
    case "unknown_session_kind":
      return "未登记的会话 kind";
    case "unknown_daemon_token":
      return "远端 daemon 声明了我们不认识的能力";
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
    case "unknown_daemon_token":
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

/** 整份报告 → 可粘贴的纯文本（提 issue 时直接贴）。 */
export function formatReport(report: DriftFaceReport[]): string {
  if (report.length === 0) {
    return "数据面漂移记账：本次运行期间没有遇到任何看不懂的东西。";
  }
  const parts = report.map((f) => {
    const head = `${faceTitle(f.face)}（${f.entries.length} 种${f.overflowed ? "，已触顶" : ""}）`;
    const why = `  后果：${f.consequence}`;
    const rows = f.entries.map((e) => formatEntry(f.face, e)).join("\n");
    return `${head}\n${why}\n${rows}`;
  });
  return `数据面漂移记账（本进程内，重启归零）\n\n${parts.join("\n\n")}`;
}

export class DriftLedgerSection {
  readonly element: HTMLElement;
  private body!: HTMLElement;
  private copyBtn!: HTMLButtonElement;
  private last: DriftFaceReport[] = [];

  constructor() {
    this.element = this.build();
    void this.refresh();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless drift-ledger-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "这一页列出 cc-monitor 在本次运行里遇到的、看不懂的东西。" +
      "看不懂就降级是刻意的（对它们告警会刷屏），但降级本身不该是无声的 —— " +
      "这一页就是那个声音。只读、按需读一次，不后台轮询；计数在本进程内，重启归零。";
    root.appendChild(hint);

    const bar = document.createElement("div");
    bar.className = "settings-row";
    const refreshBtn = document.createElement("button");
    refreshBtn.className = "btn";
    refreshBtn.textContent = "重新读取";
    refreshBtn.addEventListener("click", () => void this.refresh());
    bar.appendChild(refreshBtn);

    this.copyBtn = document.createElement("button");
    this.copyBtn.className = "btn";
    this.copyBtn.textContent = "复制诊断文本";
    this.copyBtn.addEventListener("click", () => void this.copy());
    bar.appendChild(this.copyBtn);
    root.appendChild(bar);

    this.body = document.createElement("div");
    this.body.className = "drift-ledger-body";
    root.appendChild(this.body);
    return root;
  }

  private async refresh(): Promise<void> {
    try {
      this.last = await commands.drift_ledger_report();
    } catch (e) {
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
    const text = formatReport(this.last);
    try {
      await navigator.clipboard.writeText(text);
      this.copyBtn.textContent = "已复制";
      setTimeout(() => (this.copyBtn.textContent = "复制诊断文本"), 1500);
    } catch (e) {
      showActionFailureToast("复制诊断文本失败", String(e));
    }
  }
}
