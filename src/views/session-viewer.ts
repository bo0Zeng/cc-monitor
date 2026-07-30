/**
 * 只读历史会话查看器。
 *
 * 用法：HistoryView 点击一条历史条目时实例化这个组件，给定 jsonl_path 加载并渲染。
 * 复用 cards/renderMessage 与实时 Tab 同一套渲染逻辑（user 气泡 / assistant full-width /
 * tool_use 折叠条 / tool_result 合并等），只是数据源换成"一次性 IPC 读全文件"。
 *
 * 与 TabManager 的关系：完全独立。这里不创建 Tab、不影响实时流、不调 event_replay。
 * 关闭查看器后状态彻底释放。
 */

import { Channel } from "@tauri-apps/api/core";
import { commands } from "../ipc/commands";
import { MessageStream } from "../stream";
import {
  type JsonlRecord,
  type RenderContext,
  reconcilePendingToolResults,
} from "../cards";
import { BranchFolder } from "../branch-fold";
import { type BranchRecord } from "../branching";
import { RecordTimeline } from "../record-timeline";
import {
  renderStreamRecord,
  routeMetaAndBranch,
  type MetaSink,
  type StreamSink,
} from "../render-stream-record";
import { UnrenderedRanges } from "../render-window";
import { getBehavior } from "../behavior";
import { showActionFailureToast } from "../error-toast";
import { validateLocalLaunch } from "../launch-requests";

interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  /** P5.1：per-file 单调 seq。SessionViewer 一次性 load 时按 seq 排到 timeline。 */
  seq: number;
  message: JsonlRecord;
}

// C04d 批 6a：手写的 `BranchResult` 镜像已删——**包装层的签名直接提供它**，
// 本文件不再需要本地标注（生成物仍被 ipc/commands.ts 的 import 链消费）。

export interface ViewerOptions {
  jsonlPath: string;
  /** 顶栏标题：custom_title / ai_title / first_user_excerpt 之一 */
  displayTitle: string;
  /** 子标题：项目名 + cwd */
  subtitle?: string;
  /**
   * issue #6：从全文搜索结果跳进来时给定命中消息的 uuid。加载完成后定位到该卡片
   * （展开所在折叠段）滚动居中 + 临时高亮，而非默认贴底。
   */
  scrollToUuid?: string;
  /**
   * issue #16：远端来源。undefined=本地（走 stream_read_session_jsonl）；
   * host=远端（走 stream_read_remote_session，经 SSH 拉取，chunk 口径一致）。
   */
  origin?: string;
  /**
   * F62：会话工作目录（= 历史条目 projectPath）。本地会话建分支后一键 resume 时作
   * 新终端起始目录。远端会话不建分支，可缺省。
   */
  cwd?: string;
  /**
   * F77：抑制「从这一轮建分支」按钮。子 agent 记录（点进 agent 看记录）不是可分支的会话——
   * 对子 agent jsonl 建分支会产出残缺/无归属会话，故 F77 传 true 关掉该可操作面。
   */
  suppressBranch?: boolean;
}

const TAIL_INITIAL = 150; // 首屏渲染的末尾条数(实测 37MB 全量 65s → 首屏 1.1s)
const BATCH_SIZE = 200; // 上翻每批补渲染条数(实测 200 条 ≈ 1-2s,肉眼可等的一口)
const TOP_TRIGGER_PX = 800; // 距顶触发补批阈值(约一屏余量,提前于撞顶)

export class SessionViewer {
  private root: HTMLElement;
  private streamEl!: HTMLElement;
  private stream: MessageStream | null = null;
  // Batch13-F39:尾部优先增量渲染状态(load 时重建)
  private payloads: JsonlLinePayload[] = [];
  private unrendered: UnrenderedRanges | null = null;
  private uuidToIdx = new Map<string, number>();
  private renderCtx: RenderContext | null = null;
  private renderSink: StreamSink | null = null;
  private folder: BranchFolder | null = null;
  private branchRecords: BranchRecord[] = [];
  private renderingBatch = false;
  private renderErrors = 0;
  private firstError = "";
  /** D 审计 S1/R 竞态:load 世代号——异步间隙(rAF/Channel)后核对,跨会话残余操作直接丢弃 */
  private loadGeneration = 0;
  private lastFirstScreenMs: number | null = null;
  private onScrollFill = (): void => {
    void this.maybeFillAbove();
  };
  private titleEl!: HTMLElement;
  private subtitleEl!: HTMLElement;
  private statusEl!: HTMLElement;
  /** 用户点"返回历史"时调用 */
  private onBack: () => void;

