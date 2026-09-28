/**
 * 〔AS1 · 第四波 4B〕MCP 分节里的「推到 / 拉自另一台」（`设计/96` 的 B）。
 *
 * 用户 2026-09-24 裁（`99 §1` V111 · V112）：各管各的，只有显式推 / 拉；推 / 拉之前先给看差异，对面有不同就问盖不盖；
 * 内容原样拷过去并标出可疑项，不替用户改写；不做自动同步、不做冲突合并。
 *
 * # 本文件只做排版与手势（`设计/01 §1.1`）
 *
 * 差异四态、可疑项、「写哪几条」都由**要被写的那一台**的后端判（`mcp-sync-plan`），读与写也在那台
 * （〔MIG-3a〕`src/mcp-sync-reads.ts` 经通道问，从前的 monitor 两条命令删了）。本文件只：把行画出来 · 收勾选 · 把看差异时拿到的两份原文原样送回去。
 * 「对面不同的那一条要不要盖」这一问由勾选框问（默认不勾）；勾了 ⇒ 这一条进 `overwrite`。后端那一侧再核一遍：
 * 勾了 `take` 却没进 `overwrite` 的 `differs` ⇒ 整趟拒 —— 界面漏问也漏不过去。
 *
 * 纯函数（`defaultTake` · `applyArgs` · `stateText` · `suspectText`）零 DOM，node 可测。
 */
import type { commands } from "../ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { copyText } from "../copy-table";
import type { McpSyncPreview, McpSyncRow, McpSyncSuspect, mcpSyncApply, mcpSyncPreview } from "../mcp-sync-reads";

export type Direction = "push" | "pull";

/**
 * 差异四态 → 给人看的字（`to` = 要被写的那一台的名字：推时是另一台，拉时是本页这台）。
 * **键集 == 后端 `mcp_sync::STATES`**（`tests/settings/mcp-sync.vitest.ts` 从后端源码现抠、两向相等）。
 */
export const STATE_TEXT: Record<string, (to: string) => string> = {
  new: (to) => copyText("mcpSync.state.new", { machine: to }),
  same: () => copyText("mcpSync.state.same"),
  differs: (to) => copyText("mcpSync.state.differs", { machine: to }),
  "only-there": (to) => copyText("mcpSync.state.onlyThere", { machine: to }),
};

/**
 * 绝对路径那一种：按「要被写的那一台上有没有」各一句。**键集 == 后端 `mcp_sync::THERE`**（同上）。
 * 一句话一个键，不在这里拼半句（`设计/91 §5.1.1` 决定 3）。
 */
export const ABS_PATH_TEXT: Record<
  string,
  (x: McpSyncSuspect, to: string) => string
> = {
  present: (x, to) =>
    copyText("mcpSync.suspect.absPresent", {
      field: x.field,
      value: x.value,
      machine: to,
    }),
  absent: (x, to) =>
    copyText("mcpSync.suspect.absAbsent", {
      field: x.field,
      value: x.value,
      machine: to,
    }),
  foreign: (x, to) =>
    copyText("mcpSync.suspect.absForeign", {
      field: x.field,
      value: x.value,
      machine: to,
    }),
  unknown: (x, to) =>
    copyText("mcpSync.suspect.absUnknown", {
      field: x.field,
      value: x.value,
      machine: to,
    }),
};

/** 可疑项的种类 → 那句话。**键集 == 后端 `mcp_sync::SUSPECT_KINDS`**（同上）。 */
export const SUSPECT_TEXT: Record<
  string,
  (x: McpSyncSuspect, to: string) => string
> = {
  "abs-path": (x, to) => {
    const f = x.there ? ABS_PATH_TEXT[x.there] : undefined;
    return f ? f(x, to) : otherText(x);
  },
  "command-missing": (x, to) =>
    x.there === "unknown"
      ? copyText("mcpSync.suspect.commandUnknown", {
          value: x.value,
          machine: to,
        })
      : copyText("mcpSync.suspect.commandMissing", {
          value: x.value,
          machine: to,
        }),
  "command-relative": (x, to) =>
    copyText("mcpSync.suspect.commandRelative", {
      value: x.value,
      machine: to,
    }),
};

/** 认不出的种类 / 事实（两端版本对不上）：照原样说出是哪一格、什么值，不猜成某一种。 */
function otherText(x: McpSyncSuspect): string {
  return copyText("mcpSync.suspect.other", { field: x.field, value: x.value });
}

/** 一种状态给人看的字；认不出的态原样显示（不猜成某一档）。 */
export function stateText(state: string, to: string): string {
  const f = STATE_TEXT[state];
  return f ? f(to) : state;
}

