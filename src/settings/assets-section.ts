/**
 * 〔AS2 · 第四波 4B · V113〕机器页「资产目录」一块：别的机器有、这台还没有（或不一样）的 skill 与项目级 MCP，**装要你点**。
 *
 * 用户 2026-09-25 裁（`99 §1` V113，逐字）：「比如本机后端在本机看见一个skill并记录下来, 就会和远端后端同步,
 * 这样远端后端也能在远端装skill或者mcp / mcp保持项目级别」·「目录自动同步，装要你点」。仍有效 V112：内容原样拷过去并标出可疑项。
 *
 * # 本文件只做排版与手势（`设计/01 §1.1`）
 *
 * - **目录怎么对上**：这一块看得见时先让本机常驻后端对这台做一趟同步（`assets_sync`），再问**这台**的后端要目录
 *   （帧命令 `assets-catalog`）。「这台缺什么」（`missing` / `differs` / `same`）是这台后端答的，这里只照着画。
 * - **装**：MCP 走 AS1 那条路原样（`mcp_sync_preview` / `mcp_sync_apply`，只看这一条、勾「盖掉」才盖）；
 *   skill 走 `skill_install_preview` / `skill_install_apply`（判定在要被写的那台后端）。
 *   🔴 这四条「装」命令**由宿主递进来**（`mcp-section.ts::assetInstallApi`）：「装 MCP / skill」那一件的前端落点
 *   钉在一张名单上（`tests/evidence/K-R117-ruler.py` 的 `R9a`，只许缩），本文件不给它加一份新落点。
 * - **来源那台够不到**（没连上）⇒ 说清、不装：目录里不带原文（MCP 的密钥值更不带），装的那一下要从来源那台现读。
 *
 * 〔SU1 · 第四波 4C · V116〕**卸**：用户裁「要，只删装时写进去的文件」（装完改过的先问）。这一块末尾多一小节
 * 「从别的机器装来的 skill」：列这台后端记着的（帧命令 `skill-installs`），每条一颗「卸」→ 看（`skill-uninstall-plan`，
 * 逐文件的态与「要不要问」都是这台后端答的）→ 勾 → 卸（`skill_uninstall_apply`，同样由宿主递进来）。只删文件，目录留着。
 *
 * 纯函数（`decodeCatalog` · `reachOf` · `skillDefaultTake` · `skillApplyArgs` · `hereText` · `skillSuspectText` ·
 * `decodeInstalls` · `decodeUninstallPlan` · `uninstallDefaultTake` · `uninstallApplyArgs`）零 DOM，node 可测。
 */
import { commands } from "../ipc/commands";
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { copyText } from "../copy-table";
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { stateText, suspectText } from "./mcp-sync";
import type { AssetsSynced } from "../generated/AssetsSynced";
import type { McpSyncPreview } from "../generated/McpSyncPreview";
import type { SkillInstallPreview } from "../generated/SkillInstallPreview";
import type { SkillInstallRow } from "../generated/SkillInstallRow";
import type { SkillInstallSuspect } from "../generated/SkillInstallSuspect";

/** 「装」那几条命令（宿主递进来；见头注）。 */
export interface AssetInstallApi {
  dirs: typeof commands.list_mcp_project_dirs;
  mcpPreview: typeof commands.mcp_sync_preview;
  mcpApply: typeof commands.mcp_sync_apply;
  skillPreview: typeof commands.skill_install_preview;
  skillApply: typeof commands.skill_install_apply;
  /** 〔SU1〕卸：删经那台后端 `files-delete`（带 `expect`），删掉的从装记录里摘掉。 */
  skillUninstall: typeof commands.skill_uninstall_apply;
}

/** 目录里别处的一条来源。 */
export interface CatalogFrom {
  machine: string;
  project: string | null;
  digest: string;
  summary: Record<string, unknown>;
}

/** 「别的机器有的，这台怎样」一行（后端 `asset_catalog::rows`，键名一字不差）。 */
export interface CatalogRow {
  kind: string;
  name: string;
  state: string;
  from: CatalogFrom[];
}

export interface Catalog {
  self: string;
  labels: Map<string, string>;
  rows: CatalogRow[];
  problems: string[];
}

