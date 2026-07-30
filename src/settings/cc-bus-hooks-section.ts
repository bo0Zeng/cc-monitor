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
import { commands } from "../ipc/commands";
import { buildPasteBlock, type PasteBlock } from "../paste-block"; // T03

// C04d 批 3：四个类型全部换成生成物（源 `hooks_diag.rs`）。手写版与生成物**逐字等价**
// ——这一处零漂移，价值是防将来漂。`HookState` 是
// `#[serde(tag = "kind", rename_all = "kebab-case")]` 的内部标记枚举 → 判别联合。
import type { HooksDiagnosis } from "../generated/HooksDiagnosis";
import type { HooksReport } from "../generated/HooksReport";
import type { HookState } from "../generated/HookState";
import type { Snippet } from "../generated/Snippet";

export type { HooksDiagnosis, HooksReport, HookState, Snippet };

/** 一态 → 展示文案 + 三档语气。**`path-missing` 绝不能说成"已装"**；
 *  `unknown` 要中性（既不说已装也不说未装）。 */
export function describeState(st: HookState): {
  text: string;
  tone: "ok" | "bad" | "unknown";
} {
  switch (st.kind) {
    case "not-installed":
      return { text: "未装", tone: "bad" };
    case "installed-via-path":
      return { text: "已装（走 PATH）", tone: "ok" };
    case "installed-at-path":
      // 这是用户当前的实际状态。**不能渲染成"有问题"**——它能跑。
      return { text: `已装（显式路径：${st.path}）`, tone: "ok" };
    case "path-missing":
      // 真正的第三态：看着像装了，其实指不到东西。
      return { text: `装了但路径不存在：${st.path}`, tone: "bad" };
    case "unknown":
      // **第五态**（B04 审计 B04-4）：命令里出现了目标程序，但它包在 sh -c / env /
      // timeout 之类里，判不出是不是真的在跑。此前这种情况落到"未装"（红），
      // 于是装了包装写法的用户会去贴一份重复的钩子。**猜"未装"和猜"已装"一样是猜。**
      return {
        text: `无法判断（命令形态复杂，钩子里出现了它但不是被直接执行的：${st.command}）`,
        tone: "unknown",
      };
    default: {
      // 后端将来加第六态时，这里**不能整个炸掉**（原实现无 default，`d` 会是 undefined，
      // `d.ok` 当场抛，整个 renderDiag 挂掉）——与本工作区"对自己的 IPC 也要防御"一致。
      const unknownKind = (st as { kind?: string }).kind ?? "?";
      return { text: `未知状态（${unknownKind}）`, tone: "unknown" };
    }
  }
}

export class CcBusHooksSection {
  readonly element: HTMLElement;
  private originSel!: HTMLSelectElement;
  private localBox!: HTMLElement;
  private remoteBox!: HTMLElement;
  private paste!: PasteBlock;
  private warnBox!: HTMLElement;
  private formSel!: HTMLSelectElement;
  private lastReport: HooksReport | null = null;

