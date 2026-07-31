/**
 * F88a（#52）用量视图——全屏 overlay（照 HistoryView/PanoramaView 的 body-level fixed overlay 范式）。
 * 调后端 `aggregate_usage_all`（Channel 流式）拿 per-(会话,模型,天) token 桶，前端按 会话/天/项目/模型
 * 四维 pivot（判定在纯模块 `usage-pivot.ts`）+ 表格展示。
 *
 * **只 token 不 $**（用户 2026-07-17 拍板）。顶部标死「已花费≠配额剩余」硬边界。
 */

import { Channel } from "@tauri-apps/api/core";
import { commands } from "../ipc/commands";
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
import { readRemoteConfig } from "../remote-config";
import { fetchAccounts, currentWorkingAccount } from "../accounts";
import { pickPrimaryOrigin } from "../account-chip";
import {
  fetchAccountUsage,
  OK_USAGE_UNVERIFIED_CAVEAT,
  type AccountUsageOutcome,
} from "../account-usage";

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

  /** S10：plan 窗口块（按需读取；空闲态只有一个按钮）。 */
  private planEl!: HTMLElement;

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

    // ★ S10（settings-ia）：per-account 的 **plan 窗口%** 并进本视图。
    //
    // 为什么并进来：这一页此前只讲「已花费 token」，而「还剩多少」（`/usage` 的 5h/周窗口）
    // 是另一半，用户此前只能在 chip 的菜单里看到。**同一个问题不该有两个地方回答。**
    // chip 上那份**保留** —— 它是常驻状态显示，不是入口。
    //
    // **按需读取，不在 open 时自动探** —— 一次探测要在远端起一个 tmux 会话、跑一次
    // `/usage` 并抓屏（见 `account_usage.rs`）。打开用量页就自动付这个代价是不对的，
    // 也撞 §1-2（不新增轮询）。
    this.planEl = document.createElement("div");
    this.planEl.className = "usage-plan";
    view.appendChild(this.planEl);
    this.renderPlanIdle();

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

  /** S10：空闲态 —— 一句说明 + 一个按钮。**不自动探**（见 build 里的注释）。 */
  private renderPlanIdle(): void {
    this.planEl.replaceChildren();
    const label = document.createElement("div");
    label.className = "usage-plan-label";
    label.textContent = "账号 plan 窗口（还剩多少）";
    this.planEl.appendChild(label);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "settings-btn settings-btn-secondary usage-plan-load";
    btn.textContent = "读取当前账号的 plan 窗口";
    btn.title =
      "会在远端起一个一次性 tmux 会话跑 /usage 并抓屏——所以是点了才读，不自动。";
    btn.addEventListener("click", () => void this.loadPlanWindows(btn));
    this.planEl.appendChild(btn);
  }

  /**
   * S10：读当前账号的 plan 窗口并渲染。
   *
   * **失败一律可见**：解析不出时把原始屏带回来 + 「复制诊断文本」——
   * 这是 F10 建立、主计划要求本轮合并后必须保住的路径。探针的「静止 3s」判据是预算
   * 不是实测值，真 claude 卡顿超时会抓早；那时必须让用户看见「认不出」和原文，
   * **而不是给一个错的数字**。
   */
  private async loadPlanWindows(btn: HTMLButtonElement): Promise<void> {
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = "读取中…";
    try {
      const cfg = await readRemoteConfig();
      const origin = cfg.enabled ? pickPrimaryOrigin(cfg.hosts) : null;
      if (!origin) {
        this.renderPlanMessage("没有启用的远端机器 —— plan 窗口是账号级服务端数据，需要连上远端才能读。");
        return;
      }
      const state = await fetchAccounts(origin, false);
      const cur = currentWorkingAccount(state);
      if (!cur) {
        this.renderPlanMessage("没有解析到当前账号。");
        return;
      }
      const outcome = await fetchAccountUsage(origin, cur.name, cur.configDir ?? null, {
        force: true,
      });
      this.renderPlanOutcome(origin, cur.name, outcome);
    } catch (e) {
      this.renderPlanMessage(`读取失败：${String(e)}`);
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  }

  private renderPlanMessage(text: string): void {
    const msg = document.createElement("div");
    msg.className = "usage-plan-msg";
    msg.textContent = text;
    this.planEl.appendChild(msg);
  }

  private renderPlanOutcome(
    origin: string,
    account: string,
    outcome: AccountUsageOutcome,
  ): void {
    const box = document.createElement("div");
    box.className = "usage-plan-result";
    box.dataset.status = outcome.status;
    const who = document.createElement("div");
    who.className = "usage-plan-who";
    who.textContent = `${origin} · 账号 ${account}`;
    box.appendChild(who);

    if (outcome.status === "ok") {
      for (const b of outcome.buckets) {
        const row = document.createElement("div");
        row.className = "usage-plan-row";
        row.textContent = `${b.label}：${b.usedPercent}% 已用${b.resetIn ? ` · ${b.resetIn}` : ""}`;
        row.title = OK_USAGE_UNVERIFIED_CAVEAT;
        box.appendChild(row);
      }
    } else {
      // ★ 可见失败：说清是什么状态，并把原始屏带回来。
      const msg = document.createElement("div");
      msg.className = "usage-plan-fail";
      msg.textContent =
        outcome.status === "probe-failed"
          ? `探测失败：${outcome.error}`
          : outcome.status === "not-logged-in"
            ? "这个账号还没登录。"
            : outcome.status === "cli-missing"
              ? "远端找不到 claude 命令。"
              : `认不出 /usage 的格式（${outcome.reason}）—— 下面是抓到的原始屏。`;
      box.appendChild(msg);
      const raw = "raw" in outcome ? outcome.raw : undefined;
      if (raw) {
        const pre = document.createElement("textarea");
        pre.className = "settings-input usage-plan-raw";
        pre.readOnly = true;
        pre.rows = 6;
        pre.value = raw;
        box.appendChild(pre);
        const copy = document.createElement("button");
        copy.type = "button";
        copy.className = "settings-btn settings-btn-secondary";
        copy.textContent = "复制诊断文本";
        copy.addEventListener("click", () => void navigator.clipboard?.writeText(raw));
        box.appendChild(copy);
      }
    }
    this.planEl.appendChild(box);
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
    const remoteDone = commands
      .aggregate_remote_usage_all()
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
      await commands.aggregate_usage_all({ onRow: channel });
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
