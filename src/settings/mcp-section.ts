/**
 * F87（#50+#51）：MCP 管理（集成组内一节）。**SS-14 读写分界**：
 * - **读**：跨 scope 展示（用户 / local / 项目），后端 `read_mcp_servers` 宽容读三处。**用户/local scope 只读**。
 * - **写**：**只**项目 scope——增/改/删该项目 `<dir>/.mcp.json`，后端 `write_project_mcp_server`/`remove_project_mcp_server`
 *   硬编码只碰 `.mcp.json`（绝不写 `~/.claude.json`/`settings.json`）。
 *
 * 设置窗独立于主窗口、拿不到活跃会话 cwd → 用项目目录输入框（datalist 从 `list_mcp_project_dirs` 自动补全「用过的项目」）。
 * 纯函数（groupByScope / serverSummary / parseServerConfig）零 import，node 可测。
 */
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { commands } from "../ipc/commands";
// 〔步 12·C〕本机那个 origin 的**唯一住址**（Rust 侧是 `inbound_client::LOCAL_ORIGIN`，
// 两侧由 `origin_tests::the_sentinel_agrees_with_the_two_existing_homes` 两向钉着）。
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { showActionFailureToast } from "../error-toast";
import { McpSyncPanel } from "./mcp-sync";
import type { AssetInstallApi } from "./assets-section";

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
import { copyText } from "../copy-table";

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
  return copyText("mcp.serverSummary.unknown");
}

/** 解析 server 配置 JSON 文本：必须是对象。纯函数。 */
export function parseServerConfig(
  text: string,
): { ok: true; value: unknown } | { ok: false; error: string } {
  const t = text.trim();
  if (!t) return { ok: false, error: copyText("mcp.parse.empty") };
  let v: unknown;
  try {
    v = JSON.parse(t);
  } catch (e) {
    return {
      ok: false,
      error: copyText("mcp.parse.badJson", { message: e instanceof Error ? e.message : String(e) }),
    };
  }
  if (!v || typeof v !== "object" || Array.isArray(v))
    return { ok: false, error: copyText("mcp.parse.notObject") };
  return { ok: true, value: v };
}

// 〔CP2b〕做成函数、用到时才取文（模块顶层不留取文口调用 —— 顶层调用会让 Rollup 把设置面板挪进主窗共享 chunk）。
const SCOPE_LABEL = (): Record<McpScope, string> => ({
  user: copyText("mcp.scope.user"),
  local: copyText("mcp.scope.local"),
  project: copyText("mcp.scope.project"),
});

/**
 * 〔AS2 · 第四波 4B · V113〕机器页「资产目录」那一块（`assets-section.ts`）要的「装」命令，**从这里递出去**：
 * 「装 MCP / skill」那一件在前端的落点钉在本文件（`tests/evidence/K-R117-ruler.py` 的 `R9a` 名单，只许缩），
 * 资产目录那一块自己不另立一份。MCP 那两条就是 AS1 推 / 拉那两条原样；skill 那两条是 AS2 的。
 */
export function assetInstallApi(): AssetInstallApi {
  return {
    dirs: (a) => commands.list_mcp_project_dirs(a),
    mcpPreview: (a) => commands.mcp_sync_preview(a),
    mcpApply: (a) => commands.mcp_sync_apply(a),
    skillPreview: (a) => commands.skill_install_preview(a),
    skillApply: (a) => commands.skill_install_apply(a),
    // 〔SU1 · 第四波 4C · V116〕卸：同一件（③ 装 / 卸 skill）的前端落点仍只在本文件。
    skillUninstall: (a) => commands.skill_uninstall_apply(a),
  };
}

