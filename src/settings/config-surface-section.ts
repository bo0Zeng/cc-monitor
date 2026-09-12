// T02：**配置面审计视图** —— 「cc-monitor 到底动过你哪些文件」。
//
// 这一页**只读**，而且是**按需读一次**（无轮询，红线）。它把
// `src-tauri/src/tool_registry.rs` 的**环境清单闭集**遍历成一张表：每一项 app 对它是什么
// 关系（自带并装 / 该自带而还没装口 / 你自己装我提示 / 只查）、碰哪些文件、对它做什么、
// 现在是什么状态、还能不能撤。
//
// 🔴 〔`K-R65` 09-11〕**「app 假设它在」那一档没有了。** 上一行原文逐字是
// 「装 / 只查 / **假设它在**」——`K38` 裁掉了最后那一档：通用工具不是「我不看」，
// 是「**你自己装，而我会看、缺了我要说**」。⇒ 这一页对那一族**真去查**，
// 而「查了、确认没有」与「查不动」在屏幕上是两句不同的话（`promptToInstall`）。
//
// 🔴 〔`K-R60` 09-11〕**人群从 `TOOLS` 换成了那个闭集。** 原来只遍历 `TOOLS`，
// 于是 app **装不了却离不开**的那一整档（`claude` / `tmux` / 终端出口 / `git` / `ssh` …）
// 在这一页上**一行都没有** —— 而用户问「我这台机器齐了没有」时看的正是这一页。
// 表小，视图就瞎；那一档靠「不出现」表示，读者分不出「没有这种东西」与「有人忘了写」。
//
// **一条硬纪律来自后端，前端不许在这里放水**：查不了的东西显示成「未确定 + 为什么」，
// **绝不显示成"缺失"**。远端路径、相对项目目录的 `.mcp.json`、Windows 侧 `$PROFILE`
// 本机都查不到，把它们画成红叉就是对能用的安装报假警报——B04 审计已经抓过一次同型病。
import { commands } from "../ipc/commands";
import { showActionFailureToast } from "../error-toast";

// C04d 批 2：**四个线上类型全部改用生成物**（`config_surface.rs` 是源）。
// `SurfaceState` 是 `#[serde(tag = "kind", rename_all = "snake_case")]` 的内部标记枚举
// ——`ts-rs` 认 serde 属性，生成的判别联合与手写版逐字等价（本批次实测零漂移）。
//
// 本文件内部与 `.vitest.ts` 都用这些名字，所以 **import + 单独 re-export 都要有**：
// 只写 `export type { … } from` 不会把名字带进本地作用域（C02 栽两次、C04c 第三次）。
import type { ConfigSurfaceReport } from "../generated/ConfigSurfaceReport";
import type { EnvTier } from "../generated/EnvTier";
import type { SettingsScope } from "../generated/SettingsScope";
import type { SurfaceRow } from "../generated/SurfaceRow";
import type { SurfaceState } from "../generated/SurfaceState";

export type { ConfigSurfaceReport, EnvTier, SettingsScope, SurfaceRow, SurfaceState };

// 🔴 〔`K-R65`〕**「缺」与「未测过」这两个字从 `readiness.ts` 来，本文件不另写一对。**
// 件计划 `KR65D1` 逐字：「`readiness.ts` 那条 `missing` vs `unknown` 的分法**是现成的，
// 别再造一套**」。⇒ 这一页把 `absent`／`undetermined` 映到那两个 kind 上，措辞跟着它走。
import { GAP_HEAD, type GapKind } from "./readiness";

/**
 * 一态 → 它在「还差什么」那套口径里算哪一种缺口。`present` 不是缺口 ⇒ `null`。
 *
 * 这是**两页之间唯一的翻译点**：后端的三态（`present` / `absent` / `undetermined`）
 * 与前端那套两值（`missing` / `unknown`）说的是同一件事 ——
 * 「查了、确认没有」对 `missing`，「查不动 / 没测过」对 `unknown`。
 */
