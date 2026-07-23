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
  sortPivotRows,
  defaultSortForDim,
  sumAll,
  totalTokens,
  type SessionUsageRow,
  type SortCol,
  type SortDir,
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
  /** #67:当前排序列 + 方向。初值**由默认维度推导**(按天 → 日期降序,最近在上),切维度时重置为该维度默认。 */
  private sort: { col: SortCol; dir: SortDir } = defaultSortForDim(this.dim);
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
        // #67 审计:点**已激活**的维度(很多人拿当前 tab 当"刷新"用)不该静默抹掉用户排好的列 → 早退。
        if (this.dim === d.id) return;
        this.dim = d.id;
        // #67:切维度 → 排序重置为该维度默认(按天=日期降序;其余=等效∑降序)。否则"选了按天"却仍按
        // 上一个维度的列排,正是用户看到的"按天日期乱跳"。(跨维度重置是对的:key 列在各维度语义不同。)
        this.sort = defaultSortForDim(d.id);
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
    // #67 审计:流式扫描期间每帧都整表 replaceChildren 重建,会把滚动位置弹回顶部;表头可点之后用户
    // 更可能在扫描窗口里操作本表 → 存/还原 scrollTop(照 session-viewer 的既有范式;此处无高度突变,
    // 直接回写即可)。
    const prevScroll = this.listEl.scrollTop;
    // dim 按钮 active 态
    for (const btn of this.root.querySelectorAll<HTMLElement>(".usage-dim-btn")) {
      btn.classList.toggle("active", btn.dataset.dim === this.dim);
    }
    // #67:pivot 之后按**当前排序列/方向**排(pivotUsage 内部的等效∑降序只是缺省底座)。
    const pivot = sortPivotRows(pivotUsage(this.rows, this.dim), this.sort.col, this.sort.dir);
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
    // #67:每列都可点排序。原来把「▼」写死在「等效∑」上——无论切到哪个维度、都只按那列降序,
    // 于是「按天」看到的是日期乱跳;现在指示符只挂**当前排序列**,且点表头即可改列/切升降序。
    const headers: { label: string; col: SortCol; title?: string }[] = [
      { label: DIMS.find((d) => d.id === this.dim)?.label ?? "", col: "key" },
      { label: "input", col: "input" },
      { label: "cache 写", col: "cacheCreation" },
      { label: "cache 读", col: "cacheRead" },
      { label: "output", col: "output" },
      { label: "合计", col: "total" },
      {
        label: "等效∑",
        // F88d：相对成本折算，非绝对 $ + 跨模型告警。
        title:
          "等效 input token = Σ(各档 × 相对系数：input1 / cache写1.25 / cache读0.1 / output5)。反映相对成本（哪儿最烧），非绝对 $。" +
          "⚠ 系数按 token 档位、与模型无关：跨模型不可直接比（Haiku/Opus/Fable 每 token 单价差 5–10×），同一格混多模型时本列会低估贵模型占比。",
        col: "equiv",
      },
      { label: "回复", col: "msgs" },
    ];
    for (const h of headers) {
      const th = document.createElement("th");
      const active = this.sort.col === h.col;
      th.textContent = h.label + (active ? (this.sort.dir === "desc" ? " ▼" : " ▲") : "");
      th.className = active ? "usage-th-sortable active" : "usage-th-sortable";
      th.title = (h.title ? h.title + "\n\n" : "") + "点击按本列排序，再点切换升/降序。";
      // 键盘可达 + 屏幕阅读器(仓内既有做法:agents-panel/branch-fold/collapsible-group 的 role=button+tabindex+Enter/Space)。
      th.setAttribute("role", "button");
      th.tabIndex = 0;
      th.setAttribute("aria-sort", active ? (this.sort.dir === "desc" ? "descending" : "ascending") : "none");
      const applySort = (): void => {
        // #67 审计:**实时读 `this.sort`**、不用渲染时捕获的 `active`——否则将来出现"改了 sort 却不重渲"
        // 的写点(排序偏好恢复/快捷键排序)时,会翻错列的 dir、表现为"点了没反应"。
        this.sort =
          this.sort.col === h.col
            ? { col: h.col, dir: this.sort.dir === "desc" ? "asc" : "desc" }
            : { col: h.col, dir: "desc" }; // 换列 → 从降序起步(数值"大的在上"、日期"新的在上"更常用)
        this.renderList();
      };
      th.addEventListener("click", applySort);
      th.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          applySort();
        }
      });
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
    // 审计 建议:首格补 `usage-key` 类,否则「合计」二字右对齐、与上方整列左对齐的 key 不齐(既存瑕疵)。
    ftd.className = "usage-key";
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
    this.listEl.scrollTop = prevScroll; // 见顶部 prevScroll 注释(流式重渲不弹回顶部)
  }
}