/** 这台对那一条的三态 → 给人看的字。**键集 == 后端 `asset_catalog::HERE_STATES`**（vitest 从后端源码现抠、两向相等）。 */
export const HERE_TEXT: Record<string, () => string> = {
  missing: () => copyText("assets.here.missing"),
  differs: () => copyText("assets.here.differs"),
  same: () => copyText("assets.here.same"),
};

/** 种类 → 给人看的字。**键集 == 后端 `asset_catalog::KINDS`**（同上）。 */
export const KIND_TEXT: Record<string, () => string> = {
  skill: () => copyText("assets.kind.skill"),
  mcp: () => copyText("assets.kind.mcp"),
};

export function hereText(state: string): string {
  const f = HERE_TEXT[state];
  return f ? f() : state;
}

const unreadable = (what: string): never => {
  console.warn(`[assets-section] assets-catalog 的应答形状不对：${what}`);
  throw new Error(copyText("assets.load.shape"));
};

/** 后端的目录 ⇒ [`Catalog`]。缺格 / 类型不对 ⇒ 抛（不猜默认值）。 */
export function decodeCatalog(v: unknown): Catalog {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return unreadable("不是对象");
  const o = v as Record<string, unknown>;
  if (typeof o.self !== "string" || !Array.isArray(o.machines) || !Array.isArray(o.rows) || !Array.isArray(o.problems)) {
    return unreadable("顶层缺格");
  }
  const labels = new Map<string, string>();
  for (const m of o.machines) {
    const mm = m as Record<string, unknown>;
    if (typeof mm?.id !== "string" || typeof mm?.label !== "string") return unreadable("machines 那一格");
    labels.set(mm.id, mm.label);
  }
  const rows = o.rows.map((r): CatalogRow => {
    const rr = r as Record<string, unknown>;
    if (typeof rr?.kind !== "string" || typeof rr.name !== "string" || typeof rr.state !== "string" || !Array.isArray(rr.from)) {
      return unreadable("rows 那一格");
    }
    const from = rr.from.map((f): CatalogFrom => {
      const ff = f as Record<string, unknown>;
      const project = ff?.project;
      if (typeof ff?.machine !== "string" || typeof ff.digest !== "string" || !(project === null || project === undefined || typeof project === "string")) {
        return unreadable("from 那一格");
      }
      const summary = ff.summary && typeof ff.summary === "object" ? (ff.summary as Record<string, unknown>) : {};
      return { machine: ff.machine, project: (project as string | undefined) ?? null, digest: ff.digest, summary };
    });
    return { kind: rr.kind, name: rr.name, state: rr.state, from };
  });
  const problems = o.problems.map((p) => (typeof p === "string" ? p : unreadable("problems 那一格")));
  return { self: o.self, labels, rows, problems };
}

/** 目录里的一台机器从这个界面够不够得到。 */
export type Reachability = { reachable: true; at: Origin } | { reachable: false };

/**
 * 目录里的机器 id → 界面上的 origin：本机后端的 id ⇒ `LOCAL_ORIGIN`；可达表里有的 ⇒ 那台的 origin；
 * 都不是（那台现在没连上 / 这个本机后端没见过它）⇒ 够不到（装不了，要说清）。
 * ⚠ 「够不到」是一个明说的态，不是一个空的 origin（origin 没有「没说」这一档，`ipc/origin.ts`）。
 */
export function reachOf(machine: string, synced: AssetsSynced | null): Reachability {
  if (synced && synced.self !== null && synced.self === machine) return { reachable: true, at: LOCAL_ORIGIN };
  const hit = synced?.reach.find((r) => r.machine === machine);
  return hit ? { reachable: true, at: hit.origin } : { reachable: false };
}

/** skill 可疑项 → 那句话。**键集 == 后端 `skill_install::SUSPECT_KINDS`**（同上）。 */
export const SKILL_SUSPECT_TEXT: Record<string, (x: SkillInstallSuspect, to: string) => string> = {
  executable: () => copyText("assets.suspect.executable"),
  binary: () => copyText("assets.suspect.binary"),
  "abs-path": (x, to) =>
    x.there === "present"
      ? copyText("assets.suspect.absPresent", { value: x.value, machine: to })
      : x.there === "absent"
        ? copyText("assets.suspect.absAbsent", { value: x.value, machine: to })
        : x.there === "foreign"
          ? copyText("assets.suspect.absForeign", { value: x.value, machine: to })
          : copyText("assets.suspect.absUnknown", { value: x.value, machine: to }),
  "command-missing": (x, to) =>
    x.there === "unknown"
      ? copyText("assets.suspect.commandUnknown", { value: x.value, machine: to })
      : copyText("assets.suspect.commandMissing", { value: x.value, machine: to }),
};