  constructor(onBack: () => void) {
    this.onBack = onBack;
    this.root = this.build();
  }

  get element(): HTMLElement {
    return this.root;
  }

  /**
   * Batch13-F39:两阶段加载(实测 37MB 全量渲染 65.5s → 首屏 1.1s)。
   *
   * 阶段一(收集):后端按 100 行一 chunk 经 Channel 发,前端只收集 payload +
   * 预提取 branch/queue 数据,**不渲染**。
   * 阶段二(增量渲染):收齐后渲染末尾 TAIL_INITIAL 条首屏(+深链岛)→ fold 一次
   * 重建 → 贴底/定位;此后上翻由 maybeFillAbove 按批补渲染,每批先摊平再插入再重折。
   *
   * 取消:dispose() 时 stream = null + loadGeneration 递增,后续 chunk/异步残余
   * 双守卫丢弃;Channel 随 GC 回收,backend 下次 send 返 Err 自然 break。
   */
  async load(opts: ViewerOptions): Promise<void> {
    this.titleEl.textContent = opts.displayTitle;
    this.subtitleEl.textContent = opts.subtitle ?? "";

    this.disposeStream();
    const gen = ++this.loadGeneration;
    this.streamEl.replaceChildren();
    this.stream = new MessageStream(this.streamEl);

    this.statusEl.textContent = "加载中…";

    // Batch13-F39:lazy hljs(此前 viewer eager 全量高亮,是 65s 的组成部分)
    const ctx: RenderContext = {
      parentPath: opts.jsonlPath,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
      // Batch9-F29（审计三家共识）：远端会话展开 subagent 需 origin 降级
      origin: opts.origin ?? null,
      lazy: true,
    };
    const timeline = new RecordTimeline(this.stream);
    // Batch13-F39:增量渲染期间 renderStreamRecord 会重复 feed branch 记录——
    // branch/queue 数据在收集阶段一次性预提取,sink 的对应回调置 no-op
    const sink: StreamSink = {
      timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      observeForLazyEnhance: true,
      // F62：仅本地会话给每张 user/assistant 卡挂「从这一轮建分支」按钮（远端会话不建分支）。
      // F77：子 agent 记录 `suppressBranch` 也关掉（子 agent jsonl 不是可分支的会话）。
      onCardRendered:
        opts.origin || opts.suppressBranch
          ? undefined
          : (el, msg) => this.attachBranchButton(el, msg, opts.jsonlPath, opts.cwd),
    };
    this.renderCtx = ctx;
    this.renderSink = sink;
    this.payloads = [];
    this.uuidToIdx.clear();
    this.branchRecords = [];
    this.renderErrors = 0;
    this.firstError = "";
    const queuedContents: string[] = [];
    let totalRecords = 0;
    const t0 = performance.now(); // Batch13-F39 实测仪表:首屏耗时常驻状态栏

    // 渲染韧性 + 探针：renderStreamRecord 在 Channel 回调里跑，一旦某条记录渲染
    // 抛错，异常**不会**被下面 load() 的 try/catch 接住（不同事件回合），会导致
    // totalRecords 卡住 → while 循环空转 → 整个查看器空白（已观察到的 bug）。
    // 这里逐条 try/catch：单条失败不影响其余，并记录首个错误供定位 / 显示。
    // F39:Channel 阶段只收集 payload + 预提取 branch/queue 数据,不渲染——
    // 全量渲染 37MB 实测 65s,渲染延后到「尾段首屏 + 上翻增量」
    // F40c(账本收敛,清偿 F39 parity 欠账):meta/branch 提取与渲染路径共用
    // routeMetaAndBranch 单一来源——此前手工复刻曾被 D 审计点名为漂移风险。
    const collectSink: MetaSink = {
      onBranchRecord: (br) => this.branchRecords.push(br),
      onQueueOperation: (content) => queuedContents.push(content),
      onTitleUpdate: () => {}, // viewer 标题静态,不消费 ai-title
    };
    const channel = new Channel<JsonlLinePayload[]>();
    channel.onmessage = (chunk) => {
      if (!this.stream || this.loadGeneration !== gen) return; // 已 dispose / 已换会话
      for (const p of chunk) {
        // 逐条 try/catch:异形 message 抛错不能丢整 chunk 计数,否则下面
        // while totalRecords<finalCount 永久空转(旧版就修过这类卡死)
        try {
          routeMetaAndBranch(p, collectSink);
        } catch (err) {
          console.warn("[session-viewer] 收集阶段单条异常(跳过):", err);
        }
        this.payloads.push(p); // 占位必须 push:下标与 finalCount 对齐(meta 也占位)
      }
      totalRecords += chunk.length;
      this.statusEl.textContent = `接收中 · 已 ${totalRecords} 条…`;
    };

    try {
      // issue #16：远端会话走 stream_read_remote_session（SSH 拉取，payload 带
      // origin），本地走原 IPC。chunk 结构一致，下游渲染零差异。
      //
      // **C04d 批 6a：这里原来是「动态派发口」，现在不是了。**
      // 原形态是 `const ipc = origin ? "A" : "B"` + `invoke<number>(ipc, 超集args)`
      // ——它被 C04a 记成「7 个命令 TS 静态看不见」的盲区之一。但它**从来不是任意字符串**，
      // 只是在**两个字面量之间**选。改成两次静态调用后：
      // ① 那个盲区消失（两个命令名现在是 TS 侧的字面量，守卫扫得到）；
      // ② **两条命令拿到各自精确的签名**——远端那条 `origin` 必填、本地那条**根本没有
      //    origin 参数**（Rust 签名本就不同）。此前给两边传同一个超集 args、靠 Tauri
      //    丢掉 `undefined` 才对；现在「给本地命令传 origin」是**编译期错误**。
      const finalCount = opts.origin
        ? await commands.stream_read_remote_session({
            jsonlPath: opts.jsonlPath,
            origin: opts.origin,
            onChunk: channel,
          })
        : await commands.stream_read_session_jsonl({
            jsonlPath: opts.jsonlPath,
            onChunk: channel,
          });
      // **竞态修复**：Channel 和 invoke 是两条独立 IPC 通道，invoke resolve 时
      // 余下 chunk 的 onmessage 可能还排队没跑。等 totalRecords 追上 finalCount
      // 再切到最终状态文，否则会被晚到的 onmessage 又改回"加载中"。
      while (totalRecords < finalCount) {
        await new Promise((r) => setTimeout(r, 0));
        if (!this.stream || this.loadGeneration !== gen) return; // dispose / 已换会话
      }
      if (!this.stream) return;
      // F39:排序防御(chunk 应有序,二分插入也容乱序,排序让区间账本与 payload 下标对齐)
      this.payloads.sort((a, b) => a.seq - b.seq);
      this.uuidToIdx.clear();
      this.payloads.forEach((p, i) => {
        const u = (p.message as { uuid?: string }).uuid;
        if (u) this.uuidToIdx.set(u, i);
      });
      this.unrendered = new UnrenderedRanges(this.payloads.length);
      // fold 组件建一次,增量批后幂等重建(branchRecords 全量已知;搬的只是已渲染卡)
      this.folder = new BranchFolder(this.stream.contentElement);
      for (const c of queuedContents) this.folder.addQueuedContent(c);

      // 首屏:深链 → 目标岛 + 尾段;否则只尾段
      const total = this.payloads.length;
      const targetIdx = opts.scrollToUuid
        ? (this.uuidToIdx.get(opts.scrollToUuid) ?? null)
        : null;
      this.renderRange(Math.max(0, total - TAIL_INITIAL), total);
      if (targetIdx !== null && this.unrendered.contains(targetIdx)) {
        this.renderRange(Math.max(0, targetIdx - 100), Math.min(total, targetIdx + 100));
      }
      this.rebuildFold();
      this.lastFirstScreenMs = Math.round(performance.now() - t0);
      this.updateStatus(total);
      // issue #6：从搜索结果跳进来 → 定位到命中消息；否则默认贴底。
      if (opts.scrollToUuid) {
        this.scrollToMessage(opts.scrollToUuid);
      } else {
        this.stream?.scrollToBottom();
      }
      // 上翻补批:挂在 .stream 滚动容器上(dispose 时随 streamEl 替换自然解绑)
      this.streamEl.addEventListener("scroll", this.onScrollFill, { passive: true });
      // R1(D 审计):短会话首屏不足一屏时永远不会有 scroll 事件——主动踢一脚自链
      requestAnimationFrame(() => void this.maybeFillAbove());
    } catch (e) {
      this.statusEl.textContent = `加载失败：${String(e)}`;
    }
  }

