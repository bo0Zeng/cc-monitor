/**
 * F88a（#52）用量视图——全屏 overlay（照 HistoryView/PanoramaView 的 body-level fixed overlay 范式）。
 * 调后端 `aggregate_usage_all`（Channel 流式）拿 per-(会话,模型,天) token 桶，前端按 会话/天/项目/模型
 * 四维 pivot（判定在纯模块 `usage-pivot.ts`）+ 表格展示。
 *
 * **只 token 不 $**（用户 2026-07-17 拍板）。顶部标死「已花费≠配额剩余」硬边界。
 */

import { invoke, Channel } from "@tauri-apps/api/core";
import { dispatcher } from "../keybindings/registry";
import { showActionFailureToast } from "../error-toast";
import {
  pivotUsage,
  sumAll,
  totalTokens,
  type SessionUsageRow,
  type UsageDim,
  type UsageTotals,
} from "./usage-pivot";
import { equivalentInputTokens } from "./pricing";

const DIMS: { id: UsageDim; label: string }[] = [
  { id: "day", label: "按天" },
  { id: "project", label: "按项目" },
  { id: "model", label: "按模型" },
  { id: "session", label: "按会话" },
];

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

export class UsageView {
  private root: HTMLElement;
  private listEl!: HTMLElement;
  private statusEl!: HTMLElement;
  private isOpen = false;
  private rows: SessionUsageRow[] = [];
  private dim: UsageDim = "day";
  /** 每次 open 自增，防旧 open 的流式结果污染新 open。 */
  private loadSeq = 0;

  constructor() {
    this.root = this.build();
  }

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "usage-view";

    const bar = document.createElement("div");
    bar.className = "usage-bar";
    const back = document.createElement("button");
    back.type = "button";
    back.className = "usage-back";
    back.textContent = "← 返回";
    back.addEventListener("click", () => this.close());
    bar.appendChild(back);

