// B04：cc-bus 钩子在 `~/.claude/settings.json` 里的**只读诊断** + 生成待贴文本。
//
// **本文件里没有、也不得有任何写入路径**（有测试守着）。用户 2026-07-28 定调不写
// `~/.claude/settings.json`；`cc-bus-install.sh` 第 3 行同样写着"只做可逆的本地安装：
// 不改全局 settings.json、不 systemctl"——**两边一致，是这个生态的既定约定**，不是加码。
// 这是一份**共享的全局配置**（用户自己的编辑器、别的工具、别的 skill 都可能在动它），
// cc-monitor 不该单方面改；文案里要把这个理由说给用户听，而不是只丢一句"请手动粘贴"。
//
// **四态而不是三态**（见 features/B04-hook-states-from-real-disk.md）：计划原写三态
// 「已装 / 未装 / 装了但指向别的路径」，实测发现「已装」必须分形态才说得清哪种才是问题。
// 用户盘上装的是 `"$HOME/.local/bin/cc-register" …`，与规范片段的裸 `cc-register …`
// **功能等价但字符串不等**——按等值比较会把这套**完全正确的安装**报成第三态，
// 然后建议用户去修一个没坏的东西。所以这里必须把
// 「显式路径且存在（没问题）」与「显式路径但不存在（真问题）」**分开渲染**。
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { isLocalOrigin, isRemoteOrigin, type Origin } from "../ipc/origin";
import { commands } from "../ipc/commands";
import { buildPasteBlock, type PasteBlock } from "../paste-block"; // T03

// C04d 批 3：四个类型全部换成生成物（源 `hooks_diag.rs`）。手写版与生成物**逐字等价**
// ——这一处零漂移，价值是防将来漂。`HookState` 是
// `#[serde(tag = "kind", rename_all = "kebab-case")]` 的内部标记枚举 → 判别联合。
import type { HooksDiagnosis } from "../generated/HooksDiagnosis";
import type { HooksReport } from "../generated/HooksReport";
import type { HookState } from "../generated/HookState";
import type { Snippet } from "../generated/Snippet";
import { copyText } from "../copy-table";

export type { HooksDiagnosis, HooksReport, HookState, Snippet };

/** 一态 → 展示文案 + 三档语气。**`path-missing` 绝不能说成"已装"**；
 *  `unknown` 要中性（既不说已装也不说未装）。 */
export function describeState(st: HookState): {
  text: string;
  tone: "ok" | "bad" | "unknown";
} {
  switch (st.kind) {
    case "not-installed":
      return { text: copyText("ccBusHooks.state.missing"), tone: "bad" };
    case "installed-via-path":
      return { text: copyText("ccBusHooks.state.onPath"), tone: "ok" };
    case "installed-at-path":
      // 这是用户当前的实际状态。**不能渲染成"有问题"**——它能跑。
      return { text: copyText("ccBusHooks.state.explicit", { path: st.path }), tone: "ok" };
    case "path-missing":
      // 真正的第三态：看着像装了，其实指不到东西。
      return { text: copyText("ccBusHooks.state.brokenPath", { path: st.path }), tone: "bad" };
    case "unknown":
      // **第五态**（B04 审计 B04-4）：命令里出现了目标程序，但它包在 sh -c / env /
      // timeout 之类里，判不出是不是真的在跑。此前这种情况落到"未装"（红），
      // 于是装了包装写法的用户会去贴一份重复的钩子。**猜"未装"和猜"已装"一样是猜。**
      return {
        text: copyText("ccBusHooks.state.indirect", { command: st.command }),
        tone: "unknown",
      };
    default: {
      // 后端将来加第六态时，这里**不能整个炸掉**（原实现无 default，`d` 会是 undefined，
      // `d.ok` 当场抛，整个 renderDiag 挂掉）——与本工作区"对自己的 IPC 也要防御"一致。
      const unknownKind = (st as { kind?: string }).kind ?? "?";
      return { text: copyText("ccBusHooks.state.unknown", { unknownKind }), tone: "unknown" };
    }
  }
}

export class CcBusHooksSection {
  readonly element: HTMLElement;
  /** E59：当前在看哪台机器（只读显示；值由共用 store 给，不可在本分节改）。 */
  private originName!: HTMLSpanElement;