  /** F39:渲染 payload 下标区间 [lo,hi)(逐条 renderStreamRecord,二分插入保序) */
  private renderRange(lo: number, hi: number): void {
    if (!this.renderCtx || !this.renderSink || !this.unrendered || !this.stream) return;
    // 不变量:二分插入只发生在**摊平**的 DOM 上——邻居若已被 fold wrap 收编,
    // insertBefore 会 NotFoundError(E2E 实测 58 条失败)。先摊平,批后重折。
    this.folder?.unwrapAll();
    const from = Math.max(0, lo);
    const to = Math.min(this.payloads.length, hi);
    // S-7 对齐(Phase G 终审:同协议修复双向回灌)——批内暂停逐卡守卫 snap:
    // 首屏 150 卡逐卡 snap 各读一次 scrollHeight = 150 次强制 reflow,直接摊在
    // 首屏耗时里;批末按粘底状态一次贴底(与 tabs.renderPayloadsBatch 同款)。
    this.stream.batchInsert(() => {
      for (let i = from; i < to; i++) {
        if (!this.unrendered!.contains(i)) continue; // 已渲染(岛重叠)跳过
        const p = this.payloads[i];
        try {
          renderStreamRecord(p, this.renderCtx!, this.renderSink!);
        } catch (err) {
          this.renderErrors += 1;
          if (!this.firstError) {
            const t = (p as { message?: { type?: string } })?.message?.type ?? "?";
            this.firstError = `seq=${p?.seq} type=${t}: ${String(err)}`;
            console.error("[session-viewer] renderStreamRecord 抛错", p, err);
          }
        }
      }
    });
    this.unrendered.markRendered(from, to);
    // R2(D 审计):批缝落在 tool_use/tool_result 配对中间时,result 先渲染成
    // fallback 孤儿卡;上方批把 tool_use 补出来后必须回填合并(TabManager 每批
    // onBatchEnd 都做,viewer 此前从未调过——乱序渲染下是确定性视觉回归)。
    // F40b S-6:孤儿卡出 DOM 的同时出账,防悬空 anchor。
    if (this.renderCtx) {
      for (const el of reconcilePendingToolResults(this.renderCtx)) {
        this.renderSink?.timeline.removeByElement(el);
      }
    }
  }

