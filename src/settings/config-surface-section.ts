// T02：**配置面审计视图** —— 「cc-monitor 到底动过你哪些文件」。
//
// 这一页**只读**，而且是**按需读一次**（无轮询，红线）。它把
// `src-tauri/src/tool_registry.rs` 的 `TOOLS` 遍历成一张表：每个受管工具碰哪些文件、
// 对它做什么、现在是什么状态、还能不能撤。
//
// **一条硬纪律来自后端，前端不许在这里放水**：查不了的东西显示成「未确定 + 为什么」，
// **绝不显示成"缺失"**。远端路径、相对项目目录的 `.mcp.json`、Windows 侧 `$PROFILE`
// 本机都查不到，把它们画成红叉就是对能用的安装报假警报——B04 审计已经抓过一次同型病。
import { invoke } from "@tauri-apps/api/core";
import { showActionFailureToast } from "../error-toast";

/** 与 Rust 侧 `SurfaceState` 的 `#[serde(tag = "kind", rename_all = "snake_case")]` 对位。 */
export type SurfaceState =
  | { kind: "present"; detail: string }
  | { kind: "absent" }
  | { kind: "undetermined"; why: string };

export interface SurfaceRow {
  tool_id: string;
  tool_name: string;
  source_label: string;
  path_declared: string;
  path_resolved: string | null;
  note: string | null;
  host_label: string;
  effect_label: string;
  state: SurfaceState;
  installable: boolean;
  uninstallable: boolean;
}

export interface SettingsScope {
  scope: string;
  path: string;
  state: SurfaceState;
  has_cc_bus_hooks: boolean | null;
  precedence_note: string;
}

export interface ConfigSurfaceReport {
  rows: SurfaceRow[];
  settings_scopes: SettingsScope[];
  claude_config_dir: string;
  home: string;
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

/** 「能否撤」列。`uninstallable=false` 时**不给按钮**——本工作区不做点了没反应的按钮。 */
export function describeUndo(row: SurfaceRow): string {
  if (row.uninstallable) return "可按围栏/整文件撤销（在对应工具的部署入口里）";
  if (!row.installable) return "尚未支持部署，也就无所谓撤销";
  return "暂无自动撤销；如需清理请按上面的路径手动处理";
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
      const r = await invoke<ConfigSurfaceReport>("config_surface_report");
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

    const p = document.createElement("div");
    p.className = "config-surface-path";
    p.textContent = row.path_declared;
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

    // T04：先说清"在哪台机器上"——`$PROFILE` 与 `~/.local/bin/ccm` 长得都像本机路径，
    // 不标明的话用户根本分不出。
    const host = document.createElement("div");
    host.className = "config-surface-host";
    host.textContent = `位置：${row.host_label}`;
    el.appendChild(host);

    const eff = document.createElement("div");
    eff.className = "config-surface-effect";
    eff.textContent = row.effect_label;
    el.appendChild(eff);

    const state = document.createElement("div");
    state.className = "config-surface-state";
    state.textContent = st.text;
    el.appendChild(state);

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