/** 一条可疑项给人看的那句话。 */
export function suspectText(x: McpSyncSuspect, to: string): string {
  const f = SUSPECT_TEXT[x.kind];
  return f ? f(x, to) : otherText(x);
}

/** 能勾的只有「对面没有」与「对面不同」两态（`same` 写不写都一样，`only-there` 永远不碰）。 */
export function selectable(state: string): boolean {
  return state === "new" || state === "differs";
}

/** 默认勾选：只勾「对面没有」的。「对面不同」的**默认不勾** —— 盖不盖要用户自己说。 */
export function defaultTake(rows: readonly McpSyncRow[]): Set<string> {
  return new Set(rows.filter((r) => r.state === "new").map((r) => r.name));
}

/** 勾选 → 交给后端的两张单子：`take` = 勾了的；`overwrite` = 勾了的里「对面不同」的那几条。 */
export function applyArgs(
  rows: readonly McpSyncRow[],
  checked: ReadonlySet<string>,
): { take: string[]; overwrite: string[] } {
  const picked = rows.filter((r) => selectable(r.state) && checked.has(r.name));
  return {
    take: picked.map((r) => r.name),
    overwrite: picked.filter((r) => r.state === "differs").map((r) => r.name),
  };
}

function machineName(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("mcpSync.machine.local") : origin;
}

/**
 * 本面板要的四条命令。**由宿主（`mcp-section.ts`）递进来**，本文件自己不调包装层 ——
 * 「装 MCP / skill」那一件的前端落点钉在一张名单上（`tests/evidence/K-R117-ruler.py` 的 `R9a`：
 * 目标是每组收成一份、只许缩），本件不给它加一份新落点。
 */
export interface McpSyncApi {
  /** 已配置的远端（只读配置，不连机器）。 */
  machines: typeof commands.list_remote_mcp_origins;
  /** 那台机器用过的项目目录（自动补全）。 */
  dirs: (a: { origin: Origin }) => Promise<string[]>;
  preview: typeof mcpSyncPreview;
  apply: typeof mcpSyncApply;
}

/** 看差异那一刻定下的一切（写的时候原样用，不再读界面上的输入框）。 */
interface Pending {
  to: Origin;
  toDir: string;
  preview: McpSyncPreview;
  checked: Set<string>;
}

/**
 * 面板本体。宿主（`McpSection`）在「本页那台机器 ＋ 已填项目目录」可写时把 `element` 挂进项目 scope 那一块；
 * 切机器时调 [`reset`]。
 */
export class McpSyncPanel {
  readonly element: HTMLElement;
  private direction: Direction = "push";
  private machineSelect: HTMLSelectElement;
  private dirInput: HTMLInputElement;
  private datalist: HTMLDataListElement;
  private resultBox: HTMLElement;
  private pending: Pending | null = null;
  private body: HTMLElement;
  /** 「另一台」的候选读过了没有（本页换机器就作废）。 */
  private machinesLoaded = false;
  /** 防旧结果盖新的：每次看差异 / 写都 +1，回来时对不上就丢。 */
  private seq = 0;

  constructor(
    private readonly here: () => { origin: Origin; dir: string },
    private readonly onWroteHere: () => void,
    private readonly api: McpSyncApi,
  ) {
    const root = document.createElement("div");
    // 样式全用本页既有的类，不新立样式文件：设置窗第一份 CSS Module 会撞上产物里「module 规则排在全局规则之后」
    // 那条次序（`tests/entry-graphs.vitest.ts`；设置窗的 JS 导入样式在产物里排在 html 链的全局样式前面）——
    // 那是 UC2 那一侧的题，本件不去解。
    root.className = "mcp-scope";
    // 折起来的：展开之前一发 I/O 都不打（`设计/70 §5.3` 判据 2：内容只在看得见时才发 I/O；
    // 本页读 MCP 那一趟不该顺带把「另一台」的清单也读一遍）。
    const open = document.createElement("button");
    open.type = "button";
    open.className = "settings-btn";
    open.textContent = copyText("mcpSync.panel.open");
    // 折 / 展是「挂不挂进去」，不是 `hidden`（S30 ⑦：被 hidden 切的元素要能静态认出类名）。
    this.body = document.createElement("div");
    open.addEventListener("click", () => {
      if (this.body.isConnected) {
        this.body.remove();
        return;
      }
      root.appendChild(this.body);
      if (!this.machinesLoaded) void this.loadMachines();
    });
    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent = copyText("mcpSync.panel.hint");

    const row = document.createElement("div");
    row.className = "settings-row mcp-machine-row"; // 一行可折行（同本页「机器」那一行）
    const dir = document.createElement("select");
    dir.className = "settings-input";
    for (const [v, text] of [
      ["push", copyText("mcpSync.direction.push")],
      ["pull", copyText("mcpSync.direction.pull")],
    ] as const) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = text;
      dir.appendChild(o);
    }
    dir.addEventListener("change", () => {
      this.direction = dir.value === "pull" ? "pull" : "push";
      this.clearResult();
    });
    this.machineSelect = document.createElement("select");
    this.machineSelect.className = "settings-input";
    this.machineSelect.addEventListener("change", () => {
      this.clearResult();
      void this.loadDirCandidates();
    });
    this.dirInput = document.createElement("input");
    this.dirInput.className = "settings-input";
    this.dirInput.placeholder = copyText("mcpSync.dir.placeholder");
    this.datalist = document.createElement("datalist");
    this.datalist.id = "mcp-sync-dirs";
    this.dirInput.setAttribute("list", this.datalist.id);
    const go = document.createElement("button");
    go.type = "button";
    go.className = "settings-btn";
    go.textContent = copyText("mcpSync.preview.action");
    go.addEventListener("click", () => void this.preview());
    row.append(dir, this.machineSelect, this.dirInput, this.datalist, go);