export function gapKindOfState(st: SurfaceState): GapKind | null {
  switch (st?.kind) {
    case "present":
      return null;
    case "absent":
      return "missing";
    case "undetermined":
      return "unknown";
    default:
      // 后端加第四态时**不许当成「没缺」** —— 不知道就是不知道。
      return "unknown";
  }
}

/**
 * 🔴 `KR65D1`：**「你缺这个，去装」这句话在这里被说出来。**
 *
 * 只有「你自己装」那一档需要它 —— 别的档缺了不该劝用户去装：
 * `AppInstalls` 有按钮、`AppShipsNoInstallerYet` 是**我们欠的实现**（劝他去装是甩锅）、
 * `AppOnlyChecks` 压根没人装得出来。
 *
 * 返回 `null` = 这一行不出这句话。
 */
export function promptToInstall(row: SurfaceRow): string | null {
  if (row.tier !== "UserInstallsWePrompt") return null;
  const kind = gapKindOfState(row.state);
  if (kind === null) return null;
  if (kind === "missing") {
    // **测过、确认没有** —— 这一句就是本件的正题：产品说得出「你缺这个，去装」。
    return `${GAP_HEAD.missing} —— cc-monitor 不装这一项，请你自己装上 \`${row.path_declared}\``;
  }
  // **查不动**：说「缺」就是替用户下一个他没做过的结论（`readiness.ts` 头注逐字）。
  return `${GAP_HEAD.unknown} —— 这一项本机查不动（见上面的原因），别当成它不在`;
}

/** 一态 → 文案 + 三档语气。**`undetermined` 必须中性且带出理由**，不能借"缺失"的红。 */
export function describeSurfaceState(st: SurfaceState): {
  text: string;
  tone: "ok" | "bad" | "unknown";
} {
  switch (st?.kind) {
    case "present":
      return { text: st.detail, tone: "ok" };
    case "absent":
      return { text: "不存在", tone: "bad" };
    case "undetermined":
      return { text: `未确定 —— ${st.why}`, tone: "unknown" };
    default: {
      // 后端将来加第四态时**不许整页炸掉**（B03 踩过：`invoke` 返回形状没校验，
      // `origins.length` 当场抛，整个 section 挂掉）。
      const k = (st as { kind?: string } | null)?.kind ?? "?";
      return { text: `未知状态（${k}）`, tone: "unknown" };
    }
  }
}

/**
 * 「能否撤」列。`uninstallable=false` 时**不给按钮**——本工作区不做点了没反应的按钮。
 *
 * 〔`K-R60` 09-11〕第二档的措辞改了半句：这一页的人群扩到闭集之后，
 * `installable=false` 里**多了一类本来就不该由 cc-monitor 装的东西**
 * （`claude` / `tmux` / Claude Code 的会话记录…）。原文逐字是「尚未支持部署」——
 * 对那一类是句**误导**（「尚未」听起来像排期问题，而那是设计判断）。
 *
 * 🔴 〔`K-R65` 09-11〕**上一版那条 ⚠ 兑现了，删掉它的前提今天成立。**
 * 原文逐字：「⚠ 两类**今天在行上分不开**：`SurfaceRow` 的线上形状里没有档这一格，
 * 而那份形状是 `ts-rs` 生成物、不在本轮写区里。⇒ 措辞把两类都涵盖住」——
 * 那句「涵盖住」的措辞（「尚未支持部署，**或**本来就不该由它装」）是一句
 * **两头下注**的话：读者读不出自己这一行是哪一种。
 * 档进线上形状之后，这里按**值**分档，不再和稀泥。
 */
