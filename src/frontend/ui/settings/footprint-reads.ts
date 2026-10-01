/**
 * 「足迹」**经通道问那台后端要成品**（帧命令 `footprint-report`，本体 `src/backend/footprint/`）。
 *
 * - 远端那一台：一问即得（住 monitor 那台的那一族不进人群）。
 * - 本机那一栏：`HostScope::Client` 那一族（monitor 自己那台的东西）要的、只有 monitor 知道的事实（它自己进程的家目录 · agent 家 · PATH）
 *   先问 monitor 一次（`footprint_client_facts`），原样带给本机后端，一问出整份报告（同一台、同一用户，stat 在后端；两拍）。
 * - 形状严格收（多一格 / 少一格 / 类型不对 ⇒「两端契约对不上」，不猜）；线上形状由跨语言金样 `footprint-report.golden.json` 钉着。
 * 〔墓碑 —— 从前是 Tauri 命令 `config_surface_report`〔散文墓碑〕（判定住 monitor），类型是 ts-rs 生成物。〕
 */
import { chan, ChanError } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import { commands } from "../ipc/commands";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";

/** app 与一个环境项的关系（四档，后端 `footprint/registry.rs::EnvTier` 派生）。 */
export type EnvTier = "AppInstalls" | "AppShipsNoInstallerYet" | "UserInstallsWePrompt" | "AppOnlyChecks";
/** 现状。没有「疑似缺失」这一档。 */
// `expected_absent`：该不在、确实不在（旧版遗留那一档）—— 结论是后端给的（`footprint/rows.rs::read_absence`），界面照档画。
export type SurfaceState =
  | { kind: "present"; detail: string }
  | { kind: "absent" }
  | { kind: "undetermined"; why: string }
  | { kind: "expected_absent"; detail: string };
/** 表格里的一行。 */
export interface SurfaceRow {
  tool_id: string;
  tool_name: string;
  tier: EnvTier;
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
/** settings 的一个作用域（「会影响钩子诊断结论」的文件）。 */
export interface SettingsScope {
  scope: string;
  path: string;
  state: SurfaceState;
  has_cc_bus_hooks: boolean | null;
  precedence_note: string;
}
/** 一次足迹的完整回报。 */
export interface ConfigSurfaceReport {
  rows: SurfaceRow[];
  settings_scopes: SettingsScope[];
  /** 解析基准，展示用（让用户知道 `~/.claude` 被解释成了哪里）。 */
  claude_config_dir: string;
  home: string;
}

/** 一问的期限（值归发起方，DL1）：那台 stat 几十条路径、读一两份小文件，秒级；给足余量。 */
export const FOOTPRINT_BUDGET_MS = 20_000;

const TIERS: readonly string[] = ["AppInstalls", "AppShipsNoInstallerYet", "UserInstallsWePrompt", "AppOnlyChecks"];

function bad(): never {
  throw new Error(copyText("configSurface.refresh.badShape"));
}

function obj(v: unknown, keys: string): Record<string, unknown> {
  if (v === null || typeof v !== "object" || Array.isArray(v)) bad();
  const o = v as Record<string, unknown>;
  if (Object.keys(o).sort().join(",") !== keys) bad();
  return o;
}

const str = (v: unknown): string => (typeof v === "string" ? v : bad());
const strOrNull = (v: unknown): string | null => (v === null ? null : str(v));
const bool = (v: unknown): boolean => (typeof v === "boolean" ? v : bad());

function decodeState(v: unknown): SurfaceState {
  const kind = (v as { kind?: unknown } | null)?.kind;
  if (kind === "absent") {
    obj(v, "kind");
    return { kind };
  }
  if (kind === "present") return { kind, detail: str(obj(v, "detail,kind").detail) };
  if (kind === "undetermined") return { kind, why: str(obj(v, "kind,why").why) };
  if (kind === "expected_absent") return { kind, detail: str(obj(v, "detail,kind").detail) };
  return bad();
}

function decodeRow(v: unknown): SurfaceRow {
  const o = obj(
    v,
    "effect_label,host_label,installable,note,path_declared,path_resolved,source_label,state,tier,tool_id,tool_name,uninstallable",
  );
  const tier = str(o.tier);
  if (!TIERS.includes(tier)) bad();
  return {
    tool_id: str(o.tool_id),
    tool_name: str(o.tool_name),
    tier: tier as EnvTier,
    source_label: str(o.source_label),
    path_declared: str(o.path_declared),
    path_resolved: strOrNull(o.path_resolved),
    note: strOrNull(o.note),
    host_label: str(o.host_label),
    effect_label: str(o.effect_label),
    state: decodeState(o.state),
    installable: bool(o.installable),
    uninstallable: bool(o.uninstallable),
  };
}

function decodeScope(v: unknown): SettingsScope {
  const o = obj(v, "has_cc_bus_hooks,path,precedence_note,scope,state");
  return {
    scope: str(o.scope),
    path: str(o.path),
    state: decodeState(o.state),
    has_cc_bus_hooks: o.has_cc_bus_hooks === null ? null : bool(o.has_cc_bus_hooks),
    precedence_note: str(o.precedence_note),
  };
}

/** `footprint-report` 的应答（整份报告），严格收。 */
export function decodeFootprint(v: unknown): ConfigSurfaceReport {
  const r = obj(v, "claude_config_dir,home,rows,settings_scopes");
  if (!Array.isArray(r.rows) || !Array.isArray(r.settings_scopes)) bad();
  return {
    rows: r.rows.map(decodeRow),
    settings_scopes: r.settings_scopes.map(decodeScope),
    claude_config_dir: str(r.claude_config_dir),
    home: str(r.home),
  };
}

/** 那台后端比这条命令老（不认它）时那句话。 */
const OLD_BACKEND = (): string => copyText("configSurface.backend.tooOld");

/** 那台的后端答不了这一问（不认这条命令）—— 界面说「这台还答不了」，不当失败弹。 */
export class FootprintUnanswered extends Error {}

async function ask(origin: Origin, args: Record<string, unknown>): Promise<ConfigSurfaceReport> {
  const body = jsonBody(args);
  const budget = budgetWithin(FOOTPRINT_BUDGET_MS);
  let reply: Uint8Array;
  try {
    reply = await chan.call(origin, "footprint-report", body, budget);
  } catch (e) {
    const unanswered = e instanceof ChanError && e.error.layer === "peer" && e.error.why === "unsupported";
    throw unanswered ? new FootprintUnanswered(saidOf(e, OLD_BACKEND())) : new Error(saidOf(e, OLD_BACKEND()));
  }
  return decodeFootprint(readJson(reply));
}

/** 按机器问足迹（本机那一栏：monitor 事实一次 ＋ 本机后端一次；远端一次；见头注）。问不到 / 形状不对 ⇒ 抛一句人话。 */
export async function readFootprint(origin: Origin): Promise<ConfigSurfaceReport> {
  if (!isLocalOrigin(origin)) return ask(origin, {});
  const client = await commands.footprint_client_facts();
  return ask(LOCAL_ORIGIN, { client });
}
