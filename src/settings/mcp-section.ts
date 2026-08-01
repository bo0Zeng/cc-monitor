/**
 * F87（#50+#51）：MCP 管理（集成组内一节）。**SS-14 读写分界**：
 * - **读**：跨 scope 展示（用户 / local / 项目），后端 `read_mcp_servers` 宽容读三处。**用户/local scope 只读**。
 * - **写**：**只**项目 scope——增/改/删该项目 `<dir>/.mcp.json`，后端 `write_project_mcp_server`/`remove_project_mcp_server`
 *   硬编码只碰 `.mcp.json`（绝不写 `~/.claude.json`/`settings.json`）。
 *
 * 设置窗独立于主窗口、拿不到活跃会话 cwd → 用项目目录输入框（datalist 从 `list_mcp_project_dirs` 自动补全「用过的项目」）。
 * 纯函数（groupByScope / serverSummary / parseServerConfig）零 import，node 可测。
 */
import { subscribeMachine } from "./machine-context";
import { commands } from "../ipc/commands";
import { showActionFailureToast } from "../error-toast";

export type McpScope = "user" | "local" | "project";
// C04d 批 5b：改用生成物（源 `mcp.rs`）。
//
// **一处方向选择 + 一处对我自己的订正**：Rust 侧 `scope` 是 `String`，
// 而手写版把它窄化成 `McpScope = "user" | "local" | "project"`。
//
// 我一开始推断「来了第四种 scope 会 `undefined.push` 抛」——**那是错的**。
// `groupByScope` 的实现里有**显式三值判断**，未知 scope 被跳过、**从不抛**；
// 它的 vitest 注释也明写「测未知 scope 被忽略」。**运行时一直是对的。**
//
// 真正的问题是：手写的窄 union 让那条测试**必须挂一个 `@ts-expect-error`**
// 才能构造一个**真实会从线上来的** entry ——
// **是类型在逼测试撒谎，而运行时早就处理好了这个情况。**
// 换成生成物（`scope: string`，与线上一致）后那个抑制不再需要，已删。
//
// `McpScope` 保留：它是 TS 侧的**域细化**，`groupByScope` 的返回类型用它是对的
// （分组结果确实只有三档）。运行时**逐字节不变**。
import type { McpServerEntry } from "../generated/McpServerEntry";

export type { McpServerEntry };

/** 按 scope 分组（保序）。纯函数。 */
export function groupByScope(
  entries: McpServerEntry[],
): Record<McpScope, McpServerEntry[]> {
  const g: Record<McpScope, McpServerEntry[]> = {
    user: [],
    local: [],
    project: [],
  };
  for (const e of entries) {
    if (e.scope === "user" || e.scope === "local" || e.scope === "project")
      g[e.scope].push(e);
  }
  return g;
}

/** 一行摘要：远程型 `<type> · <url>`；stdio 型 `stdio · <command> <args>`；否则「(未知形态)」。纯函数。 */
export function serverSummary(server: unknown): string {
  if (server && typeof server === "object") {
    const s = server as {
      type?: unknown;
      url?: unknown;
      command?: unknown;
      args?: unknown;
    };
    if (typeof s.url === "string" && s.url) {
      const t = typeof s.type === "string" && s.type ? s.type : "http";
      return `${t} · ${s.url}`;
    }
    if (typeof s.command === "string" && s.command) {
      const args = Array.isArray(s.args)
        ? s.args.filter((a) => typeof a === "string").join(" ")
        : "";
      return `stdio · ${s.command}${args ? " " + args : ""}`;
    }
  }
  return "(未知形态)";
}

/** 解析 server 配置 JSON 文本：必须是对象。纯函数。 */
export function parseServerConfig(
  text: string,
): { ok: true; value: unknown } | { ok: false; error: string } {
  const t = text.trim();
  if (!t) return { ok: false, error: "配置为空" };
  let v: unknown;
  try {
    v = JSON.parse(t);
  } catch (e) {
    return {
      ok: false,
      error: `JSON 无效：${e instanceof Error ? e.message : String(e)}`,
    };
  }
  if (!v || typeof v !== "object" || Array.isArray(v))
    return { ok: false, error: "配置必须是 JSON 对象" };
  return { ok: true, value: v };
}