export function describeUndo(row: SurfaceRow): string {
  if (row.uninstallable) return "可按围栏/整文件撤销（在对应工具的部署入口里）";
  switch (row.tier) {
    case "AppInstalls":
      return "暂无自动撤销；如需清理请按上面的路径手动处理";
    // **我们欠的实现** —— 不许说成「不该由它装」（`KR65D2` 逐字）。
    case "AppShipsNoInstallerYet":
      return "这一项该由 cc-monitor 自带，而安装入口还没写 —— 撤销也一样还没有";
    // `K38` 裁的那一档：不该我们装，所以也无所谓撤。
    case "UserInstallsWePrompt":
      return "cc-monitor 不装这一项（通用工具，请你自己装），也就无所谓撤销";
    case "AppOnlyChecks":
      return "cc-monitor 本来就不该装这一项（它不是我们的东西），也就无所谓撤销";
    default:
      // 后端加第五档时**不许整页炸掉**，也不许假装认识它。
      return "这一项的档本前端还不认识 —— 撤销请按上面的路径手动处理";
  }
}

/**
 * 🔴 `KR65D2` 的**「数得出来」那一半上屏**：这一页上有几项是「app 该自带、
 * 而今天还没有装口」。
 *
 * 件计划逐字：「把 `cc-acct-iso-local` 标成「app 该装」而不给实现 ⇒
 * **必须能被数出来**（不是红，是**能报出来**）」——**报出来的地方就是这里**，
 * 用户在这一页上看得见这个数，不用去读判据。
 *
 * 返回 `null` = 一项都没有（那时整句不渲染，不写「0 项」）。
 */
export function summarizeOwedInstallers(rows: SurfaceRow[]): string | null {
  const owed = rows.filter((r) => r.tier === "AppShipsNoInstallerYet");
  if (owed.length === 0) return null;
  const names = [...new Set(owed.map((r) => r.tool_name))];
  return `其中 ${names.length} 项该由 cc-monitor 自带、而安装入口还没写：${names.join("、")}`;
}

/** 生成一段可复制的纯文本诊断，便于用户贴给我或存档。 */
export function formatReportText(r: ConfigSurfaceReport): string {
  const lines: string[] = [];
  lines.push("== cc-monitor 配置面审计 ==");
  lines.push(`HOME=${r.home}`);
  lines.push(`~/.claude 解析为=${r.claude_config_dir}`);
  lines.push("");
  let lastTool = "";
  for (const row of r.rows) {
    if (row.tool_name !== lastTool) {
      lines.push(`[${row.tool_name}] ${row.source_label}`);
      lastTool = row.tool_name;
    }
    const st = describeSurfaceState(row.state);
    lines.push(`  ${row.path_declared}${row.note ? `（${row.note}）` : ""}`);
    lines.push(`    位置: ${row.host_label}`);
    if (row.path_resolved) lines.push(`    解析为: ${row.path_resolved}`);
    lines.push(`    我们做什么: ${row.effect_label}`);
    lines.push(`    现状: ${st.text}`);
    // `KR65D1`：那句话**也要进这份可复制的诊断文本** —— 用户贴出来的那一份
    // 如果不含它，「产品说得出「你缺这个，去装」」就只在屏幕上成立。
    const prompt = promptToInstall(row);
    if (prompt) lines.push(`    ${prompt}`);
  }
  const owed = summarizeOwedInstallers(r.rows);
  if (owed) {
    lines.push("");
    lines.push(`  ${owed}`);
  }
  lines.push("");
  lines.push("== settings.json 的各作用域（会影响钩子诊断结论）==");
  for (const s of r.settings_scopes) {
    const st = describeSurfaceState(s.state);
    const hooks =
      s.has_cc_bus_hooks === null
        ? "读不到，不猜"
        : s.has_cc_bus_hooks
          ? "含 cc-bus 钩子字样"
          : "不含 cc-bus 钩子字样";
    lines.push(`  [${s.scope}] ${s.path}`);
    lines.push(`    ${st.text} · ${hooks} · ${s.precedence_note}`);
  }
  return lines.join("\n");
}

export class ConfigSurfaceSection {
  readonly element: HTMLElement;
  private body!: HTMLElement;
  private scopesBox!: HTMLElement;
  private meta!: HTMLElement;
  /** `KR65D2`：「app 该自带而还没有装口」那一格的计数行。空时整行不显示。 */
  private owed!: HTMLElement;
  private copyBtn!: HTMLButtonElement;
  private last: ConfigSurfaceReport | null = null;