export class McpSection {
  readonly element: HTMLElement;
  private machineRow!: HTMLElement;
  private dirRow!: HTMLElement;
  private dirInput!: HTMLInputElement;
  private datalist!: HTMLDataListElement;
  /** P6b：可浏览的工作目录清单（与 `datalist` 同源，见 `renderDirCandidates`）。 */
  private dirsBox!: HTMLElement;
  private listBox!: HTMLElement;
  /** F87b②：project scope 加/改表单的输入引用——「编辑」按钮预填用（每次 reload 重建时刷新）。 */
  private addNameInput: HTMLInputElement | null = null;
  private addJsonInput: HTMLTextAreaElement | null = null;
  /** F87b-fix：编辑态横幅（「编辑中 X · 取消」）——编辑时锁名，防改名静默重复。 */
  private editBanner: HTMLElement | null = null;
  private editNameLabel: HTMLElement | null = null;
  /** F87b③：当前选中的机器。`LOCAL_ORIGIN` = 本机（既有本地读写）；其余 = 远端 origin（只读跨机）。 */
  private origin: Origin = LOCAL_ORIGIN;
  /** ST1：`loadNow()` 之前收到的「要看哪台」（只记不读）。 */
  private wantedOrigin: Origin = getCurrentMachine();
  private loaded = false;
  /**
   * 〔AS1 · 第四波 4B〕「跨机器推 / 拉」那一块（`mcp-sync.ts`）。一个实例跟着本分节活，挂在可写的项目 scope 下面；
   * 本页那台机器 ＝ `this.origin`、项目目录 ＝ 输入框里那一个。拉（写的是本页这台）写完 ⇒ 本页重读。
   */
  private readonly sync = new McpSyncPanel(
    () => ({ origin: this.origin, dir: this.currentDir() }),
    () => void this.refresh(),
    // 四条命令从这里递进去（本分节是「装 MCP」那一件在前端的落点，面板自己不另立一份）。
    {
      machines: () => commands.list_remote_mcp_origins(),
      dirs: (a) => commands.list_mcp_project_dirs(a),
      preview: (a) => commands.mcp_sync_preview(a),
      apply: (a) => commands.mcp_sync_apply(a),
    },
  );

  constructor() {
    this.element = this.build();
    // S4a：跟随共用的「当前在看哪台机器」store。本分节是四块里**唯一**能表示「本机」的
    // （它的机器行第一颗按钮就是本机），所以本机也照单全收。
    // `selectMachine` 自带「同值早退」，与 store 的「同值不通知」两道去重叠加，
    // 不会因为往返而多打一次 ssh。
    // ST1「延后加载」：还没 `loadNow()` 之前只记下要看哪台，**不发 I/O**。
    subscribeMachine((origin) => {
      this.wantedOrigin = origin;
      if (this.loaded) void this.selectMachine(origin);
    });
    this.loadMachines(); // E59：只渲染「在看哪台」那一行（选择按钮已删）
  }

