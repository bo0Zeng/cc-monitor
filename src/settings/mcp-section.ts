/**
 * F87（#50+#51）：MCP 管理（集成组内一节）。**SS-14 读写分界**：
 * - **读**：跨 scope 展示（用户 / local / 项目），后端 `read_mcp_servers` 宽容读三处。**用户/local scope 只读**。
 * - **写**：**只**项目 scope——增/改/删该项目 `<dir>/.mcp.json`，后端 `write_project_mcp_server`/`remove_project_mcp_server`
 *   硬编码只碰 `.mcp.json`（绝不写 `~/.claude.json`/`settings.json`）。
 *
 * 设置窗独立于主窗口、拿不到活跃会话 cwd → 用项目目录输入框（datalist 从 `list_mcp_project_dirs` 自动补全「用过的项目」）。
 * 纯函数（groupByScope / serverSummary / parseServerConfig）零 import，node 可测。
 */
import { invoke } from "@tauri-apps/api/core";
import { showActionFailureToast } from "../error-toast";

export type McpScope = "user" | "local" | "project";
export interface McpServerEntry {
  scope: McpScope;
  name: string;
  server: unknown; // 原样保留（宽容）
  sourcePath: string;
}

/** 按 scope 分组（保序）。纯函数。 */
export function groupByScope(entries: McpServerEntry[]): Record<McpScope, McpServerEntry[]> {
  const g: Record<McpScope, McpServerEntry[]> = { user: [], local: [], project: [] };
  for (const e of entries) {
    if (e.scope === "user" || e.scope === "local" || e.scope === "project") g[e.scope].push(e);
  }
  return g;
}

/** 一行摘要：远程型 `<type> · <url>`；stdio 型 `stdio · <command> <args>`；否则「(未知形态)」。纯函数。 */
export function serverSummary(server: unknown): string {
  if (server && typeof server === "object") {
    const s = server as { type?: unknown; url?: unknown; command?: unknown; args?: unknown };
    if (typeof s.url === "string" && s.url) {
      const t = typeof s.type === "string" && s.type ? s.type : "http";
      return `${t} · ${s.url}`;
    }
    if (typeof s.command === "string" && s.command) {
      const args = Array.isArray(s.args) ? s.args.filter((a) => typeof a === "string").join(" ") : "";
      return `stdio · ${s.command}${args ? " " + args : ""}`;
    }
  }
  return "(未知形态)";
}

/** 解析 server 配置 JSON 文本：必须是对象。纯函数。 */
export function parseServerConfig(text: string): { ok: true; value: unknown } | { ok: false; error: string } {
  const t = text.trim();
  if (!t) return { ok: false, error: "配置为空" };
  let v: unknown;
  try {
    v = JSON.parse(t);
  } catch (e) {
    return { ok: false, error: `JSON 无效：${e instanceof Error ? e.message : String(e)}` };
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) return { ok: false, error: "配置必须是 JSON 对象" };
  return { ok: true, value: v };
}

const SCOPE_LABEL: Record<McpScope, string> = { user: "用户", local: "本项目(local)", project: "项目 .mcp.json" };

export class McpSection {
  readonly element: HTMLElement;
  private dirInput!: HTMLInputElement;
  private datalist!: HTMLDataListElement;
  private listBox!: HTMLElement;
  /** F87b②：project scope 加/改表单的输入引用——「编辑」按钮预填用（每次 reload 重建时刷新）。 */
  private addNameInput: HTMLInputElement | null = null;
  private addJsonInput: HTMLTextAreaElement | null = null;