  constructor() {
    this.element = this.build();
    void this.refresh();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless config-surface-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "这一页列出 cc-monitor 会碰你哪些文件、对它做什么、现在什么状态。" +
      "这一页只读：不会写任何东西，也不后台轮询——每次打开或点「重新扫描」才读一次。";
    root.appendChild(hint);

    const honesty = document.createElement("div");
    honesty.className = "settings-hint config-surface-honesty";
    honesty.textContent =
      "查不了的会写成「未确定」并说明原因，不会画成红叉。远端路径要 SSH（请到部署向导里查）、" +
      "项目里的 .mcp.json 得先知道是哪个项目、Windows 的 $PROFILE 由 PowerShell 决定——" +
      "这三类本机无从判断，报成「缺失」会是假警报。";
    root.appendChild(honesty);

    const bar = document.createElement("div");
    bar.className = "settings-row config-surface-bar";
    const rescan = document.createElement("button");
    rescan.type = "button";
    rescan.className = "btn";
    rescan.textContent = "重新扫描";
    rescan.addEventListener("click", () => void this.refresh());
    bar.appendChild(rescan);

    this.copyBtn = document.createElement("button");
    this.copyBtn.type = "button";
    this.copyBtn.className = "btn";
    this.copyBtn.textContent = "复制诊断文本";
    this.copyBtn.disabled = true;
    this.copyBtn.addEventListener("click", () => void this.copy());
    bar.appendChild(this.copyBtn);
    root.appendChild(bar);

    this.meta = document.createElement("div");
    this.meta.className = "settings-hint config-surface-meta";
    root.appendChild(this.meta);

    this.owed = document.createElement("div");
    this.owed.className = "settings-hint config-surface-owed";
    this.owed.hidden = true;
    root.appendChild(this.owed);

    this.body = document.createElement("div");
    this.body.className = "config-surface-body";
    root.appendChild(this.body);

    const scopesT = document.createElement("div");
    scopesT.className = "settings-subtitle";
    scopesT.textContent = "settings.json 的各作用域";
    root.appendChild(scopesT);
    const scopesHint = document.createElement("div");
    scopesHint.className = "settings-hint";
    scopesHint.textContent =
      "钩子可以写在多个作用域里，优先级从低到高。钩子诊断读的是「用户级」那一份——" +
      "所以如果你把钩子写在了别处，那边报的「未装」可能是错的。";
    root.appendChild(scopesHint);
    this.scopesBox = document.createElement("div");
    this.scopesBox.className = "config-surface-scopes";
    root.appendChild(this.scopesBox);

    return root;
  }

  async refresh(): Promise<void> {
    this.body.textContent = "扫描中…";
    try {
      const r = await commands.config_surface_report();
      // **校验自己 IPC 的返回形状**（B03 的真 bug：`invoke` 可能 resolve 成 undefined，
      // 于后续 `.length` 当场抛，把整个 section 挂掉）。
      if (!r || !Array.isArray(r.rows) || !Array.isArray(r.settings_scopes)) {
        throw new Error(
          "后端返回的形状不对（rows / settings_scopes 不是数组）",
        );
      }
      this.last = r;
      this.copyBtn.disabled = false;
      this.render(r);
    } catch (e) {
      this.last = null;
      this.copyBtn.disabled = true;
      this.body.textContent = `扫描失败：${String(e)}`;
      showActionFailureToast("扫描配置面", String(e));
    }
  }