  /** 已配置的远端清单——用来判断「store 给的这台我认不认得」。 */
  private knownOrigins: string[] = [];

  /**
   * 「检查远端」要诊断的那台远端；`null` = **没有可诊断的远端**（在看本机，或 store 给的那台不在已配置清单里）。
   * 〔C4a〕它不是 origin（上一版叫 `origin`、`null` 同时装着「本机」与「不认得」两件事）——本机在 store 里是
   * `LOCAL_ORIGIN`，这一格只回答「按钮该打向哪台远端」。
   */
  private diagnosable: string | null = null;

  /** 「检查远端」按钮。**存直接引用而不是每次 `this.element.querySelector`** ——
   *  `build()` 里就会调 `setOrigin`，而那时 `this.element` 还没赋值（实测抛 undefined）。 */
  private checkRemoteBtn!: HTMLButtonElement;
  private localBox!: HTMLElement;
  private remoteBox!: HTMLElement;
  private paste!: PasteBlock;
  private warnBox!: HTMLElement;
  private formSel!: HTMLSelectElement;
  private lastReport: HooksReport | null = null;

  constructor() {
    this.element = this.build();
    // ST1「延后加载」：构造期不读 —— 见 `loadNow()`。
  }

  /**
   * ST1「延后加载」（`设计/70 §5.3` 判据 2：**子页内容只在该子页可见时才发 I/O**）：
   * 构造期不再发 I/O；宿主（`panel.ts`）在**某台机器的子页第一次可见**时调它。
   * 重开设置后宿主会再调一次（重开要看新读数）。
   */
  loadNow(): void {
    void this.loadOrigins();
    // 本机诊断是纯本地读文件（无 SSH、无远端往返），代价可忽略 → 直接读。
    // 远端那份要 SSH，**只在用户点「检查远端」时才发**（同 cc-bus 驾驶舱的纪律）。
    void this.checkLocal();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless cc-bus-hooks-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      copyText("ccBusHooks.build.intro");
    root.appendChild(hint);

    // 把"为什么不代劳"说清楚，而不是只丢一句"请手动粘贴"
    const why = document.createElement("div");
    why.className = "settings-hint cc-bus-hooks-why";
    why.textContent =
      copyText("ccBusHooks.build.whyNotWrite");
    root.appendChild(why);

    const localT = document.createElement("div");
    localT.className = "settings-label";
    localT.textContent = copyText("ccBusHooks.build.local");
    root.appendChild(localT);
    this.localBox = document.createElement("div");
    this.localBox.className = "cc-bus-hooks-local";
    this.localBox.textContent = copyText("ccBusHooks.build.checking");
    root.appendChild(this.localBox);

    const row = document.createElement("div");
    row.className = "settings-row";
    const remoteT = document.createElement("span");
    remoteT.className = "settings-label";
    remoteT.textContent = copyText("ccBusHooks.build.remote");
    row.appendChild(remoteT);

    // E59：**这里原来有一个 origin 下拉，已删**（理由同 `accounts-section`：
    // 本分节只作为机器详情页上的一块存在，页头就是选择器；留一个能指向别台机器的
    // 入口 = 在标着 A 的页面上对 B 动手）。现在只显示「你在看哪台」，不可改。
    //
    // 顺带说明：本分节此前**连 change 监听都没有**，下拉只在点「检查远端」时被读一次 ——
    // 所以它与另外几块的不同步是最彻底的那个。删掉之后这个不同步在结构上没了。
    this.originName = document.createElement("span");
    this.originName.className = "settings-value cc-bus-hooks-origin";
    row.appendChild(this.originName);

    const btn = document.createElement("button");
    this.checkRemoteBtn = btn;
    btn.type = "button";
    btn.className =
      "settings-btn settings-btn-secondary cc-bus-hooks-check-remote";
    btn.textContent = copyText("ccBusHooks.build.checkRemote");
    btn.addEventListener("click", () => void this.checkRemote(btn));
    row.appendChild(btn);

    // ★ 顺序有讲究：`setOrigin` 要摸 `checkRemoteBtn`，所以必须在按钮建出来之后接。
    subscribeMachine((origin) => this.setOrigin(origin));
    this.setOrigin(getCurrentMachine());
    root.appendChild(row);

    this.remoteBox = document.createElement("div");
    this.remoteBox.className = "cc-bus-hooks-remote";
    this.remoteBox.textContent = copyText("ccBusHooks.build.notChecked");
    root.appendChild(this.remoteBox);

    root.appendChild(this.buildSnippet());
    return root;
  }