export function skillSuspectText(x: SkillInstallSuspect, to: string): string {
  const f = SKILL_SUSPECT_TEXT[x.kind];
  return f ? f(x, to) : copyText("assets.suspect.other", { value: `${x.kind} ${x.value}` });
}

/** 这一个能不能勾：只有 `new` / `differs`，且来源有原文（不是 binary）、这台那一份盖得了。 */
export function skillSelectable(r: SkillInstallRow): boolean {
  return (r.state === "new" || r.state === "differs") && r.blocked === null && !r.suspects.some((s) => s.kind === "binary");
}

/** 默认勾选：只勾「这台没有」的。「这台不同」的**默认不勾** —— 盖不盖要用户自己说（同 AS1）。 */
export function skillDefaultTake(rows: readonly SkillInstallRow[]): Set<string> {
  return new Set(rows.filter((r) => r.state === "new" && skillSelectable(r)).map((r) => r.path));
}

/** 勾选 → 交给后端的两张单子：`take` = 勾了的；`overwrite` = 勾了的里「这台不同」的那几个。 */
export function skillApplyArgs(rows: readonly SkillInstallRow[], checked: ReadonlySet<string>): { take: string[]; overwrite: string[] } {
  const picked = rows.filter((r) => skillSelectable(r) && checked.has(r.path));
  return { take: picked.map((r) => r.path), overwrite: picked.filter((r) => r.state === "differs").map((r) => r.path) };
}

// ── 〔SU1〕卸：纯函数 ──────────────────────────────────────────────────────────

/** 这台记着的一个「从别处装来的 skill」（后端 `skill-installs`，键名一字不差）。 */
export interface SkillInstalled {
  dir: string;
  name: string;
  files: number;
}

/** 卸时记录里的一个文件（后端 `skill-uninstall-plan` 的 `rows`）。 */
export interface UninstallRow {
  path: string;
  state: string;
  created: boolean;
  deletable: boolean;
  ask: boolean;
}

export interface UninstallPlan {
  dir: string;
  name: string;
  rows: UninstallRow[];
  /** 能删的那几份现有原文：卸的时候原样送回去当 CAS 期望。 */
  seen: { path: string; text: string }[];
}

const unreadableReply = (what: string): never => {
  console.warn(`[assets-section] skill 卸那两问的应答形状不对：${what}`);
  throw new Error(copyText("assets.load.shape"));
};

/** `skill-installs` 的应答 ⇒ 列表。缺格 / 类型不对 ⇒ 抛（不猜成「什么都没装过」）。 */
export function decodeInstalls(v: unknown): SkillInstalled[] {
  const o = v as Record<string, unknown> | null;
  if (!o || typeof o !== "object" || !Array.isArray(o.installs)) return unreadableReply("installs");
  return o.installs.map((i) => {
    const r = i as Record<string, unknown>;
    if (typeof r?.dir !== "string" || typeof r.name !== "string" || typeof r.files !== "number") return unreadableReply("installs 那一格");
    return { dir: r.dir, name: r.name, files: r.files };
  });
}

/** `skill-uninstall-plan`（不带 `take`）的应答 ⇒ [`UninstallPlan`]。缺格就抛。 */
export function decodeUninstallPlan(v: unknown): UninstallPlan {
  const o = v as Record<string, unknown> | null;
  if (!o || typeof o !== "object" || typeof o.dir !== "string" || typeof o.name !== "string" || !Array.isArray(o.rows) || !Array.isArray(o.seen)) {
    return unreadableReply("顶层缺格");
  }
  const rows = o.rows.map((r): UninstallRow => {
    const rr = r as Record<string, unknown>;
    if (
      typeof rr?.path !== "string" ||
      typeof rr.state !== "string" ||
      typeof rr.created !== "boolean" ||
      typeof rr.deletable !== "boolean" ||
      typeof rr.ask !== "boolean"
    ) {
      return unreadableReply("rows 那一格");
    }
    return { path: rr.path, state: rr.state, created: rr.created, deletable: rr.deletable, ask: rr.ask };
  });
  const seen = o.seen.map((s) => {
    const ss = s as Record<string, unknown>;
    if (typeof ss?.path !== "string" || typeof ss.text !== "string") return unreadableReply("seen 那一格");
    return { path: ss.path, text: ss.text };
  });
  return { dir: o.dir, name: o.name, rows, seen };
}