/** F89b：确定性 JSON（递归排序对象键）——供 catalog dedup，防同配置不同键序被判成两条。纯函数。 */
export function stableStringify(v: unknown): string {
  if (v === null || typeof v !== "object") return JSON.stringify(v) ?? "null";
  if (Array.isArray(v)) return `[${v.map(stableStringify).join(",")}]`;
  const obj = v as Record<string, unknown>;
  const keys = Object.keys(obj).sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${stableStringify(obj[k])}`).join(",")}}`;
}

/** F89b：统一目录（库）的去重键 = 名 + \0 + 确定性 server JSON。纯函数。 */
export function catalogKey(name: string, server: unknown): string {
  return `${name}\u0000${stableStringify(server)}`;
}

const SCOPE_LABEL: Record<McpScope, string> = {
  user: "用户",
  local: "本项目(local)",
  project: "项目 .mcp.json",
};

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
  /** F87b-fix：编辑态横幅（「编辑中 X · 取消」）——编辑时锁名，防改名静默重复。 */
  private editBanner: HTMLElement | null = null;
  private editNameLabel: HTMLElement | null = null;
  /** F87b③：当前选中的机器。null = 本机（既有本地读写）；非空 = 远端 origin（只读跨机）。 */
  private origin: string | null = null;
  /** F89b：统一目录（库）——会话内读到过的所有 distinct server（键=catalogKey），供一键注册进项目。累积不清（有清空钮）。 */
  private catalog = new Map<string, { name: string; server: unknown }>();

  constructor() {
    this.element = this.build();
    // S4a：跟随共用的「当前在看哪台机器」store。本分节是四块里**唯一**能表示「本机」的
    // （它的机器行第一颗按钮就是本机），所以 null 也照单全收。
    // `selectMachine` 自带「同值早退」，与 store 的「同值不通知」两道去重叠加，
    // 不会因为往返而多打一次 ssh。
    subscribeMachine((origin) => void this.selectMachine(origin));
    void this.loadProjectCandidates();
    this.loadMachines(); // E59：只渲染「在看哪台」那一行（选择按钮已删）
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
    readBtn.addEventListener("click", () => void this.refresh());
    this.dirInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void this.refresh();
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
      const dirs = await commands.list_mcp_project_dirs();
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

  /**
   * 渲染「你在看哪台机器」那一行。
   *
   * **E59 之前它叫「读远端清单并渲染机器选择按钮」** —— 选择按钮删掉之后，
   * 远端清单在这里就没有消费者了（`origin` 只来自共用 store，本分节不再自己挑）。
   * 所以那次 `list_remote_mcp_origins` 调用**一并删掉**：留着就是一次没人用的 IPC，
   * 而它是要连 SSH 的。
   */
  private loadMachines(): void {
    // E59：**这一整行「机器：本机 / aya / nano」按钮已删。**
    //
    // 本分节只作为机器详情页上的一块存在（`panel.ts` 的 `perMachineBlocks`，唯一构造点），
    // 页头已经说了在看哪台。留着这排按钮 = 两层上下文，而写动作按分节自己的 `this.origin`
    // 定目标（`:699` 的 `const startOrigin = this.origin`）⇒ **在标着 A 的页面上把 MCP
    // 服务器写进 B**，`router.activeId` 仍是 A、界面上看不出来。
    //
    // 「删」而不是「藏」是用户 2026-08-01 拍板的。⇒ `origin` 只能来自共用 store。
    // 行本身留着（`machineRow`）当「你在看哪台」的只读显示 —— 本分节**本机与远端都有意义**，
    // 两种模式下的读写面不同（本机可写 user/local/项目；远端只读 user scope + 可写项目），
    // 所以还是得让用户看见现在是哪一种。
    this.machineRow.replaceChildren();
    const label = document.createElement("span");
    label.className = "mcp-machine-label";
    label.textContent = "机器：";
    this.machineRow.appendChild(label);
    const name = document.createElement("span");
    name.className = "mcp-machine-name";
    name.textContent = this.origin ?? "本机";
    this.machineRow.appendChild(name);
  }

  /** F87b③/F89a：切机器。本机 → 本地读写；远端 → **项目目录行也显**（F89a：填项目=远端项目 .mcp.json 可写；
   *  空=远端 user scope 只读）。切机器清空目录（本机/远端项目路径不通用）+ 换 datalist 候选。
   *  F87b-fix：已是当前机器 → 早退（防误双击并发 SSH）。 */
  private async selectMachine(origin: string | null): Promise<void> {
    if (origin === this.origin) return;
    this.origin = origin;
    this.dirRow.style.display = ""; // F89a：远端也显目录行（可填项目管理远端 .mcp.json）
    this.dirInput.value = ""; // 本机/远端项目路径不通用，切机器清空
    // E59：按钮没了，改成更新那行只读显示。
    const name = this.machineRow.querySelector<HTMLElement>(".mcp-machine-name");
    if (name) name.textContent = origin ?? "本机";
    if (origin === null) void this.loadProjectCandidates();
    else void this.loadRemoteProjectCandidates(origin);
    await this.refresh();
  }

  /** F89a：统一刷新入口，按 (机器, 目录) 三态分发。 */
  private async refresh(): Promise<void> {
    if (this.origin === null) return this.reload(); // 本机
    const dir = this.currentDir();
    if (dir) return this.reloadRemoteProject(this.origin, dir); // 远端项目（可写）
    return this.reloadRemote(this.origin); // 远端 user scope（只读）
  }

  /** F89a：读远端某项目的 datalist 候选（远端 `~/.claude.json` projects 键）。 */
  private async loadRemoteProjectCandidates(origin: string): Promise<void> {
    let dirs: string[] = [];
    try {
      dirs = await commands.list_remote_mcp_project_dirs({ origin });
    } catch {
      /* 拿不到不影响手填 */
    }
    if (this.origin !== origin) return; // 期间切走
    this.datalist.replaceChildren();
    for (const d of dirs) {
      const opt = document.createElement("option");
      opt.value = d;
      this.datalist.appendChild(opt);
    }
  }

  /** F89a：读+管理远端某项目的 `.mcp.json`（project scope 可写）。切走/改目录 → 丢弃。 */
  private async reloadRemoteProject(
    origin: string,
    dir: string,
  ): Promise<void> {
    this.listBox.replaceChildren();
    const loading = document.createElement("div");
    loading.className = "settings-hint mcp-loading";
    loading.textContent = `读取远端 [${origin}] 项目 ${dir} 的 .mcp.json…（SSH）`;
    this.listBox.appendChild(loading);
    let entries: McpServerEntry[];
    try {
      entries = await commands.read_remote_project_mcp({
        origin,
        projectDir: dir,
      });
    } catch (e) {
      if (this.origin !== origin || this.currentDir() !== dir) return;
      this.listBox.replaceChildren();
      const box = document.createElement("div");
      box.className = "mcp-remote-error";
      const line = document.createElement("div");
      line.textContent = `读取远端 [${origin}] 项目 .mcp.json 失败：${String(e)}`;
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "settings-btn settings-btn-secondary";
      retry.textContent = "重试";
      retry.addEventListener(
        "click",
        () => void this.reloadRemoteProject(origin, dir),
      );
      box.append(line, retry);
      this.listBox.appendChild(box);
      return;
    }
    if (this.origin !== origin || this.currentDir() !== dir) return; // 切走/改目录 → 丢弃
    this.renderList(entries, dir, true);
    const head = document.createElement("div");
    head.className = "mcp-remote-head";
    const note = document.createElement("span");
    note.className = "settings-hint";
    note.textContent = `远端 [${origin}] 项目 ${dir} · 可增/改/删（写远端 .mcp.json，SS-14）。`;
    this.listBox.prepend(head);
    head.appendChild(note);
  }

  private async reload(): Promise<void> {
    const startOrigin = this.origin; // 捕获调用时机器：await 期间用户切走则丢弃这次本地结果（防旧结果盖新选中）
    const dir = this.currentDir();
    this.listBox.replaceChildren();
    let entries: McpServerEntry[];
    try {
      entries = await commands.read_mcp_servers({
        projectDir: dir || null,
      });
    } catch (e) {
      if (this.origin !== startOrigin) return; // 期间已切走 → 静默丢弃
      showActionFailureToast("读取 MCP 配置失败", String(e));
      return;
    }
    if (this.origin !== startOrigin) return; // 期间已切走 → 丢弃这次结果，不盖当前选中
    this.renderList(entries, dir, false);
  }

  /** F87b③：跨机只读读远端 user scope（机器全局 MCP）。切走的旧结果由 origin 守卫丢弃。
   *  F87b-fix：失败 → **常驻内联错误 + 重试**（原仅 5s toast，消失后留白与「无配置」不可区分）；
   *  加远端头（user-scope-only 说明 + 「重新读取」，因远端下项目目录行的「读取」钮已隐藏）。 */
  private async reloadRemote(origin: string): Promise<void> {
    this.listBox.replaceChildren();
    const loading = document.createElement("div");
    loading.className = "settings-hint mcp-loading";
    loading.textContent = `读取远端 [${origin}] 的 MCP 配置…（走 SSH，可能数秒）`;
    this.listBox.appendChild(loading);
    let entries: McpServerEntry[];
    try {
      entries = await commands.read_remote_mcp_servers({
        origin,
      });
    } catch (e) {
      if (this.origin !== origin) return; // 期间已切走 → 丢弃
      this.renderRemoteError(origin, String(e));
      return;
    }
    if (this.origin !== origin) return; // 期间已切走 → 丢弃这次结果
    this.renderList(entries, "", true);
    this.listBox.prepend(this.buildRemoteHeader(origin)); // 头：仅 user scope 说明 + 重新读取
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = `远端 [${origin}] 无 user scope MCP 配置（或 ~/.claude.json 缺失）。`;
      this.listBox.appendChild(empty);
    }
  }

  /** F87b-fix：远端读失败 → 常驻内联错误 + 重试钮（区分「读失败」vs「远端无配置」）。 */
  private renderRemoteError(origin: string, msg: string): void {
    this.listBox.replaceChildren();
    const box = document.createElement("div");
    box.className = "mcp-remote-error";
    const line = document.createElement("div");
    line.textContent = `读取远端 [${origin}] MCP 失败：${msg}`;
    box.appendChild(line);
    const retry = document.createElement("button");
    retry.type = "button";
    retry.className = "settings-btn settings-btn-secondary";
    retry.textContent = "重试";
    retry.addEventListener("click", () => void this.reloadRemote(origin));
    box.appendChild(retry);
    this.listBox.appendChild(box);
  }

  /** F87b-fix：远端列表头——标明跨机只取 user scope（项目/local 不跨机取）+ 「重新读取」入口
   *  （远端下项目目录行的「读取」钮已隐藏，否则重读远端无可见入口）。 */
  private buildRemoteHeader(origin: string): HTMLElement {
    const head = document.createElement("div");
    head.className = "mcp-remote-head";
    const note = document.createElement("span");
    note.className = "settings-hint";
    note.textContent =
      "跨机 user scope（机器全局）MCP · 只读。要管理远端**项目级** .mcp.json：在上方项目目录填/选远端项目路径。";
    const refresh = document.createElement("button");
    refresh.type = "button";
    refresh.className =
      "settings-btn settings-btn-secondary mcp-remote-refresh";
    refresh.textContent = "重新读取";
    refresh.addEventListener("click", () => void this.reloadRemote(origin));
    head.append(note, refresh);
    return head;
  }

  /** 渲染分组列表。remote 模式跳过空 scope（user scope 只读噪音）——**但可写的远端项目 scope 即使空也渲染**
   *  （F89a 审计修·阻塞：否则新/空远端项目不出加表单，无法建第一条 server）。
   *  F89b：读到的 server 累进库；列表尾 append 库区（可一键注册进当前项目）。 */
  private renderList(
    entries: McpServerEntry[],
    dir: string,
    remote: boolean,
  ): void {
    for (const e of entries) {
      this.catalog.set(catalogKey(e.name, e.server), {
        name: e.name,
        server: e.server,
      });
    }
    this.listBox.replaceChildren();
    const grouped = groupByScope(entries);
    for (const scope of ["user", "local", "project"] as McpScope[]) {
      const writableProject = scope === "project" && !!dir; // 可写项目 scope 恒渲染（带加表单）
      if (remote && grouped[scope].length === 0 && !writableProject) continue;
      this.listBox.appendChild(
        this.renderScope(scope, grouped[scope], dir, remote),
      );
    }
    // F89b：库区（尾部）。注册目标 = 当前机器(this.origin)+当前项目目录(dir)。已在本项目的条目标注、不重复注册。
    const projectKeys = new Set(
      grouped.project.map((e) => catalogKey(e.name, e.server)),
    );
    const cat = this.renderCatalog(dir, projectKeys);
    if (cat) this.listBox.appendChild(cat);
  }

  /** F89b：统一目录（库）区。空库 → null（不显）。可折叠。每条：已在本项目→标注；否则有可写目标→注册钮。 */
  private renderCatalog(
    dir: string,
    projectKeys: Set<string>,
  ): HTMLElement | null {
    if (this.catalog.size === 0) return null;
    const box = document.createElement("div");
    box.className = "mcp-scope mcp-catalog";

    const head = document.createElement("div");
    head.className = "settings-group-title mcp-catalog-head";
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "settings-btn settings-btn-secondary mcp-catalog-toggle";
    const title = document.createElement("span");
    title.textContent = `库（${this.catalog.size} 个见过的 MCP server）`;
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "settings-btn settings-btn-secondary";
    clear.textContent = "清空库";
    clear.addEventListener("click", () => {
      this.catalog.clear();
      void this.refresh();
    });
    const body = document.createElement("div");
    body.className = "mcp-catalog-body";
    toggle.textContent = "▾";
    toggle.addEventListener("click", () => {
      const hidden = body.classList.toggle("is-collapsed");
      toggle.textContent = hidden ? "▸" : "▾";
    });
    head.append(toggle, title, clear);
    box.appendChild(head);

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent = dir
      ? `注册目标：${this.origin === null ? "本机" : `远端 [${this.origin}]`} 项目 ${dir}`
      : "注册到项目需先在上方填项目目录（作为注册目标）。";
    body.appendChild(hint);

    for (const { name, server } of this.catalog.values()) {
      const key = catalogKey(name, server);
      const row = document.createElement("div");
      row.className = "mcp-server-row";
      const nameEl = document.createElement("span");
      nameEl.className = "mcp-server-name";
      nameEl.textContent = name;
      const summary = document.createElement("span");
      summary.className = "mcp-server-summary";
      summary.textContent = serverSummary(server);
      row.append(nameEl, summary);
      if (projectKeys.has(key)) {
        const mark = document.createElement("span");
        mark.className = "mcp-catalog-here";
        mark.textContent = "✓ 已在本项目";
        row.appendChild(mark);
      } else {
        const reg = document.createElement("button");
        reg.type = "button";
        reg.className = "settings-btn settings-btn-secondary mcp-catalog-reg";
        reg.textContent = "注册到此项目";
        if (!dir) {
          reg.disabled = true;
          reg.title = "先填项目目录作注册目标";
        } else {
          reg.addEventListener(
            "click",
            () => void this.writeEntry(dir, name, server),
          );
        }
        row.appendChild(reg);
      }
      body.appendChild(row);
    }
    box.appendChild(body);
    return box;
  }

  private renderScope(
    scope: McpScope,
    entries: McpServerEntry[],
    dir: string,
    remote: boolean,
  ): HTMLElement {
    // F89a：可写 = project scope + 已填目录（**本机或远端**——远端项目写走 SFTP，写面仍只 .mcp.json，SS-14）。
    // user/local scope 恒只读（SS-14：绝不写 ~/.claude.json）。
    const writable = scope === "project" && !!dir;
    const box = document.createElement("div");
    box.className = "mcp-scope";
    const title = document.createElement("div");
    title.className = "settings-group-title";
    const readOnlySuffix = writable
      ? ""
      : remote
        ? " · 只读（远端）"
        : " · 只读";
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
      // F87b-fix：懒建——`JSON.stringify` 推迟到首次展开（原每行急切 stringify，即便折叠、多 server 时白算）。
      const detail = document.createElement("div");
      detail.className = "mcp-server-detail is-collapsed";
      const src = document.createElement("div");
      src.className = "mcp-server-src";
      src.textContent = `来源：${e.sourcePath}`;
      const pre = document.createElement("pre");
      pre.className = "mcp-server-json";
      detail.append(src, pre);
      let jsonBuilt = false;

      const jsonBtn = document.createElement("button");
      jsonBtn.type = "button";
      jsonBtn.className = "settings-btn settings-btn-secondary mcp-json-toggle";
      jsonBtn.textContent = "JSON";
      jsonBtn.title = "看完整配置 JSON + 来源文件";
      jsonBtn.addEventListener("click", () => {
        const collapsed = detail.classList.toggle("is-collapsed");
        if (!collapsed && !jsonBuilt) {
          pre.textContent = JSON.stringify(e.server, null, 2); // 首次展开才算
          jsonBuilt = true;
        }
        jsonBtn.classList.toggle("active", !collapsed);
      });
      rowEl.appendChild(jsonBtn);

      // F87b②：编辑入口——**仅本机 project scope**（SS-14 写只本机 .mcp.json；远端一律只读）。预填加/改表单，复用既有写路径（同名覆盖）。
      if (writable) {
        const edit = document.createElement("button");
        edit.type = "button";
        edit.className = "settings-btn settings-btn-secondary mcp-edit";
        edit.textContent = "编辑";
        edit.title = "把这条填进下方表单改配置后保存（覆盖）。改名请删旧新增。";
        edit.addEventListener("click", () => this.beginEdit(e.name, e.server));
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

    // F89a：项目 scope 加/改表单——本机恒显（无目录则 save 禁用）；远端仅在已填项目目录（=可写）时显。
    if (scope === "project" && (this.origin === null || !!dir)) {
      box.appendChild(this.renderAddForm(dir));
    }
    return box;
  }

  private renderAddForm(dir: string): HTMLElement {
    const form = document.createElement("div");
    form.className = "mcp-add-form";
    // F87b-fix：编辑态横幅（默认隐藏）——「编辑中 X · 取消」。编辑时锁名，防「改名再存」静默新增副本。
    const banner = document.createElement("div");
    banner.className = "mcp-edit-banner";
    banner.style.display = "none";
    const bLabel = document.createElement("span");
    banner.appendChild(bLabel);
    const cancelBtn = document.createElement("button");
    cancelBtn.type = "button";
    cancelBtn.className = "settings-btn settings-btn-secondary";
    cancelBtn.textContent = "取消编辑";
    cancelBtn.addEventListener("click", () => this.cancelEdit());
    banner.appendChild(cancelBtn);
    this.editBanner = banner;
    this.editNameLabel = bLabel;
    const nameInput = document.createElement("input");
    nameInput.className = "settings-input";
    nameInput.placeholder = "server 名";
    const jsonInput = document.createElement("textarea");
    jsonInput.className = "settings-input mcp-json-input";
    jsonInput.placeholder =
      '{ "command": "npx", "args": ["-y", "@x/mcp"] }  或  { "type": "http", "url": "https://…" }';
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
        showActionFailureToast("缺 server 名", "填一个 MCP server 名再保存。", {
          level: "info",
        });
        return;
      }
      const parsed = parseServerConfig(jsonInput.value);
      if (!parsed.ok) {
        showActionFailureToast("配置无效", parsed.error, { level: "info" });
        return;
      }
      void this.writeEntry(dir, name, parsed.value);
    });
    form.append(banner, nameInput, jsonInput, saveBtn);
    return form;
  }

  /** F87b-fix：进入编辑态——预填 + **锁名**（编辑 = 改配置不改名，防改名静默新增副本；改名请删旧新增）。 */
  private beginEdit(name: string, server: unknown): void {
    if (!this.addNameInput || !this.addJsonInput) return;
    this.addNameInput.value = name;
    this.addNameInput.readOnly = true;
    this.addJsonInput.value = JSON.stringify(server, null, 2);
    if (this.editBanner && this.editNameLabel) {
      this.editNameLabel.textContent = `编辑中：${name}（改配置后保存覆盖；改名请删旧新增）`;
      this.editBanner.style.display = "";
    }
    this.addJsonInput.focus();
    this.addJsonInput.scrollIntoView({ block: "nearest" });
  }

  /** F87b-fix：退出编辑态——解锁名、清空、收横幅。 */
  private cancelEdit(): void {
    if (this.addNameInput) {
      this.addNameInput.readOnly = false;
      this.addNameInput.value = "";
    }
    if (this.addJsonInput) this.addJsonInput.value = "";
    if (this.editBanner) this.editBanner.style.display = "none";
  }

  private async writeEntry(
    dir: string,
    name: string,
    server: unknown,
  ): Promise<void> {
    const startOrigin = this.origin; // 捕获：目标机器在 await 前定死（写入目标不受切机器影响）
    try {
      // F89a：本机 → 本地 FS 写；远端 → SFTP 写远端 .mcp.json（写面仍只 .mcp.json，SS-14；SS-G 用户显式触发）。
      if (startOrigin === null) {
        await commands.write_project_mcp_server({
          projectDir: dir,
          name,
          server,
        });
      } else {
        await commands.write_remote_mcp_server({
          origin: startOrigin,
          projectDir: dir,
          name,
          server,
        });
      }
    } catch (e) {
      if (this.origin === startOrigin)
        showActionFailureToast("写入 .mcp.json 失败", String(e));
      return;
    }
    // F89a 审计修·重要：await 期间用户已切机器 → 不回填 dirInput、不 refresh（否则拿旧 dir 渲染新机器项目）。
    if (this.origin !== startOrigin) return;
    this.dirInput.value = dir;
    await this.refresh();
  }

  private async removeEntry(dir: string, name: string): Promise<void> {
    const startOrigin = this.origin;
    const where = startOrigin === null ? "本机" : `远端 [${startOrigin}]`;
    if (
      !window.confirm(`从${where}项目 .mcp.json 删除 MCP server「${name}」？`)
    )
      return;
    try {
      if (startOrigin === null) {
        await commands.remove_project_mcp_server({ projectDir: dir, name });
      } else {
        await commands.remove_remote_mcp_server({
          origin: startOrigin,
          projectDir: dir,
          name,
        });
      }
    } catch (e) {
      if (this.origin === startOrigin)
        showActionFailureToast("删除失败", String(e));
      return;
    }
    if (this.origin !== startOrigin) return; // 期间切机器 → 丢弃回填/刷新
    this.dirInput.value = dir;
    await this.refresh();
  }
}