    this.resultBox = document.createElement("div");
    this.resultBox.className = "mcp-list";
    this.body.append(hint, row, this.resultBox);
    root.append(open);
    this.element = root;
  }

  /** 本页换了机器：清掉上一台的一切；「另一台」的候选作废（开着就当场重读，折着就等下次展开）。 */
  reset(): void {
    this.seq++;
    this.dirInput.value = "";
    this.clearResult();
    this.machinesLoaded = false;
    if (this.body.isConnected) void this.loadMachines();
  }

  private clearResult(): void {
    this.pending = null;
    this.resultBox.replaceChildren();
  }

  private say(text: string, error = false): void {
    const line = document.createElement("div");
    line.className = error ? "mcp-remote-error" : "settings-hint";
    line.textContent = text;
    this.resultBox.replaceChildren(line);
  }

  /** 「另一台」的候选：本机 ＋ 已配置且启用的远端，去掉本页那台。只读配置，不连任何机器。 */
  private async loadMachines(): Promise<void> {
    const want = this.seq;
    let remotes: string[];
    try {
      remotes = await this.api.machines();
    } catch (e) {
      if (want === this.seq)
        this.say(
          copyText("mcpSync.machines.failed", { reason: String(e) }),
          true,
        );
      return;
    }
    if (want !== this.seq) return;
    this.machinesLoaded = true;
    const here = this.here().origin;
    const others = [LOCAL_ORIGIN, ...remotes].filter((o) => o !== here);
    this.machineSelect.replaceChildren();
    for (const o of others) {
      const opt = document.createElement("option");
      opt.value = o;
      opt.textContent = machineName(o);
      this.machineSelect.appendChild(opt);
    }
    if (others.length === 0) this.say(copyText("mcpSync.machines.none"));
    else void this.loadDirCandidates();
  }

  /** 对面那台用过的项目目录（与本页项目目录的候选同一条命令）。读不到就空着，手填照样行。 */
  private async loadDirCandidates(): Promise<void> {
    const other = this.machineSelect.value;
    this.datalist.replaceChildren();
    if (!other) return;
    try {
      const dirs = await this.api.dirs({ origin: other });
      if (this.machineSelect.value !== other) return;
      for (const d of dirs) {
        const opt = document.createElement("option");
        opt.value = d;
        this.datalist.appendChild(opt);
      }
    } catch {
      // 候选只是自动补全；读不到时输入框照样能手填，这里不打扰。
    }
  }

  private async preview(): Promise<void> {
    const other = this.machineSelect.value;
    const otherDir = this.dirInput.value.trim();
    const { origin: here, dir: hereDir } = this.here();
    if (!other) return this.say(copyText("mcpSync.machines.none"));
    if (!otherDir) return this.say(copyText("mcpSync.dir.missing"), true);
    const push = this.direction === "push";
    const [from, fromDir, to, toDir] = push
      ? [here, hereDir, other, otherDir]
      : [other, otherDir, here, hereDir];
    const want = ++this.seq;
    this.pending = null;
    this.say(copyText("mcpSync.preview.loading"));
    let preview: McpSyncPreview;
    try {
      preview = await this.api.preview({ from, fromDir, to, toDir });
    } catch (e) {
      if (want === this.seq)
        this.say(
          copyText("mcpSync.preview.failed", { reason: String(e) }),
          true,
        );
      return;
    }
    if (want !== this.seq) return;
    this.pending = { to, toDir, preview, checked: defaultTake(preview.rows) };
    this.renderPreview(from, to);
  }

  private renderPreview(from: Origin, to: Origin): void {
    const p = this.pending;
    if (!p) return;
    const box = document.createElement("div");
    const head = document.createElement("div");
    head.className = "settings-hint";
    head.textContent = copyText("mcpSync.preview.head", {
      source: machineName(from),
      sourcefile: p.preview.sourcePath,
      target: machineName(to),
      targetfile: p.preview.targetPath,
    });
    box.appendChild(head);
    if (p.preview.rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "settings-hint";
      empty.textContent = copyText("mcpSync.preview.empty");
      box.appendChild(empty);
    }
    const applyBtn = document.createElement("button");
    applyBtn.type = "button";
    applyBtn.className = "settings-btn";
    const refreshApply = () => {
      const n = applyArgs(p.preview.rows, p.checked).take.length;
      applyBtn.textContent = copyText("mcpSync.apply.action", {
        machine: machineName(to),
        n,
      });
      applyBtn.disabled = n === 0;
    };
    const names = { from: machineName(from), to: machineName(to) };
    for (const r of p.preview.rows)
      box.appendChild(this.renderRow(r, p.checked, names, refreshApply));
    refreshApply();
    applyBtn.addEventListener("click", () => void this.apply(applyBtn));
    box.appendChild(applyBtn);
    this.resultBox.replaceChildren(box);
  }

  private renderRow(
    r: McpSyncRow,
    checked: Set<string>,
    names: { from: string; to: string },
    changed: () => void,
  ): HTMLElement {
    const item = document.createElement("div");
    item.className = "mcp-server-item";
    const line = document.createElement("div");
    line.className = "mcp-server-row";
    if (selectable(r.state)) {
      const label = document.createElement("label");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = checked.has(r.name);
      cb.addEventListener("change", () => {
        if (cb.checked) checked.add(r.name);
        else checked.delete(r.name);
        changed();
      });
      label.append(
        cb,
        r.state === "differs"
          ? copyText("mcpSync.take.overwrite", { machine: names.to })
          : copyText("mcpSync.take.copy"),
      );
      line.appendChild(label);
    }
    const name = document.createElement("span");
    name.className = "mcp-server-name";
    name.textContent = r.name;
    const state = document.createElement("span");
    state.className = "mcp-server-summary";
    state.textContent = stateText(r.state, names.to);
    line.append(name, state);
    // 展开 / 收起同样是「挂不挂进去」（理由同面板本体）。
    const detail = document.createElement("div");
    const side = (label: string, value: unknown) => {
      if (value === null || value === undefined) return;
      const head = document.createElement("div");
      head.className = "mcp-server-src";
      head.textContent = label;
      const pre = document.createElement("pre");
      pre.className = "mcp-server-json";
      pre.textContent = JSON.stringify(value, null, 2);
      detail.append(head, pre);
    };
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "settings-btn mcp-json-toggle";
    toggle.textContent = copyText("mcpSync.row.showConfig");
    toggle.addEventListener("click", () => {
      if (detail.isConnected) {
        detail.remove();
        return;
      }
      // 懒建：展开第一次才序列化（同本页「JSON」那颗钮）。
      if (detail.childElementCount === 0) {
        side(copyText("mcpSync.row.sideOf", { machine: names.from }), r.source);
        side(copyText("mcpSync.row.sideOf", { machine: names.to }), r.target);
      }
      item.appendChild(detail);
    });
    line.appendChild(toggle);
    item.appendChild(line);
    for (const x of r.suspects) {
      const warn = document.createElement("div");
      warn.className = "settings-cc-profile-warn"; // 告警色左边线（本页既有的那一种提醒条）
      warn.textContent = suspectText(x, names.to);
      item.appendChild(warn);
    }
    return item;
  }

  private async apply(btn: HTMLButtonElement): Promise<void> {
    const p = this.pending;
    if (!p) return;
    const { take, overwrite } = applyArgs(p.preview.rows, p.checked);
    if (take.length === 0) return;
    const want = ++this.seq;
    btn.disabled = true;
    let done;
    try {
      done = await this.api.apply({
        to: p.to,
        toDir: p.toDir,
        sourceText: p.preview.sourceText,
        targetText: p.preview.targetText,
        take,
        overwrite,
      });
    } catch (e) {
      if (want !== this.seq) return;
      // 写失败（含「对面在你看差异之后又变了」）⇒ 这份差异作废，要重新看。
      this.pending = null;
      this.say(copyText("mcpSync.apply.failed", { reason: String(e) }), true);
      return;
    }
    if (want !== this.seq) return;
    this.pending = null;
    this.say(
      done.written
        ? copyText("mcpSync.apply.done", {
            path: done.path,
            names: done.names.join(copyText("mcpSync.apply.listSep")),
          })
        : copyText("mcpSync.apply.unchanged", { machine: machineName(p.to) }),
    );
    // 拉（写的是本页这台）⇒ 本页的列表要重读。
    if (done.written && p.to === this.here().origin) this.onWroteHere();
  }
}