/** 卸时一个文件的态 → 给人看的字。**键集 == 后端 `skill_install::UNINSTALL_STATES`**（vitest 从后端源码现抠、两向相等）。 */
export const UNINSTALL_STATE_TEXT: Record<string, () => string> = {
  intact: () => copyText("assets.uninstallState.intact"),
  modified: () => copyText("assets.uninstallState.modified"),
  gone: () => copyText("assets.uninstallState.gone"),
  unreadable: () => copyText("assets.uninstallState.unreadable"),
};

/** 默认勾选：能删、且这台后端没说「要问」的。要问的（装完改过 / 装之前就在）**默认不勾** —— 删不删要用户自己说。 */
export function uninstallDefaultTake(rows: readonly UninstallRow[]): Set<string> {
  return new Set(rows.filter((r) => r.deletable && !r.ask).map((r) => r.path));
}

/** 勾选 → 交给后端的两张单子：`take` = 勾了的（能删的）；`confirm` = 勾了的里「要问」的那几个（勾它就是点名确认）。 */
export function uninstallApplyArgs(rows: readonly UninstallRow[], checked: ReadonlySet<string>): { take: string[]; confirm: string[] } {
  const picked = rows.filter((r) => r.deletable && checked.has(r.path));
  return { take: picked.map((r) => r.path), confirm: picked.filter((r) => r.ask).map((r) => r.path) };
}

/** 读目录的期限：同步那一趟另算（`assets_sync` 在 monitor 那侧有自己的预算）。 */
const CATALOG_BUDGET_MS = 30_000;