  constructor() {
    this.element = this.build();
    void this.loadProjectCandidates();
    // 业务二审 gap#6：打开即读（空 dir 也先显 user/local scope），不再是看似坏掉的空框。
    void this.reload();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless mcp-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "读：跨 scope 展示 MCP 服务器（用户 / local / 项目）。写：只增改删「项目 .mcp.json」——" +
      "绝不动 ~/.claude.json。设置窗拿不到当前会话项目，请在下面填/选项目目录。";
    root.appendChild(hint);

    // 项目目录输入 + datalist + 读取
    const row = document.createElement("div");
    row.className = "settings-row";
    this.dirInput = document.createElement("input");
    this.dirInput.className = "settings-input";
    this.dirInput.placeholder = "项目目录（含 .mcp.json 的项目根）";
    this.dirInput.setAttribute("list", "mcp-project-dirs");
    this.datalist = document.createElement("datalist");
    this.datalist.id = "mcp-project-dirs";
    const readBtn = document.createElement("button");
    readBtn.type = "button";
    readBtn.className = "settings-btn settings-btn-secondary";
    readBtn.textContent = "读取";
    readBtn.addEventListener("click", () => void this.reload());
    this.dirInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void this.reload();
    });
    row.append(this.dirInput, this.datalist, readBtn);
    root.appendChild(row);

    this.listBox = document.createElement("div");
    this.listBox.className = "mcp-list";
    root.appendChild(this.listBox);

    return root;
  }

  private async loadProjectCandidates(): Promise<void> {
    try {
      const dirs = await invoke<string[]>("list_mcp_project_dirs");
      this.datalist.replaceChildren();
      for (const d of dirs) {
        const opt = document.createElement("option");
        opt.value = d;
        this.datalist.appendChild(opt);
      }
    } catch {
      /* 候选补全拿不到不影响手填 */
    }
  }

  private currentDir(): string {
    return this.dirInput.value.trim();
  }

  private async reload(): Promise<void> {
    const dir = this.currentDir();
    this.listBox.replaceChildren();
    let entries: McpServerEntry[];
    try {
      entries = await invoke<McpServerEntry[]>("read_mcp_servers", { projectDir: dir || null });
    } catch (e) {
      showActionFailureToast("读取 MCP 配置失败", String(e));
      return;
    }
    const grouped = groupByScope(entries);
    for (const scope of ["user", "local", "project"] as McpScope[]) {
      this.listBox.appendChild(this.renderScope(scope, grouped[scope], dir));
    }
  }

  private renderScope(scope: McpScope, entries: McpServerEntry[], dir: string): HTMLElement {
    const box = document.createElement("div");
    box.className = "mcp-scope";
    const title = document.createElement("div");
    title.className = "settings-group-title";
    title.textContent = `${SCOPE_LABEL[scope]}（${entries.length}）${scope === "project" ? "" : " · 只读"}`;
    box.appendChild(title);

    for (const e of entries) {
      const item = document.createElement("div");
      item.className = "mcp-server-item";
      const rowEl = document.createElement("div");
      rowEl.className = "mcp-server-row";
      const name = document.createElement("span");
      name.className = "mcp-server-name";
      name.textContent = e.name;
      const summary = document.createElement("span");
      summary.className = "mcp-server-summary";
      summary.textContent = serverSummary(e.server);
      rowEl.append(name, summary);

      // F87b①：看完整 JSON——「JSON」折叠钮切换下方详情（含完整配置 + 来源文件路径，诊断用）。
      const detail = document.createElement("div");
      detail.className = "mcp-server-detail is-collapsed";
      const src = document.createElement("div");
      src.className = "mcp-server-src";
      src.textContent = `来源：${e.sourcePath}`;
      const pre = document.createElement("pre");
      pre.className = "mcp-server-json";
      pre.textContent = JSON.stringify(e.server, null, 2);
      detail.append(src, pre);

      const jsonBtn = document.createElement("button");
      jsonBtn.type = "button";
      jsonBtn.className = "settings-btn settings-btn-secondary mcp-json-toggle";
      jsonBtn.textContent = "JSON";
      jsonBtn.title = "看完整配置 JSON + 来源文件";
      jsonBtn.addEventListener("click", () => {
        const collapsed = detail.classList.toggle("is-collapsed");
        jsonBtn.classList.toggle("active", !collapsed);
      });
      rowEl.appendChild(jsonBtn);

      // F87b②：编辑入口——**仅 project scope**（SS-14 写只 .mcp.json）。预填加/改表单，复用既有写路径（同名覆盖）。
      if (scope === "project" && dir) {
        const edit = document.createElement("button");
        edit.type = "button";
        edit.className = "settings-btn settings-btn-secondary mcp-edit";
        edit.textContent = "编辑";
        edit.title = "把这条填进下方表单编辑后保存（覆盖同名）";
        edit.addEventListener("click", () => {
          if (this.addNameInput && this.addJsonInput) {
            this.addNameInput.value = e.name;
            this.addJsonInput.value = JSON.stringify(e.server, null, 2);
            this.addJsonInput.focus();
            this.addJsonInput.scrollIntoView({ block: "nearest" });
          }
        });
        rowEl.appendChild(edit);

        const del = document.createElement("button");
        del.type = "button";
        del.className = "settings-btn settings-btn-danger mcp-del";
        del.textContent = "删";
        del.addEventListener("click", () => void this.removeEntry(dir, e.name));
        rowEl.appendChild(del);
      }
      item.append(rowEl, detail);
      box.appendChild(item);
    }

    // 项目 scope：加/改表单（名 + server JSON）。需先填项目目录。
    if (scope === "project") {
      box.appendChild(this.renderAddForm(dir));
    }
    return box;
  }

  private renderAddForm(dir: string): HTMLElement {
    const form = document.createElement("div");
    form.className = "mcp-add-form";
    const nameInput = document.createElement("input");
    nameInput.className = "settings-input";
    nameInput.placeholder = "server 名";
    const jsonInput = document.createElement("textarea");
    jsonInput.className = "settings-input mcp-json-input";
    jsonInput.placeholder = '{ "command": "npx", "args": ["-y", "@x/mcp"] }  或  { "type": "http", "url": "https://…" }';
    jsonInput.rows = 3;
    // F87b②：暴露引用给「编辑」按钮预填（每次 reload 重建表单时刷新）。
    this.addNameInput = nameInput;
    this.addJsonInput = jsonInput;
    const saveBtn = document.createElement("button");
    saveBtn.type = "button";
    saveBtn.className = "settings-btn";
    saveBtn.textContent = "添加/更新到 .mcp.json";
    if (!dir) {
      saveBtn.disabled = true;
      saveBtn.title = "先填项目目录并「读取」";
    }
    saveBtn.addEventListener("click", () => {
      const name = nameInput.value.trim();
      if (!name) {
        showActionFailureToast("缺 server 名", "填一个 MCP server 名再保存。", { level: "info" });
        return;
      }
      const parsed = parseServerConfig(jsonInput.value);
      if (!parsed.ok) {
        showActionFailureToast("配置无效", parsed.error, { level: "info" });
        return;
      }
      void this.writeEntry(dir, name, parsed.value);
    });
    form.append(nameInput, jsonInput, saveBtn);
    return form;
  }

  private async writeEntry(dir: string, name: string, server: unknown): Promise<void> {
    try {
      await invoke("write_project_mcp_server", { projectDir: dir, name, server });
      this.dirInput.value = dir; // 同步输入框到刚写的 dir，reload 读同一 dir（防用户中途改输入致列表错位）
      await this.reload();
    } catch (e) {
      showActionFailureToast("写入 .mcp.json 失败", String(e));
    }
  }

  private async removeEntry(dir: string, name: string): Promise<void> {
    if (!window.confirm(`从项目 .mcp.json 删除 MCP server「${name}」？`)) return;
    try {
      await invoke("remove_project_mcp_server", { projectDir: dir, name });
      this.dirInput.value = dir; // 同上，reload 读同一 dir
      await this.reload();
    } catch (e) {
      showActionFailureToast("删除失败", String(e));
    }
  }
}