    for (const d of DIMS) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "usage-dim-btn";
      b.dataset.dim = d.id;
      b.textContent = d.label;
      b.addEventListener("click", () => {
        this.dim = d.id;
        this.renderList();
      });
      bar.appendChild(b);
    }
    view.appendChild(bar);

    // 硬边界标注——已花费 token，非配额剩余。
    const note = document.createElement("div");
    note.className = "usage-note";
    note.textContent =
      "以下为已花费用量（token 数），非配额剩余。「还剩多少」（/usage 的 5h/周窗口）是账号级服务端数据，本地会话文件推不出。「按天」为 UTC 日期。";
    view.appendChild(note);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "usage-status";
    view.appendChild(this.statusEl);

    this.listEl = document.createElement("div");
    this.listEl.className = "usage-list";
    view.appendChild(this.listEl);

    return view;
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  handleEsc(): void {
    this.close();
  }

  async open(): Promise<void> {
    if (this.isOpen) return;
    document.body.appendChild(this.root);
    this.isOpen = true;
    dispatcher.pushOverlay(this);
    await this.refresh();
  }

  close(): void {
    if (!this.isOpen) return;
    this.loadSeq++; // 让 pending 扫描的 onmessage/结果/失败 toast 被 seq 守卫丢弃（关闭后不再渲染/弹窗）
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  private async refresh(): Promise<void> {
    const seq = ++this.loadSeq;
    this.rows = [];
    this.statusEl.textContent = "扫描用量…";
    this.listEl.replaceChildren();
    const channel = new Channel<SessionUsageRow>();
    let rafPending = false;
    channel.onmessage = (row) => {
      if (seq !== this.loadSeq) return; // 被新 open 抢占
      this.rows.push(row);
      if (rafPending) return;
      rafPending = true;
      requestAnimationFrame(() => {
        rafPending = false;
        if (seq === this.loadSeq && this.isOpen) this.renderList();
      });
    };
    // F88a-remote：远端 daemon 服务端聚合 fan-out（非流式，一次返 Vec），与本地流式并发；
    // 各带 origin=host → pivot 按 [origin] 分桶（usage-pivot 已 origin-aware、零改）。远端失败不拖垮本地。
    const remoteDone = invoke<SessionUsageRow[]>("aggregate_remote_usage_all")
      .then((rows) => {
        if (seq !== this.loadSeq) return; // 被新 open/close 抢占
        if (rows.length) {
          this.rows.push(...rows);
          if (this.isOpen) this.renderList();
        }
      })
      .catch((e) => {
        // 远端用量失败只 warn、不弹 toast、不阻本地（daemonless/旧 daemon/断线均属正常降级）。
        if (seq === this.loadSeq) console.warn("远端用量聚合失败（跳过）:", e);
      });
    try {
      await invoke("aggregate_usage_all", { onRow: channel });
      await remoteDone;
      if (seq === this.loadSeq && this.isOpen) this.renderList();
    } catch (e) {
      if (seq !== this.loadSeq) return; // 被新 open/close 抢占 → 静默（关闭后不弹 toast）
      this.statusEl.textContent = `扫描失败：${String(e)}`;
      showActionFailureToast("用量扫描失败", String(e));
    }
  }

  private renderList(): void {
    // dim 按钮 active 态
    for (const btn of this.root.querySelectorAll<HTMLElement>(".usage-dim-btn")) {
      btn.classList.toggle("active", btn.dataset.dim === this.dim);
    }
    const pivot = pivotUsage(this.rows, this.dim);
    const grand = sumAll(this.rows);
    this.statusEl.textContent =
      this.rows.length === 0
        ? "尚无用量记录（会话里没有带 usage 的 assistant 记录）。"
        : `${this.rows.length} 个会话 · 合计 ${fmt(totalTokens(grand))} tokens（${fmt(grand.msgs)} 条回复）`;

    this.listEl.replaceChildren();
    if (pivot.length === 0) return;

    const table = document.createElement("table");
    table.className = "usage-table";
    const head = document.createElement("tr");
    for (const h of [
      { t: DIMS.find((d) => d.id === this.dim)?.label ?? "" },
      { t: "input" },
      { t: "cache 写" },
      { t: "cache 读" },
      { t: "output" },
      { t: "合计" },
      {
        t: "等效∑",
        // F88d：相对成本折算，非绝对 $。
        title:
          "等效 input token = Σ(各档 × 相对系数：input1 / cache写1.25 / cache读0.1 / output5)。反映相对成本（哪儿最烧），非绝对 $。",
      },
      { t: "回复" },
    ]) {
      const th = document.createElement("th");
      th.textContent = h.t;
      if (h.title) th.title = h.title;
      head.appendChild(th);
    }
    table.appendChild(head);

    const cells = (t: UsageTotals): number[] => [
      t.input,
      t.cacheCreation,
      t.cacheRead,
      t.output,
      totalTokens(t),
      equivalentInputTokens(t), // F88d：等效 input token（相对成本折算）
      t.msgs,
    ];
    for (const r of pivot) {
      const tr = document.createElement("tr");
      const keyTd = document.createElement("td");
      keyTd.className = "usage-key";
      keyTd.textContent = r.label;
      keyTd.title = r.label;
      tr.appendChild(keyTd);
      for (const n of cells(r.totals)) {
        const td = document.createElement("td");
        td.className = "usage-num";
        td.textContent = fmt(n);
        tr.appendChild(td);
      }
      table.appendChild(tr);
    }
    // 合计行
    const foot = document.createElement("tr");
    foot.className = "usage-total-row";
    const ftd = document.createElement("td");
    ftd.textContent = "合计";
    foot.appendChild(ftd);
    for (const n of cells(grand)) {
      const td = document.createElement("td");
      td.className = "usage-num";
      td.textContent = fmt(n);
      foot.appendChild(td);
    }
    table.appendChild(foot);

    this.listEl.appendChild(table);
  }
}