  /**
   * F62：给一张 user/assistant 卡挂「从这一轮创建分支」按钮（仅本地会话；见 onCardRendered）。
   * 点击 → 后端 `create_branch_session` 复制 [根…这条] 前缀产出新会话（原生 forkedFrom 格式，
   * 原会话不改），成功后弹 info toast，点 toast 一键 resume 新分支。增量重渲会重复调本函数，
   * 靠幂等守卫（已挂过就跳过）避免重复按钮。
   */
  private attachBranchButton(
    cardEl: HTMLElement,
    message: JsonlRecord,
    jsonlPath: string,
    cwd: string | undefined,
  ): void {
    if (message.type !== "user" && message.type !== "assistant") return;
    const uuid = message.uuid;
    if (!uuid) return;
    if (cardEl.querySelector(":scope > .viewer-branch-btn")) return; // 幂等
    cardEl.classList.add("has-branch-btn");

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "viewer-branch-btn";
    btn.textContent = "⑂";
    btn.title = "从这一轮创建分支（复制到这条为止 → 新会话，原会话不变，可 resume）";
    btn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      if (btn.dataset.busy === "1") return;
      btn.dataset.busy = "1";
      try {
        const res = await commands.create_branch_session({
          sourceJsonlPath: jsonlPath,
          messageUuid: uuid,
        });
        const sid8 = res.sessionId.slice(0, 8);
        showActionFailureToast("✓ 已从这一轮创建分支", `点此在新终端 resume 分支 ${sid8}`, {
          level: "info",
          durationMs: 8000,
          onClick: () => void this.resumeBranch(res.sessionId, cwd),
        });
        btn.textContent = "✓";
        window.setTimeout(() => (btn.textContent = "⑂"), 2000);
      } catch (err) {
        showActionFailureToast("创建分支失败", String(err));
      } finally {
        btn.dataset.busy = "0";
      }
    });
    cardEl.appendChild(btn);
  }

  /** F62：在新终端 resume 刚建的分支（复用本地 resume 命令 + 用户自定义 launcher）。 */
  private async resumeBranch(sessionId: string, cwd: string | undefined): Promise<void> {
    // F06：走一遍本地 IR 构造，sid 校验先于任何 IPC 往返；构造失败与拉起失败分两个 catch，
    // headline 对齐远端 `runRemoteResume` 的"无法构造 resume 命令"/执行失败两分。
    try {
      validateLocalLaunch({ kind: "resume", sid: sessionId }, cwd ?? "");
    } catch (err) {
      showActionFailureToast("无法构造 resume 命令", String(err));
      return;
    }
    try {
      const behavior = await getBehavior();
      await commands.resume_history_session({
        sessionId,
        cwd: cwd ?? "",
        launcher: behavior.resumeCommandLocal || null,
      });
    } catch (err) {
      showActionFailureToast("恢复分支失败", String(err));
    }
  }

  /** F39:增量批后幂等重建 fold(branchRecords 全量;未渲染 uuid 的卡不在 DOM,自然跳过) */
  private rebuildFold(): void {
    if (!this.folder || this.branchRecords.length === 0) return;
    try {
      this.folder.setRecordsAndRebuild(this.branchRecords);
    } catch (err) {
      this.renderErrors += 1;
      if (!this.firstError) this.firstError = `branch-fold: ${String(err)}`;
      console.error("[session-viewer] BranchFolder.setRecordsAndRebuild 抛错", err);
    }
  }

  private updateStatus(total: number): void {
    const left = this.unrendered?.remaining ?? 0;
    const shown = total - left;
    const err =
      this.renderErrors > 0 ? `（${this.renderErrors} 条渲染失败，首个 ${this.firstError}）` : "";
    const ms = this.lastFirstScreenMs !== null ? ` · 首屏 ${this.lastFirstScreenMs}ms` : "";
    // 顶部还有洞 → "上翻加载";只剩深链岛-尾段之间的内部缝 → 如实说(上翻无洞可补)
    const fillable = this.unrendered
      ? this.unrendered.gapAbove(this.unrendered.lowestRenderedIdx()) !== null
      : false;
    this.statusEl.textContent =
      left > 0
        ? `已显示 ${shown}/${total} 条${ms} · ${fillable ? "上翻加载更早" : "中部有未加载段（搜索跳转缝）"}${err}`
        : `${total} 条记录${ms} · 只读历史视图${err}`;
  }

  /** R1:触发判定——不足一屏(无滚动条,事件永远不来)或滚近顶部 */
  private shouldFill(): boolean {
    const el = this.streamEl;
    return el.scrollHeight - el.clientHeight <= 1 || el.scrollTop <= TOP_TRIGGER_PX;
  }

  /**
   * F39:滚近顶部/不足一屏 → 往上补一批。
   * 视口稳定不再依赖原生 overflow-anchor(D 审计 R3:每批 fold 全量重建会销毁
   * 锚点节点致跳视口;且 scrollTop==0 时规范不做补偿、WebKitGTK 根本没有锚定)
   * ——改为手动补偿:突变同一任务内完成,临时关原生锚定,按 scrollHeight 差值回写。
   * 批后自链复检(R1:零高批/短内容场景没有 scroll 事件可依赖)。
   */
  private async maybeFillAbove(): Promise<void> {
    if (this.renderingBatch || !this.unrendered || this.unrendered.isEmpty) return;
    if (!this.shouldFill()) return;
    // 选区守卫(Phase G 终审:与 tabs.fillAbove 对齐)——补批的 unwrapAll/rebuildFold
    // 会杀进行中的选区,等下次 scroll 再试
    const sel = document.getSelection();
    if (sel && !sel.isCollapsed) return;
    const gap = this.unrendered.gapAbove(this.unrendered.lowestRenderedIdx());
    if (!gap) return;
    const gen = this.loadGeneration;
    this.renderingBatch = true;
    this.statusEl.textContent = "加载更早消息…";
    try {
      // 让状态文先绘一帧再做同步渲染批
      await new Promise((r) => requestAnimationFrame(() => r(null)));
      // 世代守卫:rAF 间隙里可能已切换会话(旧 gap 套新会话会渲出错乱岛/覆写状态栏)
      if (!this.stream || this.loadGeneration !== gen) return;
      const [a, b] = gap;
      const el = this.streamEl;
      const beforeH = el.scrollHeight;
      const beforeTop = el.scrollTop;
      try {
        el.style.overflowAnchor = "none";
        this.renderRange(Math.max(a, b - BATCH_SIZE), b);
        this.rebuildFold();
        el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
      } finally {
        // 还原必须在 finally(Phase G 终审,三家共识——与 tabs.fillAbove 的
        // F40b-D 修复对齐):renderRange 的 unwrapAll/reconcile 段抛出会留下
        // overflow-anchor:none,该 viewer 会话永久失去原生锚定(§21.2)
        el.style.overflowAnchor = "";
      }
      this.updateStatus(this.payloads.length);
    } finally {
      this.renderingBatch = false;
    }
    // 自链:下一帧复检(补批通常把 scrollTop 顶过阈值自然停;零高批/不足一屏则继续)
    requestAnimationFrame(() => void this.maybeFillAbove());
  }

  /**
   * issue #6：滚动定位到指定 uuid 的卡片并临时高亮。
   * 命中卡片可能被折叠在 ESC 回退段（`<details>`）里 → 先展开所有祖先 details 再滚。
   * 找不到（极少：该 uuid 未渲染成带 data-uuid 的卡）则退化为贴底。
   */
  private scrollToMessage(uuid: string): void {
    // F39:目标还没渲染(非首屏路径调进来,如未来的重复定位)→ 先渲染目标岛
    const idx = this.uuidToIdx.get(uuid);
    if (idx !== undefined && this.unrendered?.contains(idx)) {
      this.renderRange(Math.max(0, idx - 100), Math.min(this.payloads.length, idx + 100));
      this.rebuildFold();
      this.updateStatus(this.payloads.length);
    }
    // CSS.escape 防 uuid 里有特殊字符破坏选择器
    const sel = `[data-uuid="${CSS.escape(uuid)}"]`;
    const el = this.streamEl.querySelector<HTMLElement>(sel);
    if (!el) {
      this.stream?.scrollToBottom();
      return;
    }
    // 展开所有折叠祖先，确保目标可见。注:ESC 回退段是 div.branch-fold-wrap
    // + .expanded 类(非 <details>)——此前只开 details,命中折叠段内的卡会被
    // 0fr 裁剪、flash 不可见(Batch13 D 审计发现的既有 bug)
    let p: HTMLElement | null = el.parentElement;
    while (p && p !== this.streamEl) {
      if (p instanceof HTMLDetailsElement) p.open = true;
      if (p.classList.contains("branch-fold-wrap") && !p.classList.contains("expanded")) {
        p.classList.add("expanded");
        p.querySelector(".branch-fold-header")?.setAttribute("aria-expanded", "true");
      }
      p = p.parentElement;
    }
    el.scrollIntoView({ block: "center" });
    // Batch13-F38:首次落点基于 content-visibility 估值几何;双 rAF 后周边已
    // 材料化(真实尺寸),幂等重发一次让 block:center 落点精确
    requestAnimationFrame(() =>
      requestAnimationFrame(() => el.scrollIntoView({ block: "center" })),
    );
    el.classList.add("search-hit-flash");
    // 动画结束后移除 class（再次跳同一条还能重放）
    window.setTimeout(() => el.classList.remove("search-hit-flash"), 2200);
  }

  /** 主动释放（HistoryView 卸载本组件时调） */
  dispose(): void {
    this.disposeStream();
  }

  private disposeStream(): void {
    this.streamEl?.removeEventListener("scroll", this.onScrollFill);
    if (this.stream) {
      this.stream.dispose();
      this.stream = null;
    }
    // F39:释放增量渲染状态(payloads 可达 37MB 量级)
    this.payloads = [];
    this.unrendered = null;
    this.uuidToIdx.clear();
    this.renderCtx = null;
    this.renderSink = null;
    this.folder = null;
    this.branchRecords = [];
    this.renderingBatch = false;
    this.lastFirstScreenMs = null;
  }

  // (旧的 renderAll 被流式 load 替代，删了 —— v2.2 issue #12)

  // === DOM ===

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "session-viewer";

    // 顶栏
    const bar = document.createElement("div");
    bar.className = "session-viewer-bar";

    const backBtn = document.createElement("button");
    backBtn.type = "button";
    backBtn.className = "history-back";
    backBtn.textContent = "← 返回历史";
    backBtn.addEventListener("click", () => this.onBack());
    bar.appendChild(backBtn);

    const titles = document.createElement("div");
    titles.className = "session-viewer-titles";
    this.titleEl = document.createElement("div");
    this.titleEl.className = "session-viewer-title";
    titles.appendChild(this.titleEl);
    this.subtitleEl = document.createElement("div");
    this.subtitleEl.className = "session-viewer-subtitle";
    titles.appendChild(this.subtitleEl);
    bar.appendChild(titles);

    view.appendChild(bar);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "history-status";
    view.appendChild(this.statusEl);

    // 消息流容器（与实时 Tab 用相同的 .stream 样式）
    this.streamEl = document.createElement("div");
    this.streamEl.className = "stream session-viewer-stream";
    view.appendChild(this.streamEl);

    return view;
  }
}
