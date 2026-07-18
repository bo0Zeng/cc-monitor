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
  private machineRow!: HTMLElement;
  private dirRow!: HTMLElement;
  private dirInput!: HTMLInputElement;
  private datalist!: HTMLDataListElement;
  private listBox!: HTMLElement;
  /** F87b②：project scope 加/改表单的输入引用——「编辑」按钮预填用（每次 reload 重建时刷新）。 */
  private addNameInput: HTMLInputElement | null = null;
  private addJsonInput: HTMLTextAreaElement | null = null;
  /** F87b③：当前选中的机器。null = 本机（既有本地读写）；非空 = 远端 origin（只读跨机）。 */
  private origin: string | null = null;

  constructor() {
    this.element = this.build();
    void this.loadProjectCandidates();
    void this.loadMachines(); // F87b③：有远端则显机器选择行
    // 业务二审 gap#6：打开即读（空 dir 也先显 user/local scope），不再是看似坏掉的空框。
    void this.reload();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless mcp-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "读：跨 scope 展示 MCP 服务器（用户 / local / 项目）；配了远端可切「机器」跨机只读看远端 user scope。" +
      "写：只增改删「本机项目 .mcp.json」——绝不动 ~/.claude.json、绝不跨机写。设置窗拿不到当前会话项目，请在下面填/选项目目录。";
    root.appendChild(hint);

    // F87b③：机器选择行（本机 / 各远端 origin）。仅当配了远端时由 loadMachines 填充；否则留空不显。
    this.machineRow = document.createElement("div");
    this.machineRow.className = "settings-row mcp-machine-row";
    root.appendChild(this.machineRow);

    // 项目目录输入 + datalist + 读取
    const row = document.createElement("div");
    row.className = "settings-row mcp-dir-row";
    this.dirRow = row;
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

  /** F87b③：读**后端 canonical 远端 origin**（已去重/空 label 回退 host/丢不完整主机——与
   *  `read_remote_mcp_servers` 解析口径一致，前端不再自行从原始 config 重推）。有则渲染机器选择行。
   *  无远端 → 不显（本机用户零变化）。 */
  private async loadMachines(): Promise<void> {
    let origins: string[] = [];
    try {
      origins = await invoke<string[]>("list_remote_mcp_origins");
    } catch {
      /* 拿不到就当没有远端（本机模式），不影响本地功能 */
    }
    this.machineRow.replaceChildren();
    if (origins.length === 0) return; // 无远端 → 不显选择行
    const label = document.createElement("span");
    label.className = "mcp-machine-label";
    label.textContent = "机器：";
    this.machineRow.appendChild(label);
    const mk = (origin: string | null, text: string): void => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "settings-btn settings-btn-secondary mcp-machine-btn";
      btn.textContent = text;
      btn.dataset.origin = origin ?? ""; // 身份存 dataset（本机=""），active 高亮/切换不靠脆弱的 textContent
      if (origin === this.origin) btn.classList.add("active");
      btn.addEventListener("click", () => void this.selectMachine(origin));
      this.machineRow.appendChild(btn);
    };
    mk(null, "本机");
    for (const o of origins) mk(o, o);
  }

  /** F87b③：切机器。本机 → 恢复本地读写；远端 → 隐藏项目目录行、只读跨机读。 */
  private async selectMachine(origin: string | null): Promise<void> {
    this.origin = origin;
    this.dirRow.style.display = origin === null ? "" : "none"; // 远端无项目目录概念
    const key = origin ?? "";
    for (const btn of this.machineRow.querySelectorAll<HTMLElement>(".mcp-machine-btn")) {
      btn.classList.toggle("active", (btn.dataset.origin ?? "") === key); // 靠 dataset 身份，非 textContent
    }
    if (origin === null) await this.reload();
    else await this.reloadRemote(origin);
  }

  private async reload(): Promise<void> {
    const startOrigin = this.origin; // 捕获调用时机器：await 期间用户切走则丢弃这次本地结果（防旧结果盖新选中）
    const dir = this.currentDir();
    this.listBox.replaceChildren();
    let entries: McpServerEntry[];
    try {
      entries = await invoke<McpServerEntry[]>("read_mcp_servers", { projectDir: dir || null });
    } catch (e) {
      if (this.origin !== startOrigin) return; // 期间已切走 → 静默丢弃
      showActionFailureToast("读取 MCP 配置失败", String(e));
      return;
    }
    if (this.origin !== startOrigin) return; // 期间已切走 → 丢弃这次结果，不盖当前选中
    this.renderList(entries, dir, false);
  }

  /** F87b③：跨机只读读远端 user scope（机器全局 MCP）。失败弹 toast、不改本地。 */
  private async reloadRemote(origin: string): Promise<void> {
    this.listBox.replaceChildren();
    const loading = document.createElement("div");
    loading.className = "settings-hint";
    loading.textContent = `读取远端 [${origin}] 的 MCP 配置…`;
    this.listBox.appendChild(loading);
    let entries: McpServerEntry[];
    try {
      entries = await invoke<McpServerEntry[]>("read_remote_mcp_servers", { origin });
    } catch (e) {
      if (this.origin !== origin) return; // 期间已切走 → 丢弃
      this.listBox.replaceChildren();
      showActionFailureToast(`读取远端 [${origin}] MCP 失败`, String(e));
      return;
    }
    if (this.origin !== origin) return; // 期间已切走 → 丢弃这次结果
    this.renderList(entries, "", true);
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = `远端 [${origin}] 无 user scope MCP 配置（或 ~/.claude.json 缺失）。`;
      this.listBox.appendChild(empty);
    }
  }

  /** 渲染分组列表。remote=true：全只读、只显有条目的 scope、无加/改表单（跨机 user scope 只读）。 */
  private renderList(entries: McpServerEntry[], dir: string, remote: boolean): void {
    this.listBox.replaceChildren();
    const grouped = groupByScope(entries);
    for (const scope of ["user", "local", "project"] as McpScope[]) {
      if (remote && grouped[scope].length === 0) continue;
      this.listBox.appendChild(this.renderScope(scope, grouped[scope], dir, remote));
    }
  }

  private renderScope(
    scope: McpScope,
    entries: McpServerEntry[],
    dir: string,
    remote: boolean,
  ): HTMLElement {
    // 可写 = 本机 + project scope + 已填目录（远端一律只读，守 SS-14 不跨机写）。
    const writable = !remote && scope === "project" && !!dir;
    const box = document.createElement("div");
    box.className = "mcp-scope";
    const title = document.createElement("div");
    title.className = "settings-group-title";
    const readOnlySuffix = remote ? " · 只读（远端）" : scope === "project" ? "" : " · 只读";
    title.textContent = `${SCOPE_LABEL[scope]}（${entries.length}）${readOnlySuffix}`;
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

      // F87b②：编辑入口——**仅本机 project scope**（SS-14 写只本机 .mcp.json；远端一律只读）。预填加/改表单，复用既有写路径（同名覆盖）。
      if (writable) {
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

    // 项目 scope 加/改表单（名 + server JSON）——**仅本机**（远端只读，不出写表单，SS-14 不跨机写）。
    if (scope === "project" && !remote) {
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