  /**
   * ST1「延后加载」（`设计/70 §5.3` 判据 2：**子页内容只在该子页可见时才发 I/O**）：
   * 构造期不再发 I/O；宿主（`panel.ts`）在**某台机器的子页第一次可见**时调它。
   * 重开设置后宿主会再调一次（重开要看新读数）。
   */
  loadNow(): void {
    this.loaded = true;
    if (this.wantedOrigin !== this.origin) {
      void this.selectMachine(this.wantedOrigin); // 它自己会拉候选 + 读
      return;
    }
    if (isLocalOrigin(this.origin)) void this.loadProjectCandidates();
    else void this.loadRemoteProjectCandidates(this.origin);
    // 业务二审 gap#6：打开即读（空 dir 也先显 user/local scope），不再是看似坏掉的空框。
    void this.refresh();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless mcp-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      copyText("mcp.build.intro");
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
    this.dirInput.placeholder = copyText("mcp.build.projectDir");
    this.dirInput.setAttribute("list", "mcp-project-dirs");
    this.datalist = document.createElement("datalist");
    this.datalist.id = "mcp-project-dirs";
    const readBtn = document.createElement("button");
    readBtn.type = "button";
    readBtn.className = "settings-btn";
    readBtn.textContent = copyText("mcp.build.read");
    readBtn.addEventListener("click", () => void this.refresh());
    this.dirInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void this.refresh();
    });
    row.append(this.dirInput, this.datalist, readBtn);
    root.appendChild(row);

    // P6b：工作目录**清单**。`datalist` 是自动补全 —— 你得先敲出点什么它才帮你补，
    // 而实测本机 12 个项目里只有 4 个真有 `.mcp.json` ⇒ 用户只能「猜一个、点进去、
    // 发现是空的、再猜下一个」。清单把「先知道路径」这个前提去掉。
    this.dirsBox = document.createElement("div");
    this.dirsBox.className = "mcp-dirs";
    root.appendChild(this.dirsBox);

    this.listBox = document.createElement("div");
    this.listBox.className = "mcp-list";
    root.appendChild(this.listBox);

    return root;
  }

  /**
   * P6b：候选目录的**唯一渲染口** —— `datalist`（自动补全）与可见清单**同源**。
   *
   * 两处各写一遍就是「一段逻辑、两种表示」：加一台机器的路径来源时，
   * 很容易只喂了其中一个，而**少喂的那个不会报错，只是少了几项**。
   */
  /** P6b：候选**没读到**时的样子〔E 阶段补审〕。
   *
   * ⚠ 不许说成「这台机器还没有用过的项目目录」—— 那是一句与真实原因无关的话，
   * 而这两件事的下一步完全不同：「没用过」⇒ 手填一个新路径；「没读到」⇒ 看看那台机器连没连上。
   * 本工作区整轮都在收口这一族（`P4d-Y5` 的「未找到远端配置: `<local>`」是同一个病）。
   */
  private renderDirCandidatesFailed(): void {
    this.datalist.replaceChildren();
    this.dirsBox.replaceChildren();
    const hint = document.createElement("div");
    hint.className = "settings-hint mcp-dirs-empty";
    hint.textContent = copyText("mcp.dirs.failed");
    this.dirsBox.appendChild(hint);
  }

  /** P6b：候选还没回来时的样子 —— **空着并说清在读**，绝不留上一台的路径。 */
  private renderDirCandidatesLoading(): void {
    this.datalist.replaceChildren();
    this.dirsBox.replaceChildren();
    const hint = document.createElement("div");
    hint.className = "settings-hint mcp-dirs-empty";
    hint.textContent = copyText("mcp.dirs.loading");
    this.dirsBox.appendChild(hint);
  }

  private renderDirCandidates(dirs: string[]): void {
    this.datalist.replaceChildren();
    this.dirsBox.replaceChildren();
    for (const d of dirs) {
      const opt = document.createElement("option");
      opt.value = d;
      this.datalist.appendChild(opt);
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "settings-btn mcp-dir-chip";
      chip.textContent = d;
      chip.title = d; // 路径可能很长（实测最长 111 字符），悬停看全
      chip.addEventListener("click", () => {
        this.dirInput.value = d;
        void this.refresh();
      });
      this.dirsBox.appendChild(chip);
    }
    if (dirs.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint mcp-dirs-empty";
      // 说清是「这台机器没用过项目」，不是「加载失败」——两者的下一步完全不同。
      empty.textContent = copyText("mcp.dirs.none");
      this.dirsBox.appendChild(empty);
    }
  }

  private async loadProjectCandidates(): Promise<void> {
    // ★ **本机这条也要守竞态**〔P6b-Y2〕：它同样是 `await`，
    // 而 `origin` 可能在这期间被切走（共用 store 是别处也能改的）。
    // 远端那条早就有这个守卫（`if (this.origin !== origin) return;`），本机那条**漏了**。
    const want = this.origin;
    try {
      const dirs = await commands.list_mcp_project_dirs({ origin: LOCAL_ORIGIN });
      if (this.origin !== want) return; // 期间切走
      this.renderDirCandidates(dirs);
    } catch {
      // ★ 不能什么都不做〔E 阶段补审〕：`selectMachine` 已经把清单换成「读取中…」，
      // 这里静默返回 ⇒ 面板**永远停在「读取中…」**。
      if (this.origin === want) this.renderDirCandidatesFailed();
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
    // E59：**这一整行「机器：本机 / devbox / nano」按钮已删。**
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
    label.textContent = copyText("mcp.machine.label");
    this.machineRow.appendChild(label);
    const name = document.createElement("span");
    name.className = "mcp-machine-name";
    name.textContent = isLocalOrigin(this.origin) ? copyText("mcp.machine.local") : this.origin;
    this.machineRow.appendChild(name);
  }

  /** F87b③/F89a：切机器。本机 → 本地读写；远端 → **项目目录行也显**（F89a：填项目=远端项目 .mcp.json 可写；
   *  空=远端 user scope 只读）。切机器清空目录（本机/远端项目路径不通用）+ 换 datalist 候选。
   *  F87b-fix：已是当前机器 → 早退（防误双击并发 SSH）。 */
  private async selectMachine(origin: Origin): Promise<void> {
    if (origin === this.origin) return;
    this.origin = origin;
    this.dirRow.style.display = ""; // F89a：远端也显目录行（可填项目管理远端 .mcp.json）
    this.dirInput.value = ""; // 本机/远端项目路径不通用，切机器清空
    this.sync.reset(); // 〔AS1〕上一台的差异与「另一台」候选一并作废（面板折着就一发 I/O 都不打）
    // ★ **候选清单也要一起清**〔P6b D 阶段补审 08-12〕。
    //
    // 上面那句注释说的「路径不通用」对候选同样成立，而在 P6b 之前它不显眼：候选只进
    // 不可见的 `datalist`。P6b 把它变成了**可见且可点**的清单 ⇒ 不清的话，
    // 切到 B 机后面板仍挂着 A 机的路径，点一下就是**拿 A 的路径去读 B**。
    // 远端那趟是一整趟 SSH（30s 超时），这个窗口一点都不短。
    this.renderDirCandidatesLoading();
    // E59：按钮没了，改成更新那行只读显示。
    const name = this.machineRow.querySelector<HTMLElement>(".mcp-machine-name");
    if (name) name.textContent = isLocalOrigin(origin) ? copyText("mcp.machine.local") : origin;
    if (isLocalOrigin(origin)) void this.loadProjectCandidates();
    else void this.loadRemoteProjectCandidates(origin);
    await this.refresh();
  }

  /** F89a：统一刷新入口，按 (机器, 目录) 三态分发。 */
  private async refresh(): Promise<void> {
    if (isLocalOrigin(this.origin)) return this.reload(); // 本机
    const dir = this.currentDir();
    if (dir) return this.reloadRemoteProject(this.origin, dir); // 远端项目（可写）
    return this.reloadRemote(this.origin); // 远端 user scope（只读）
  }

  /** F89a：读远端某项目的 datalist 候选（远端 `~/.claude.json` projects 键）。 */
  private async loadRemoteProjectCandidates(origin: string): Promise<void> {
    let dirs: string[] | null = null;
    try {
      // 〔步 12·C〕与本机那条是**同一条命令**了（`list_mcp_project_dirs`）。
      dirs = await commands.list_mcp_project_dirs({ origin });
    } catch {
      // ★ `null` 与 `[]` 是**两件事**〔E 阶段补审〕：原来这里 catch 之后 `dirs` 仍是 `[]`，
      // 于是读失败会显示「这台机器还没有用过的项目目录」—— 一句与真实原因无关的话。
      dirs = null;
    }
    if (this.origin !== origin) return; // 期间切走
    if (dirs === null) this.renderDirCandidatesFailed();
    else this.renderDirCandidates(dirs);
  }

  /** F89a：读+管理远端某项目的 `.mcp.json`（project scope 可写）。切走/改目录 → 丢弃。 */
  private async reloadRemoteProject(
    origin: string,
    dir: string,
  ): Promise<void> {
    this.listBox.replaceChildren();
    const loading = document.createElement("div");
    loading.className = "settings-hint mcp-loading";
    loading.textContent = copyText("mcp.remoteProject.reading", { machine: origin, dir });
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
      line.textContent = copyText("mcp.remoteProject.failed", { machine: origin, e: String(e) });
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "settings-btn";
      retry.textContent = copyText("mcp.reloadRemoteProject.retry");
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
    note.textContent = copyText("mcp.remoteProject.writable", { machine: origin, dir });
    this.listBox.prepend(head);
    head.appendChild(note);
  }

  private async reload(): Promise<void> {
    const startOrigin = this.origin; // 捕获调用时机器：await 期间用户切走则丢弃这次本地结果（防旧结果盖新选中）
    const dir = this.currentDir();
    // ST1「切机器 pending」：切回本机时这一格原先是空的，与「本机没有 MCP 配置」分不开 ⇒ 先说在读。
    const loading = document.createElement("div");
    loading.className = "settings-hint mcp-loading";
    loading.setAttribute("aria-busy", "true");
    loading.textContent = copyText("mcp.local.reading");
    this.listBox.replaceChildren(loading);
    let entries: McpServerEntry[];
    try {
      entries = await commands.read_mcp_servers({
        projectDir: dir || null,
      });
    } catch (e) {
      loading.remove();
      if (this.origin !== startOrigin) return; // 期间已切走 → 静默丢弃
      showActionFailureToast(copyText("mcp.local.failed"), String(e));
      return;
    }
    // 成功那一支不用撤：`renderList` 自己会整格重画（`replaceChildren`）。
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
    loading.textContent = copyText("mcp.remote.reading", { machine: origin });
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
      empty.textContent = copyText("mcp.remote.empty", { machine: origin });
      this.listBox.appendChild(empty);
    }
  }

  /** F87b-fix：远端读失败 → 常驻内联错误 + 重试钮（区分「读失败」vs「远端无配置」）。 */
  private renderRemoteError(origin: string, msg: string): void {
    this.listBox.replaceChildren();
    const box = document.createElement("div");
    box.className = "mcp-remote-error";
    const line = document.createElement("div");
    line.textContent = copyText("mcp.remote.failed", { machine: origin, msg });
    box.appendChild(line);
    const retry = document.createElement("button");
    retry.type = "button";
    retry.className = "settings-btn";
    retry.textContent = copyText("mcp.renderRemoteError.retry");
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
      copyText("mcp.remote.header");
    const refresh = document.createElement("button");
    refresh.type = "button";
    refresh.className =
      "settings-btn mcp-remote-refresh";
    refresh.textContent = copyText("mcp.remote.reread");
    refresh.addEventListener("click", () => void this.reloadRemote(origin));
    head.append(note, refresh);
    return head;
  }

  /** 渲染分组列表。remote 模式跳过空 scope（user scope 只读噪音）——**但可写的远端项目 scope 即使空也渲染**
   *  （F89a 审计修·阻塞：否则新/空远端项目不出加表单，无法建第一条 server）。
   *  〔TL1 · 4C〕从前这里还把读到的 server 累进一个「库」、列表尾挂一块「注册到此项目」（F89b）—— 退役了：
   *  资产目录的家是后端（V113，机器页「资产目录」那一块），装要经差异、对面不同要问盖不盖（V111 / V112），
   *  而「库」是本分节内存里的第二份目录、同名直接盖（`调研/第四波记录/TL1.md` 件 3）。 */
  private renderList(
    entries: McpServerEntry[],
    dir: string,
    remote: boolean,
  ): void {
    this.listBox.replaceChildren();
    const grouped = groupByScope(entries);
    for (const scope of ["user", "local", "project"] as McpScope[]) {
      const writableProject = scope === "project" && !!dir; // 可写项目 scope 恒渲染（带加表单）
      if (remote && grouped[scope].length === 0 && !writableProject) continue;
      this.listBox.appendChild(
        this.renderScope(scope, grouped[scope], dir, remote),
      );
    }
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
        ? copyText("mcp.scope.readOnlyRemote")
        : copyText("mcp.scope.readOnly");
    title.textContent = `${SCOPE_LABEL()[scope]}（${entries.length}）${readOnlySuffix}`;
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
      src.textContent = copyText("mcp.scope.source", { sourcePath: e.sourcePath });
      const pre = document.createElement("pre");
      pre.className = "mcp-server-json";
      detail.append(src, pre);
      let jsonBuilt = false;

      const jsonBtn = document.createElement("button");
      jsonBtn.type = "button";
      jsonBtn.className = "settings-btn mcp-json-toggle";
      jsonBtn.textContent = "JSON";
      jsonBtn.title = copyText("mcp.scope.detailHint");
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
        edit.className = "settings-btn mcp-edit";
        edit.textContent = copyText("mcp.scope.edit");
        edit.title = copyText("mcp.scope.editHint");
        edit.addEventListener("click", () => this.beginEdit(e.name, e.server));
        rowEl.appendChild(edit);

        const del = document.createElement("button");
        del.type = "button";
        del.className = "settings-btn settings-btn-danger mcp-del";
        del.textContent = copyText("mcp.scope.delete");
        del.addEventListener("click", () => void this.removeEntry(dir, e.name));
        rowEl.appendChild(del);
      }
      item.append(rowEl, detail);
      box.appendChild(item);
    }

    // F89a：项目 scope 加/改表单——本机恒显（无目录则 save 禁用）；远端仅在已填项目目录（=可写）时显。
    if (scope === "project" && (isLocalOrigin(this.origin) || !!dir)) {
      box.appendChild(this.renderAddForm(dir));
    }
    // 〔AS1〕跨机器推 / 拉：只在「这台机器的这个项目」可写时出现（推的来源 / 拉的落点都是它）。
    if (writable) box.appendChild(this.sync.element);
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
    cancelBtn.className = "settings-btn";
    cancelBtn.textContent = copyText("mcp.form.cancelEdit");
    cancelBtn.addEventListener("click", () => this.cancelEdit());
    banner.appendChild(cancelBtn);
    this.editBanner = banner;
    this.editNameLabel = bLabel;
    const nameInput = document.createElement("input");
    nameInput.className = "settings-input";
    nameInput.placeholder = copyText("mcp.form.name");
    const jsonInput = document.createElement("textarea");
    jsonInput.className = "settings-input mcp-json-input";
    // 〔CP2b〕两段 JSON 例子是代码、不是话（而且带花括号，进不了文案表）⇒ 只有中间那个「或」进表。
    jsonInput.placeholder = copyText("mcp.form.jsonHint", { a: '{ "command": "npx", "args": ["-y", "@x/mcp"] }', b: '{ "type": "http", "url": "https://…" }' });
    jsonInput.rows = 3;
    // F87b②：暴露引用给「编辑」按钮预填（每次 reload 重建表单时刷新）。
    this.addNameInput = nameInput;
    this.addJsonInput = jsonInput;
    const saveBtn = document.createElement("button");
    saveBtn.type = "button";
    saveBtn.className = "settings-btn";
    saveBtn.textContent = copyText("mcp.form.save");
    if (!dir) {
      saveBtn.disabled = true;
      saveBtn.title = copyText("mcp.form.needDir");
    }
    saveBtn.addEventListener("click", () => {
      const name = nameInput.value.trim();
      if (!name) {
        showActionFailureToast(copyText("mcp.form.noName"), copyText("mcp.form.noNameBody"), {
          level: "info",
        });
        return;
      }
      const parsed = parseServerConfig(jsonInput.value);
      if (!parsed.ok) {
        showActionFailureToast(copyText("mcp.form.badConfig"), parsed.error, { level: "info" });
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
      this.editNameLabel.textContent = copyText("mcp.form.editing", { name });
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
      // 〔步 12·C 收尾〕**两侧同一条命令**（`write_remote_mcp_server` 已退役）：
      // 走哪一侧由 `origin` 说，不再由命令名说。
      // 🔴 本机是 `LOCAL_ORIGIN`（`"<local>"`）**不是 `null`**。〔C4a〕本分节内部也是这个表示，原样过线。
      await commands.write_project_mcp_server({
        origin: startOrigin,
        projectDir: dir,
        name,
        server,
      });
    } catch (e) {
      if (this.origin === startOrigin)
        showActionFailureToast(copyText("mcp.write.failed"), String(e));
      return;
    }
    // F89a 审计修·重要：await 期间用户已切机器 → 不回填 dirInput、不 refresh（否则拿旧 dir 渲染新机器项目）。
    if (this.origin !== startOrigin) return;
    this.dirInput.value = dir;
    await this.refresh();
  }

  private async removeEntry(dir: string, name: string): Promise<void> {
    const startOrigin = this.origin;
    const where = isLocalOrigin(startOrigin) ? copyText("mcp.who.local") : copyText("mcp.who.remote", { machine: startOrigin });
    if (
      !window.confirm(copyText("mcp.remove.confirm", { where, name }))
    )
      return;
    try {
      // 〔步 12·C 收尾〕同 `writeEntry`：两侧一条命令，本机逐字送 `LOCAL_ORIGIN`。
      await commands.remove_project_mcp_server({
        origin: startOrigin,
        projectDir: dir,
        name,
      });
    } catch (e) {
      if (this.origin === startOrigin)
        showActionFailureToast(copyText("mcp.remove.failed"), String(e));
      return;
    }
    if (this.origin !== startOrigin) return; // 期间切机器 → 丢弃回填/刷新
    this.dirInput.value = dir;
    await this.refresh();
  }
}