  private buildSnippet(): HTMLElement {
    const box = document.createElement("div");
    box.className = "cc-bus-hooks-snippet";

    const t = document.createElement("div");
    t.className = "settings-label";
    // **说清这段片段与警示描述的是哪一端**（T03 审计阻塞 3）：远端诊断也会算出
    // `snippet_home/bare`，但屏上这一块**只来自本机报告**（`currentSnippet()` 只读
    // `lastReport`，而 `checkRemote()` 不给它赋值）。不标明的话，用户点完「检查远端」
    // 会以为下面那段警示说的是远端盘面。
    t.textContent = copyText("ccBusHooks.snippet.title");
    box.appendChild(t);

    this.formSel = document.createElement("select");
    this.formSel.className = "settings-input cc-bus-hooks-form";
    // **$HOME 形态排第一 = 默认**：实测这台机器上用的就是它，是被验证过能工作的形态。
    for (const [v, label] of [
      // **不再宣称"与本机现状一致"**（B04 审计 B04-6）：那句话是写死的，而 `snippet()`
      // 根本不接收诊断结果。若用户的 cc-register 只在 /usr/local/bin，面板仍会推荐
      // `$HOME/.local/bin/...` 并说"与现状一致"——贴上去就是一个 PathMissing 的钩子。
      // 与其给一个可能是错的承诺，不如只描述两种形态各自的取舍，让用户按诊断结果自己选。
      ["home", copyText("ccBusHooks.snippet.explicit")],
      ["bare", copyText("ccBusHooks.snippet.bare")],
    ]) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = label;
      this.formSel.appendChild(o);
    }
    this.formSel.addEventListener("change", () => this.renderSnippet());
    box.appendChild(this.formSel);

    // T03：输出面 + 复制按钮 + 三句话改走统一组件。**形态选择器留在这里**
    // （它是这一处独有的），生成仍在 Rust 侧。
    // **警示由本 section 自己渲染**（T03 审计：它只有一个消费者，不该占共享组件的槽；
    // 而且审计实测——删掉这条接线时 56 项全绿，我却在 commit 里声称"有测试钉住它上屏"）。
    this.warnBox = document.createElement("div");
    this.warnBox.className = "cc-bus-hooks-form-warning";
    this.warnBox.hidden = true;
    box.appendChild(this.warnBox);