function machineName(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("assets.machine.local") : origin;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function button(text: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", "settings-btn settings-btn-secondary", text);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

/** 一条来源的摘要：skill 取 description；MCP 取 command ＋ args 或 url（与 `mcp-section.ts::serverSummary` 同口径的简版）。 */
function summaryText(row: CatalogRow, f: CatalogFrom): string {
  const s = f.summary;
  if (row.kind === "skill") return typeof s.description === "string" ? s.description : "";
  if (typeof s.url === "string") return s.url;
  const args = Array.isArray(s.args) ? s.args.filter((a) => typeof a === "string").join(" ") : "";
  return typeof s.command === "string" ? `${s.command} ${args}`.trim() : "";
}

export class AssetsSection {
  readonly element: HTMLElement;
  private body!: HTMLElement;
  private wanted: Origin = getCurrentMachine();
  private loaded = false;
  private seq = 0;
  private synced: AssetsSynced | null = null;

  /**
   * `apiOf` 是**延迟取**的：构造时不碰宿主那一侧（宿主那个模块在别的测试里会被整块替身掉；
   * 这一块的构造不许因此跟着坏 —— `panel-block-isolation.vitest.ts` 的「一块坏只坏一块」）。
   */
  constructor(private readonly apiOf: () => AssetInstallApi) {
    this.element = this.build();
    // ST1「延后加载」：构造期零 I/O；切机器时已经放过第一发的话就当场问新那一台（同 `PluginsSection`）。
    subscribeMachine((origin) => {
      this.wanted = origin;
      if (this.loaded) void this.refresh();
    });
  }

  /** 宿主在这台机器的子页第一次可见时调（`设计/70 §5.3` 判据 2）。 */
  loadNow(): void {
    this.loaded = true;
    void this.refresh();
  }

  private build(): HTMLElement {
    const root = el("div", "settings-group settings-headless assets-section");
    root.appendChild(el("div", "settings-hint", copyText("assets.panel.hint")));
    const bar = el("div", "settings-row");
    bar.appendChild(button(copyText("assets.panel.refresh"), () => void this.refresh()));
    root.appendChild(bar);
    this.body = el("div", "assets-body");
    root.appendChild(this.body);
    return root;
  }

  private async refresh(): Promise<void> {
    const mine = ++this.seq;
    const origin = this.wanted;
    this.body.textContent = copyText("assets.load.loading");
    let syncError: string | null = null;
    try {
      this.synced = await commands.assets_sync({ origin });
    } catch (e) {
      // 同步没办成不挡「看这台的目录」：说一句，接着读（读到的是上一次对上时的样子）。
      syncError = e instanceof Error ? e.message : String(e);
    }
    let cat: Catalog | null = null;
    let catError: string | null = null;
    try {
      const budget = budgetWithin(CATALOG_BUDGET_MS);
      const body = jsonBody({});
      const reply = await chan.call(origin, "assets-catalog", body, budget);
      cat = decodeCatalog(readJson(reply));
    } catch (e) {
      catError = saidOf(e, copyText("assets.load.tooOld"));
    }
    // 〔SU1〕这台记着的「从别处装来的 skill」：与目录各问各的（目录读不出来不挡卸）。
    let installs: SkillInstalled[] | string;
    try {
      const budget = budgetWithin(CATALOG_BUDGET_MS);
      const body = jsonBody({});
      const reply = await chan.call(origin, "skill-installs", body, budget);
      installs = decodeInstalls(readJson(reply));
    } catch (e) {
      installs = saidOf(e, copyText("assets.uninstall.tooOld"));
    }
    if (mine !== this.seq) return;
    this.body.textContent = "";
    if (cat) this.render(origin, cat, syncError);
    else this.body.appendChild(el("div", "settings-hint", copyText("assets.load.failed", { reason: catError ?? "" })));
    this.body.appendChild(this.renderInstalled(origin, installs));
  }

  private render(origin: Origin, cat: Catalog, syncError: string | null): void {
    if (syncError) this.body.appendChild(el("div", "settings-hint", copyText("assets.sync.failed", { reason: syncError })));
    for (const s of this.synced?.synced ?? []) {
      if (s.error) this.body.appendChild(el("div", "settings-hint", copyText("assets.sync.oneFailed", { machine: s.origin, reason: s.error })));
    }
    for (const p of cat.problems) this.body.appendChild(el("div", "settings-hint", copyText("assets.load.problem", { reason: p })));
    const rows = cat.rows.filter((r) => r.state !== "same");
    if (rows.length === 0) {
      this.body.appendChild(el("div", "settings-hint", copyText("assets.list.empty")));
      return;
    }
    for (const row of rows) this.body.appendChild(this.renderRow(origin, cat, row));
  }

  private renderRow(here: Origin, cat: Catalog, row: CatalogRow): HTMLElement {
    const box = el("div", "plugins-row assets-row");
    const kind = KIND_TEXT[row.kind]?.() ?? row.kind;
    box.appendChild(el("div", "plugins-row-id", `${kind} · ${row.name}`));
    box.appendChild(el("div", "plugins-row-count", hereText(row.state)));
    for (const f of row.from) {
      const reach = reachOf(f.machine, this.synced);
      const label = reach.reachable ? machineName(reach.at) : (cat.labels.get(f.machine) ?? f.machine);
      const line = el("div", "settings-hint plugins-row-meta");
      const where = f.project ? copyText("assets.from.project", { machine: label, project: f.project }) : copyText("assets.from.machine", { machine: label });
      line.textContent = [where, summaryText(row, f)].filter((x) => x).join(copyText("assets.row.sep"));
      box.appendChild(line);
      const slot = el("div", "assets-install");
      if (!reach.reachable) {
        line.appendChild(document.createTextNode(copyText("assets.row.unreachable", { reason: copyText("assets.from.unreachable") })));
      } else if (reach.at !== here) {
        const from = reach.at;
        box.appendChild(slot);
        slot.appendChild(
          button(copyText("assets.install.action"), () =>
            row.kind === "mcp" ? void this.installMcp(slot, from, here, row.name, f) : void this.previewSkill(slot, from, here, row.name),
          ),
        );
      }
    }
    return box;
  }

  // ── MCP：AS1 那条路原样（只看这一条） ──────────────────────────────────────

  private async installMcp(slot: HTMLElement, from: Origin, to: Origin, name: string, f: CatalogFrom): Promise<void> {
    slot.textContent = "";
    const dirInput = el("input", "settings-input settings-input-wide");
    dirInput.placeholder = copyText("assets.mcp.dirPlaceholder");
    const listId = `assets-dirs-${Math.random().toString(36).slice(2)}`;
    const datalist = document.createElement("datalist");
    datalist.id = listId;
    dirInput.setAttribute("list", listId);
    slot.append(dirInput, datalist);
    void this.apiOf().dirs({ origin: to }).then(
      (dirs) => {
        for (const d of dirs) {
          const o = document.createElement("option");
          o.value = d;
          datalist.appendChild(o);
        }
      },
      () => {},
    );
    const out = el("div", "assets-preview");
    slot.appendChild(
      button(copyText("assets.preview.action"), () => {
        const toDir = dirInput.value.trim();
        if (!toDir) {
          out.textContent = copyText("assets.mcp.dirMissing");
          return;
        }
        void this.previewMcp(out, from, f.project ?? "", to, toDir, name);
      }),
    );
    slot.appendChild(out);
  }

  private async previewMcp(out: HTMLElement, from: Origin, fromDir: string, to: Origin, toDir: string, name: string): Promise<void> {
    out.textContent = copyText("assets.preview.loading");
    let p: McpSyncPreview;
    try {
      p = await this.apiOf().mcpPreview({ from, fromDir, to, toDir });
    } catch (e) {
      out.textContent = copyText("assets.preview.failed", { reason: e instanceof Error ? e.message : String(e) });
      return;
    }
    const row = p.rows.find((r) => r.name === name);
    out.textContent = "";
    if (!row) {
      out.textContent = copyText("assets.mcp.gone");
      return;
    }
    const toName = machineName(to);
    out.appendChild(el("div", "plugins-row-count", stateText(row.state, toName)));
    for (const s of row.suspects) out.appendChild(el("div", "settings-hint settings-cc-profile-warn", suspectText(s, toName)));
    if (row.state !== "new" && row.state !== "differs") return;
    let overwrite = false;
    if (row.state === "differs") {
      const lab = el("label", "settings-row");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.addEventListener("change", () => (overwrite = cb.checked));
      lab.append(cb, document.createTextNode(copyText("assets.take.overwrite", { machine: toName })));
      out.appendChild(lab);
    }
    const result = el("div", "settings-hint");
    out.appendChild(
      button(copyText("assets.apply.action", { machine: toName }), async () => {
        try {
          const done = await this.apiOf().mcpApply({
            to,
            toDir,
            sourceText: p.sourceText,
            targetText: p.targetText,
            take: [name],
            overwrite: overwrite ? [name] : [],
          });
          result.textContent = done.written ? copyText("assets.apply.mcpDone", { path: done.path }) : copyText("assets.apply.unchanged", { machine: toName });
        } catch (e) {
          result.textContent = copyText("assets.apply.failed", { reason: e instanceof Error ? e.message : String(e) });
        }
      }),
    );
    out.appendChild(result);
  }

  // ── 〔SU1〕从别的机器装来的 skill：卸 ────────────────────────────────────────

  private renderInstalled(here: Origin, installs: SkillInstalled[] | string): HTMLElement {
    const box = el("div", "assets-installed");
    box.appendChild(el("div", "plugins-row-id", copyText("assets.uninstall.head")));
    if (typeof installs === "string") {
      box.appendChild(el("div", "settings-hint", copyText("assets.uninstall.loadFailed", { reason: installs })));
      return box;
    }
    if (installs.length === 0) {
      box.appendChild(el("div", "settings-hint", copyText("assets.uninstall.none")));
      return box;
    }
    for (const i of installs) {
      const row = el("div", "plugins-row assets-row");
      row.appendChild(el("div", "plugins-row-id", i.name));
      row.appendChild(el("div", "settings-hint plugins-row-meta", copyText("assets.uninstall.meta", { dir: i.dir, n: String(i.files) })));
      const slot = el("div", "assets-install");
      slot.appendChild(button(copyText("assets.uninstall.action"), () => void this.previewUninstall(slot, here, i.dir)));
      row.appendChild(slot);
      box.appendChild(row);
    }
    return box;
  }

  private async previewUninstall(slot: HTMLElement, to: Origin, dir: string): Promise<void> {
    slot.textContent = copyText("assets.preview.loading");
    let p: UninstallPlan;
    try {
      const budget = budgetWithin(CATALOG_BUDGET_MS);
      const body = jsonBody({ dir });
      const reply = await chan.call(to, "skill-uninstall-plan", body, budget);
      p = decodeUninstallPlan(readJson(reply));
    } catch (e) {
      slot.textContent = copyText("assets.preview.failed", { reason: saidOf(e, copyText("assets.uninstall.tooOld")) });
      return;
    }
    slot.textContent = "";
    const toName = machineName(to);
    slot.appendChild(el("div", "settings-hint", copyText("assets.uninstall.previewHead", { dir: p.dir })));
    const checked = uninstallDefaultTake(p.rows);
    for (const r of p.rows) {
      const line = el("label", "settings-row");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = checked.has(r.path);
      cb.disabled = !r.deletable;
      cb.addEventListener("change", () => (cb.checked ? checked.add(r.path) : checked.delete(r.path)));
      const said = UNINSTALL_STATE_TEXT[r.state]?.() ?? r.state;
      line.append(cb, document.createTextNode(`${r.path} · ${said}`));
      slot.appendChild(line);
      // 「装完改过」那一句就是那一行的态本身；装之前就在的（装时盖掉了原有那一份）另说一句：删了回不到装之前。
      if (r.deletable && !r.created) slot.appendChild(el("div", "settings-hint settings-cc-profile-warn", copyText("assets.uninstall.askOverwrote")));
    }
    const result = el("div", "settings-hint");
    slot.appendChild(
      button(copyText("assets.uninstall.apply", { machine: toName }), async () => {
        const { take, confirm } = uninstallApplyArgs(p.rows, checked);
        try {
          const done = await this.apiOf().skillUninstall({ to, dir: p.dir, seen: p.seen, take, confirm });
          result.textContent = copyText("assets.uninstall.done", { n: String(done.deleted.length), dir: done.dir });
          if (done.recordFailed) result.textContent += " " + done.recordFailed;
        } catch (e) {
          result.textContent = copyText("assets.uninstall.failed", { reason: e instanceof Error ? e.message : String(e) });
        }
      }),
    );
    slot.appendChild(result);
  }

  // ── skill ────────────────────────────────────────────────────────────────

  private async previewSkill(slot: HTMLElement, from: Origin, to: Origin, name: string): Promise<void> {
    slot.textContent = copyText("assets.preview.loading");
    let p: SkillInstallPreview;
    try {
      p = await this.apiOf().skillPreview({ from, to, name });
    } catch (e) {
      slot.textContent = copyText("assets.preview.failed", { reason: e instanceof Error ? e.message : String(e) });
      return;
    }
    slot.textContent = "";
    const toName = machineName(to);
    slot.appendChild(el("div", "settings-hint", copyText("assets.skill.head", { dir: p.dir })));
    const checked = skillDefaultTake(p.rows);
    for (const r of p.rows) {
      const line = el("label", "settings-row");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = checked.has(r.path);
      cb.disabled = !skillSelectable(r);
      cb.addEventListener("change", () => (cb.checked ? checked.add(r.path) : checked.delete(r.path)));
      line.append(cb, document.createTextNode(`${r.path} · ${stateText(r.state, toName)}`));
      slot.appendChild(line);
      if (r.blocked) slot.appendChild(el("div", "settings-hint settings-cc-profile-warn", copyText("assets.skill.blocked", { reason: r.blocked })));
      for (const s of r.suspects) slot.appendChild(el("div", "settings-hint settings-cc-profile-warn", skillSuspectText(s, toName)));
    }
    const result = el("div", "settings-hint");
    slot.appendChild(
      button(copyText("assets.apply.action", { machine: toName }), async () => {
        const { take, overwrite } = skillApplyArgs(p.rows, checked);
        try {
          const done = await this.apiOf().skillApply({ to, name, source: p.source, target: p.target, take, overwrite });
          result.textContent = copyText("assets.apply.skillDone", { dir: done.dir, n: String(done.written.length) });
          if (done.chmodFailed.length > 0) {
            result.textContent += " " + copyText("assets.apply.chmodFailed", { paths: done.chmodFailed.join(copyText("assets.skill.listSep")) });
          }
          // 〔SU1〕装好了但没记下来 ⇒ 这一趟装的卸不掉，照原话说（那句话是 monitor 说的）。
          if (done.recordFailed) result.textContent += " " + done.recordFailed;
        } catch (e) {
          result.textContent = copyText("assets.apply.failed", { reason: e instanceof Error ? e.message : String(e) });
        }
      }),
    );
    slot.appendChild(result);
  }
}