  private render(r: ConfigSurfaceReport): void {
    this.meta.textContent = `HOME=${r.home} · ~/.claude 解析为 ${r.claude_config_dir}`;
    // `KR65D2`：「app 该自带而还没有装口」那一格**在屏幕上数得出来**。
    this.owed.textContent = summarizeOwedInstallers(r.rows) ?? "";
    this.owed.hidden = this.owed.textContent === "";
    this.body.textContent = "";
    let lastTool = "";
    for (const row of r.rows) {
      if (row.tool_id !== lastTool) {
        lastTool = row.tool_id;
        const h = document.createElement("div");
        h.className = "config-surface-tool";
        h.dataset.toolId = row.tool_id;
        h.textContent = row.tool_name;
        const src = document.createElement("span");
        src.className = "config-surface-source";
        src.textContent = ` — ${row.source_label}`;
        h.appendChild(src);
        this.body.appendChild(h);
      }
      this.body.appendChild(this.renderRow(row));
    }

    this.scopesBox.textContent = "";
    for (const s of r.settings_scopes) {
      const st = describeSurfaceState(s.state);
      const el = document.createElement("div");
      el.className = `config-surface-scope tone-${st.tone}`;
      el.dataset.scope = s.scope;
      const head = document.createElement("div");
      head.className = "config-surface-scope-head";
      head.textContent = `[${s.scope}] ${s.path}`;
      el.appendChild(head);
      const detail = document.createElement("div");
      detail.className = "config-surface-scope-detail";
      const hooks =
        s.has_cc_bus_hooks === null
          ? "钩子字样：读不到，不猜"
          : s.has_cc_bus_hooks
            ? "含 cc-bus 钩子字样"
            : "不含 cc-bus 钩子字样";
      detail.textContent = `${st.text} · ${hooks} · ${s.precedence_note}`;
      el.appendChild(detail);
      this.scopesBox.appendChild(el);
    }
  }

  private renderRow(row: SurfaceRow): HTMLElement {
    const st = describeSurfaceState(row.state);
    const el = document.createElement("div");
    el.className = `config-surface-row tone-${st.tone}`;
    el.dataset.path = row.path_declared;
    // 档进 DOM：判据与用户看的是同一份值，不靠措辞猜（`K-R65`）。
    el.dataset.tier = row.tier;

    const p = document.createElement("div");
    p.className = "config-surface-path";
    // T04 审计重要 8：位置收成路径行上的徽章而不是独立一行——10 行里 9 行的"位置"
    // 在「现状」那列已经说过了，独立成行等于凭空多 10 行灰字。
    const host = document.createElement("span");
    host.className = "config-surface-host";
    host.textContent = row.host_label;
    p.appendChild(host);
    p.appendChild(document.createTextNode(row.path_declared));
    if (row.note) {
      const n = document.createElement("span");
      n.className = "config-surface-note";
      n.textContent = `（${row.note}）`;
      p.appendChild(n);
    }
    el.appendChild(p);

    if (row.path_resolved) {
      const rp = document.createElement("div");
      rp.className = "config-surface-resolved";
      rp.textContent = `解析为 ${row.path_resolved}`;
      el.appendChild(rp);
    }

    const eff = document.createElement("div");
    eff.className = "config-surface-effect";
    eff.textContent = row.effect_label;
    el.appendChild(eff);

    const state = document.createElement("div");
    state.className = "config-surface-state";
    state.textContent = st.text;
    el.appendChild(state);

    // 🔴 `KR65D1`：**缺席时这一页真的出声。**
    // ⚠ 它是独立一个元素、带 `data-gap` —— 「这句话上没上屏」这件事因此判得了
    //（T02 审计重要 5 那条教训：纯函数被断言 ≠ 它进了 DOM）。
    const prompt = promptToInstall(row);
    if (prompt) {
      const p = document.createElement("div");
      p.className = "config-surface-prompt";
      p.dataset.gap = gapKindOfState(row.state) ?? "";
      p.textContent = prompt;
      el.appendChild(p);
    }

    const undo = document.createElement("div");
    undo.className = "config-surface-undo";
    undo.textContent = describeUndo(row);
    el.appendChild(undo);

    return el;
  }

  private async copy(): Promise<void> {
    if (!this.last) return;
    try {
      await navigator.clipboard.writeText(formatReportText(this.last));
      this.copyBtn.textContent = "已复制";
      setTimeout(() => {
        this.copyBtn.textContent = "复制诊断文本";
      }, 1500);
    } catch (e) {
      showActionFailureToast("复制诊断文本", String(e));
    }
  }
}