  constructor() {
    this.element = this.build();
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
      "cc-bus 要两个钩子才能自动收信：SessionStart → cc-register（上总线），Stop → cc-bus-stop-hook（兜底收信）。" +
      "这里只做诊断，并生成一段待贴文本。";
    root.appendChild(hint);

    // 把"为什么不代劳"说清楚，而不是只丢一句"请手动粘贴"
    const why = document.createElement("div");
    why.className = "settings-hint cc-bus-hooks-why";
    why.textContent =
      "为什么不替你写：~/.claude/settings.json 是一份共享的全局配置——你自己的编辑器、" +
      "别的工具、别的 skill 都可能在动它，cc-monitor 单方面改它有可能覆盖掉你或它们的改动。" +
      "cc-bus 自己的安装脚本同样拒绝碰它（它只软链命令、建目录，然后把钩子片段打印出来让你自己贴）。" +
      "所以这里给你诊断结果和现成片段，最后一步由你来做。";
    root.appendChild(why);

    const localT = document.createElement("div");
    localT.className = "settings-label";
    localT.textContent = "本机";
    root.appendChild(localT);
    this.localBox = document.createElement("div");
    this.localBox.className = "cc-bus-hooks-local";
    this.localBox.textContent = "检查中…";
    root.appendChild(this.localBox);

    const row = document.createElement("div");
    row.className = "settings-row";
    const remoteT = document.createElement("span");
    remoteT.className = "settings-label";
    remoteT.textContent = "远端";
    row.appendChild(remoteT);

    this.originSel = document.createElement("select");
    this.originSel.className = "settings-input cc-bus-hooks-origin";
    row.appendChild(this.originSel);

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className =
      "settings-btn settings-btn-secondary cc-bus-hooks-check-remote";
    btn.textContent = "检查远端";
    btn.addEventListener("click", () => void this.checkRemote(btn));
    row.appendChild(btn);
    root.appendChild(row);

    this.remoteBox = document.createElement("div");
    this.remoteBox.className = "cc-bus-hooks-remote";
    this.remoteBox.textContent = "尚未检查。";
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
    t.textContent = "待贴片段（基于本机盘面）";
    box.appendChild(t);

    this.formSel = document.createElement("select");
    this.formSel.className = "settings-input cc-bus-hooks-form";
    // **$HOME 形态排第一 = 默认**：实测这台机器上用的就是它，是被验证过能工作的形态。
    for (const [v, label] of [
      // **不再宣称"与本机现状一致"**（B04 审计 B04-6）：那句话是写死的，而 `snippet()`
      // 根本不接收诊断结果。若用户的 cc-register 只在 /usr/local/bin，面板仍会推荐
      // `$HOME/.local/bin/...` 并说"与现状一致"——贴上去就是一个 PathMissing 的钩子。
      // 与其给一个可能是错的承诺，不如只描述两种形态各自的取舍，让用户按诊断结果自己选。
      ["home", "$HOME 显式路径（不依赖 PATH；要求它确实装在那儿）"],
      ["bare", "裸命令（简洁；要求它在 PATH 上）"],
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
      target: "~/.claude/settings.json 的 hooks 段",
      mergeNote:
        "要合并进去，不是整份覆盖——那里可能还有别的工具的钩子，覆盖会把它们删掉。",
      activation: "改完新开一个会话才生效（当前会话不会重读它）。",
      invalidReason: (t) => (t.trim() ? null : "先等诊断读完。"),
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
    this.originSel.replaceChildren();
    if (origins.length === 0) {
      const o = document.createElement("option");
      o.value = "";
      o.textContent = "（未配置远端）";
      this.originSel.appendChild(o);
      this.originSel.disabled = true;
      const btn = this.element.querySelector<HTMLButtonElement>(
        ".cc-bus-hooks-check-remote",
      );
      if (btn) btn.disabled = true;
      this.remoteBox.textContent = "未配置远端。";
      return;
    }
    for (const s of origins) {
      const o = document.createElement("option");
      o.value = s;
      o.textContent = s;
      this.originSel.appendChild(o);
    }
  }

  private async checkLocal(): Promise<void> {
    try {
      const rep = await commands.diagnose_local_cc_bus_hooks();
      this.lastReport = rep;
      this.renderDiag(this.localBox, rep);
      this.renderSnippet();
    } catch (e) {
      this.localBox.textContent = `本机诊断失败：${String(e)}`;
    }
  }

  private async checkRemote(btn: HTMLButtonElement): Promise<void> {
    const origin = this.originSel.value;
    if (!origin) return;
    btn.disabled = true;
    this.remoteBox.textContent = "检查中…";
    try {
      const rep = await commands.diagnose_remote_cc_bus_hooks({ origin });
      this.renderDiag(this.remoteBox, rep, true);
    } catch (e) {
      this.remoteBox.textContent = `远端诊断失败：${String(e)}`;
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
    src.textContent = `诊断对象：${rep.source}`;
    box.appendChild(src);

    // **这一端自己的形态警示也要看得见**（T03 审计阻塞 3：远端算了 `Snippet` 却
    // 到不了屏幕，等于白算）。两种形态各自的警示都列出来——用户正是要据此选形态。
    if (showFormWarnings) {
      for (const [label, sn] of [
        ["$HOME 显式路径形态", rep.snippet_home],
        ["裸命令形态", rep.snippet_bare],
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