    this.paste = buildPasteBlock({
      text: () => this.currentSnippet()?.text ?? "",
      target: copyText("ccBusHooks.snippet.target"),
      mergeNote:
        copyText("ccBusHooks.snippet.merge"),
      activation: copyText("ccBusHooks.snippet.activation"),
      invalidReason: (t) => (t.trim() ? null : copyText("ccBusHooks.snippet.notReady")),
      multiline: true,
      rows: 8,
    });
    box.appendChild(this.paste.element);
    return box;
  }

  private async loadOrigins(): Promise<void> {
    let origins: string[] = [];
    try {
      // **别只防 reject**：invoke 也可能 resolve 成 undefined/非数组（桥接层异常、命令改了
      // 返回类型）。只 catch 不校验形状的话，下一行 `.length` 会直接抛 —— 这正是本工作区
      // 一路在守的「脏数据不能把面板搞崩」，对自己的 IPC 返回值同样适用。
      const got = await commands.list_remote_mcp_origins();
      if (Array.isArray(got)) origins = got;
    } catch {
      /* 无远端不影响本机诊断 */
    }
    this.knownOrigins = origins;
    if (origins.length === 0) {
      this.checkRemoteBtn.disabled = true;
      this.originName.textContent = copyText("ccBusHooks.origins.noneOption");
      this.remoteBox.textContent = copyText("ccBusHooks.origins.none");
      return;
    }
    // 清单到手之后重新认一次 store 给的那台（清单是异步来的，可能晚于第一次 setOrigin）。
    this.setOrigin(getCurrentMachine());
  }

  /**
   * E59：跟随共用 store。**不在切换时自动发诊断请求** —— 本分节的既有语义就是
   * 「点了才发」（只读诊断，不替用户改 `~/.claude/settings.json`），自动发就成了变相轮询。
   */
  private setOrigin(origin: Origin): void {
    const known = isRemoteOrigin(origin) && this.knownOrigins.includes(origin);
    this.diagnosable = known ? origin : null;
    this.originName.textContent = known
      ? origin
      : isLocalOrigin(origin)
        ? copyText("ccBusHooks.origin.localPage")
        : copyText("ccBusHooks.origin.unknown", { machine: origin });
    this.checkRemoteBtn.disabled = !known;
  }

  private async checkLocal(): Promise<void> {
    try {
      const rep = await commands.diagnose_local_cc_bus_hooks();
      this.lastReport = rep;
      this.renderDiag(this.localBox, rep);
      this.renderSnippet();
    } catch (e) {
      this.localBox.textContent = copyText("ccBusHooks.local.failed", { e: String(e) });
    }
  }

  private async checkRemote(btn: HTMLButtonElement): Promise<void> {
    const origin = this.diagnosable;
    if (origin === null) return;
    btn.disabled = true;
    this.remoteBox.textContent = copyText("ccBusHooks.checkRemote.checking");
    try {
      const rep = await commands.diagnose_remote_cc_bus_hooks({ origin });
      this.renderDiag(this.remoteBox, rep, true);
    } catch (e) {
      this.remoteBox.textContent = copyText("ccBusHooks.remote.failed", { e: String(e) });
    } finally {
      btn.disabled = false;
    }
  }

  /// `showFormWarnings` 只对**远端**开：本机那份形态警示已由待贴块自己显示
  /// （那一块就是基于本机盘面的），两处都渲染就是同一句话说两遍。
  private renderDiag(
    box: HTMLElement,
    rep: HooksReport,
    showFormWarnings = false,
  ): void {
    box.replaceChildren();
    const src = document.createElement("div");
    src.className = "settings-hint cc-bus-hooks-source";
    src.textContent = copyText("ccBusHooks.diag.source", { source: rep.source });
    box.appendChild(src);

    // **这一端自己的形态警示也要看得见**（T03 审计阻塞 3：远端算了 `Snippet` 却
    // 到不了屏幕，等于白算）。两种形态各自的警示都列出来——用户正是要据此选形态。
    if (showFormWarnings) {
      for (const [label, sn] of [
        [copyText("ccBusHooks.diag.explicit"), rep.snippet_home],
        [copyText("ccBusHooks.diag.bare"), rep.snippet_bare],
      ] as const) {
        if (!sn?.warning) continue;
        const w = document.createElement("div");
        // **与待贴块自己那条用不同 class**：同名的话 `querySelector` 会先命中这里，
        // 测试就串了（第一版当场红在"切到 bare 形态 → 警示隐藏"上）。
        w.className = "cc-bus-hooks-diag-warning";
        w.textContent = `${label}：${sn.warning}`;
        box.appendChild(w);
      }
    }

    if (rep.diagnosis.note) {
      const n = document.createElement("div");
      n.className = "cc-bus-hooks-note";
      n.textContent = rep.diagnosis.note;
      box.appendChild(n);
    }

    for (const [label, st] of [
      ["SessionStart → cc-register", rep.diagnosis.session_start],
      ["Stop → cc-bus-stop-hook", rep.diagnosis.stop],
    ] as [string, HookState][]) {
      const d = describeState(st);
      const line = document.createElement("div");
      line.className = `cc-bus-hooks-state cc-bus-hooks-${d.tone}`;
      line.dataset.kind = st.kind; // 靠 dataset 认状态，不靠文案
      line.textContent = `${label}：${d.text}`;
      box.appendChild(line);
    }
  }

  /** 当前形态对应的那份片段。诊断还没读完 → `null`。 */
  private currentSnippet(): Snippet | null {
    if (!this.lastReport) return null;
    return this.formSel.value === "bare"
      ? this.lastReport.snippet_bare
      : this.lastReport.snippet_home;
  }

  private renderSnippet(): void {
    if (!this.lastReport) return;
    this.paste.refresh();
    // 形态与盘上实况冲突时，Rust 侧会给出 warning——**必须上屏**，
    // 否则那条后端判据等于白做（B04 登记项的全部价值挂在这一步）。
    const w = this.currentSnippet()?.warning ?? null;
    this.warnBox.hidden = w === null;
    this.warnBox.textContent = w ?? "";
  }
}
