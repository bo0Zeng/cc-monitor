// T02：**配置面审计视图** —— 「cc-monitor 到底动过你哪些文件」。
//
// 这一页**只读**，而且是**按需读一次**（无轮询，红线）。它把
// `src/bridge/src/tool_registry.rs` 的**环境清单闭集**遍历成一张表：每一项 app 对它是什么
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
import { makeInfoIcon } from "./info-icon";
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { holdSkeletonHeight, makeSkeleton } from "./skeleton";
import { withPending } from "./pending";

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
  // 〔ST2 · `70 §11.4` #3〕原文后半「别当成它不在」是**开发者的认识论对冲**（`§2.1` 第 ④ 种）——
  //   区分本身是对的（`§2.2`：不许扫掉），只换位置：前半留在行上，后半进 ⓘ（`UNKNOWN_IS_NOT_ABSENT`）。
  return `${GAP_HEAD.unknown} —— 这一项本机查不动（见上面的原因）`;
}

/**
 * 〔ST2 · `70 §11.3.1`〕「查不动」那一句的后半，挪进 ⓘ 的那一段 —— **那条区分的全部内容住这里**。
 */
export const UNKNOWN_IS_NOT_ABSENT =
  "查不动不等于它不在：只是这台机器上判断不了，不说明它没装。";

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
    // 〔ST2 · `70 §11.4` #1 · `§11.3.1`〕原文「这一项该由 cc-monitor 自带，而安装入口还没写 —— 撤销也一样还没有」
    //   是**我们欠的实现写成产品文案**。改成一格状态；⚠ 「还没有」这个语义必须留（`KR65D2`：不许说成「不该由它装」）。
    case "AppShipsNoInstallerYet":
      return "暂无撤销：还没有安装入口";
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
 * 〔ST2 · `70 §11.4` #2 · `§11.3.1`〕原文「其中 N 项该由 cc-monitor 自带、而安装入口还没写：…」
 * 是「谁欠谁」的话。改成**一格状态 ＋ 展开看哪几项**：数照旧数得出来（`KR65D2` **不许删这一行**），
 * 名单挪进 `[哪 N 项]`（`owedInstallerNames`）。
 *
 * 返回 `null` = 一项都没有（那时整句不渲染，不写「0 项」）。
 */
export function summarizeOwedInstallers(rows: SurfaceRow[]): string | null {
  const names = owedInstallerNames(rows);
  if (names.length === 0) return null;
  return `⚠ ${names.length} 项还没有安装入口`;
}

/** 〔ST2〕`[哪 N 项]` 里那张名单（去重、按出现顺序）。 */
export function owedInstallerNames(rows: SurfaceRow[]): string[] {
  return [...new Set(rows.filter((r) => r.tier === "AppShipsNoInstallerYet").map((r) => r.tool_name))];
}

/** 生成一段可复制的纯文本诊断，便于用户贴给我或存档。 */
export function formatReportText(r: ConfigSurfaceReport): string {
  const lines: string[] = [];
  // 〔ST2 · 用户 09-24 裁「一起改」· `70 §11.6` #4〕跟块名统一：「配置面审计」→「足迹」。
  lines.push("== cc-monitor 足迹 ==");
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
    lines.push(`  ${owed}：${owedInstallerNames(r.rows).join("、")}`);
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

/**
 * 〔第四波 ST2 · 用户 09-24 裁「远端也有真栏」· `设计/70 §10.1` ② · `§8` #12〕**按机器去问足迹**。
 *
 * 读口是 RM1a 那一路补的（后端 `config_surface` 收 origin）；本路只接界面。约定的形状：
 * `config_surface_report({ origin })`，本机传 `LOCAL_ORIGIN`，回来的报告**带回它答的是哪台**（`origin`）。
 *
 * ⚠⚠ **回声校验是这一格的全部重量**：读口合进来之前，包装层那条命令不收参数 ——
 *   参数会被**静默丢掉**，后端照旧回本机那一份。那时候照原样画，就是拿本机的答案冒充 aya 的
 *   （`§10.1` 逐字要防的那一形）。⇒ 远端那一台：报告里的 `origin` 与所问**相等**才算数，
 *   否则当作「这台还答不了」；本机：没带 `origin`（旧读口）或带的是本机，才算数。
 *
 * ⚠ 这里经 `as unknown as` 调：今天包装层那条的签名是零参数的，RM1a 改签名之后这一句不用动。
 *   合并时若 RM1a 的参数形状不是 `{ origin }`，改这一处（主会话的活，报告里写着）。
 */
export async function readFootprint(origin: Origin): Promise<ConfigSurfaceReport> {
  const ask = commands.config_surface_report as unknown as (a: {
    origin: Origin;
  }) => Promise<ConfigSurfaceReport>;
  // 〔C4a〕共用 store 里本机就是 `LOCAL_ORIGIN`（不再是 `null`）⇒ 原样过线。
  return ask({ origin });
}

/** 这份报告是不是**所问那台**的答复（见 `readFootprint` 的头注）。 */
export function answersFor(r: ConfigSurfaceReport, origin: Origin): boolean {
  const said = (r as { origin?: unknown }).origin;
  // 本机：旧读口不带 `origin`（缺省 / null 那是**线上**的旧形，不是 TS 侧的本机表示）或带的是本机，才算数。
  if (isLocalOrigin(origin)) return said === undefined || said === null || said === LOCAL_ORIGIN;
  return said === origin;
}

/** 远端那一台答不了时那一句的 ⓘ —— 区分（答不出来 ≠ 没动过）只换位置（`§11.4` #4）。 */
export const REMOTE_UNANSWERED_WHY = "「答不出来」不等于「它没动过你的文件」。";

export class ConfigSurfaceSection {
  readonly element: HTMLElement;
  private body!: HTMLElement;
  private scopesBox!: HTMLElement;
  private meta!: HTMLElement;
  /** `KR65D2`：「app 该自带而还没有装口」那一格的计数行。空时整行不显示。 */
  private owed!: HTMLElement;
  private copyBtn!: HTMLButtonElement;
  /**
   * 那一整套（工具栏 / 概览 / 表 / 作用域）的包装 ——〔ST2〕本机与远端都用它；远端答不了时收起来。
   * **刻意不挂类名**：它只负责显隐，不需要任何样式；挂了类就得在 CSS 里给它写规则，
   * 而 `css-ledger` 会要求每个类说得出谁在用它、每处类引用都有规则。
   */
  private localOnly!: HTMLElement;
  /** 远端那一台答不了时那一句「这台机器还查不了，为什么」（`showUnanswered`）。平时是空的。 */
  private notForThisMachine!: HTMLElement;
  private last: ConfigSurfaceReport | null = null;
  private unsubscribeMachine?: () => void;
  /** 〔ST2〕宿主放过第一发没有（放过之后切机器才由订阅重读）。 */
  private started = false;
  /** 〔ST2〕第几趟读 —— 晚到的旧答复认得出来。 */
  private seq = 0;
  /** 〔ST2〕本机那一套里最后一格：装终端集成时留下的 `$PROFILE` 备份在哪。 */
  private backups!: HTMLElement;

  constructor() {
    this.element = this.build();
    // 🔴 步 2（`70 §1.3 B` · `§10.4`）：**构造期不再发 I/O。**
    // 原来这里是 `void this.refresh()`，而这一块住的页**不是落地页**
    //（落地页是 `machines`）⇒ 每次打开设置都白发一趟 `config_surface_report`。
    // 现在由宿主（`panel.ts`）在这一块**真正被搬到用户正在看的那一页上**时调 `loadNow()`。
    //
    // 🔴 步 14a（`70 §10.1`）：这一块已从顶层「改动足迹」页搬进**机器子页的第五栏「足迹」**。
    // ⇒ 「哪台机器」这件事从此由页面上下文回答，所以这里订阅 `machine-context`。
    this.unsubscribeMachine = subscribeMachine((origin) => this.onMachineChanged(origin));
    this.applyOriginGate();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless config-surface-section";

    // 🔴 `70 §10.1` 差项 3：原来这里第二句逐字是「这一页只读：不会写任何东西，
    //    也不后台轮询——每次打开或点「重新扫描」才读一次」——那是**我们的设计承诺**，
    //    写给评审看的，不是用户要的信息（用户不需要知道我们承诺了不轮询）。
    // ⇒ 只留「这一页是什么」，承诺收进 ⓘ。
    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent = "cc-monitor 会碰你哪些文件、对它做什么、现在什么状态、还能不能撤。";
    hint.appendChild(
      makeInfoIcon(
        "只读：这一页不会写任何东西，也不在后台轮询——每次打开或点「重新扫描」才读一次。",
      ),
    );
    root.appendChild(hint);

    // 🔴 `70 §10.1` 那条 ⚠ 逐字：**这一段不许跟着一起扫掉。**
    // 它属于 `§2.2` 那一档 ——「查不了」与「没有」的**区分本身是对的**
    //（`readiness.ts` 的 `GAP_HEAD` 是这对词的唯一住址，本页是它的第二个读者）。
    // ⇒ 按 `§2.3` 的形状办：**区分保留，换成界面状态** —— 每一行今天各自已经带着
    //   自己的 `why` 与 host 徽章，所以这里这段**通用免责**收进 ⓘ，不占正文一整段。
    const honesty = document.createElement("div");
    honesty.className = "settings-hint config-surface-honesty";
    honesty.textContent = "查不了的写成「未确定」并说明原因，不画成红叉。";
    honesty.appendChild(
      makeInfoIcon(
        "远端路径要 SSH（请到部署向导里查）、项目里的 .mcp.json 得先知道是哪个项目、" +
          "Windows 的 $PROFILE 由 PowerShell 决定——这三类本机无从判断，" +
          "报成「缺失」会是假警报。",
      ),
    );
    root.appendChild(honesty);

    // 步 14a：远端那一台答不了时那一句「这台机器还查不了，为什么」（见 `showUnanswered`）。
    this.notForThisMachine = document.createElement("div");
    this.notForThisMachine.className = "settings-hint";
    this.notForThisMachine.dataset.footprintUnanswered = "";
    this.notForThisMachine.hidden = true;
    root.appendChild(this.notForThisMachine);

    // 本机那一整套装在这个包装里（见 `localOnly` 字段的头注：为什么不挂类名）。
    this.localOnly = document.createElement("div");
    root.appendChild(this.localOnly);
    const host = this.localOnly;

    const bar = document.createElement("div");
    bar.className = "settings-row config-surface-bar";
    const rescan = document.createElement("button");
    rescan.type = "button";
    rescan.className = "btn";
    rescan.textContent = "重新扫描";
    // 步 4·E（`70 §1.3 E`）：扫一趟是一次真往返 —— 期间按住这个按钮，
    // 否则连点两下就是两趟，而第二趟的结果会盖掉第一趟、屏幕上看不出来。
    rescan.addEventListener("click", () => void withPending(rescan, "扫描中…", () => this.refresh()));
    bar.appendChild(rescan);

    this.copyBtn = document.createElement("button");
    this.copyBtn.type = "button";
    this.copyBtn.className = "btn";
    this.copyBtn.textContent = "复制诊断文本";
    this.copyBtn.disabled = true;
    this.copyBtn.addEventListener("click", () =>
      void withPending(this.copyBtn, "复制中…", () => this.copy()),
    );
    bar.appendChild(this.copyBtn);
    host.appendChild(bar);

    this.meta = document.createElement("div");
    this.meta.className = "settings-hint config-surface-meta";
    host.appendChild(this.meta);

    this.owed = document.createElement("div");
    this.owed.className = "settings-hint config-surface-owed";
    this.owed.hidden = true;
    host.appendChild(this.owed);

    this.body = document.createElement("div");
    this.body.className = "config-surface-body";
    // 步 1（`70 §10.1` 差项 2）：加载态原来是**会长高**的一行字（「扫描中…」→ 整张表）
    // ⇒ 数据回来时这一页往下窜一屏。现在容器与骨架钉在同一个高度下限上。
    holdSkeletonHeight(this.body, "footprint");
    host.appendChild(this.body);

    const scopesT = document.createElement("div");
    scopesT.className = "settings-subtitle";
    scopesT.textContent = "settings.json 的各作用域";
    host.appendChild(scopesT);
    const scopesHint = document.createElement("div");
    scopesHint.className = "settings-hint";
    scopesHint.textContent =
      "钩子可以写在多个作用域里，优先级从低到高。钩子诊断读的是「用户级」那一份——" +
      "所以如果你把钩子写在了别处，那边报的「未装」可能是错的。";
    host.appendChild(scopesHint);
    this.scopesBox = document.createElement("div");
    this.scopesBox.className = "config-surface-scopes";
    host.appendChild(this.scopesBox);

    // 〔ST2 · `70 §10.2` / `§11.3.2`〕「PowerShell profile 备份」从「数据位置」搬到这里：
    //   它和上面那张表里的 `$PROFILE` 行讲的是同一件事（cc-monitor 动过你哪些文件），
    //   原来住在两个不同的顶层页。没备份过就整块不出现。
    this.backups = document.createElement("div");
    this.backups.dataset.profileBackups = "";
    host.appendChild(this.backups);

    return root;
  }

  /** 宿主拆掉这一块时要调 —— 不退订的话 store 里会留一个指向死节点的闭包。 */
  dispose(): void {
    this.unsubscribeMachine?.();
    this.unsubscribeMachine = undefined;
  }

  /**
   * 步 2：宿主在「per-machine 那几块第一次放 I/O」时调它（一次打开里一次，`panel.ts::loadPerMachineOnce`）。
   * 之后切机器由这一块自己的 `machine-context` 订阅重读（与账号 / MCP 那几块同一个形状）。
   *
   * 〔ST2〕原来它在远端页上**直接不读**（读口不收 origin）。今天远端页照样去问 —— 答没答对由回声判。
   */
  loadNow(): void {
    this.started = true;
    void this.refresh();
  }

  /**
   * 这一页现在该长什么样：本机 / 远端都先摆那张表（骨架）；远端答不了时换成一句为什么。
   *
   * 〔ST2〕今天它只管**切到哪台时先长什么样**；「这台答不了」那一格由 `refresh` 看回声之后定
   * （`showUnanswered`）。⚠ 这一格**不许**被读成「判据 #12 绿了」—— 只有回声对上、表真画出来才算。
   */
  private applyOriginGate(): void {
    this.notForThisMachine.replaceChildren();
    this.notForThisMachine.hidden = true;
    // ⚠ 只切**两个**节点的显隐，不逐块切。
    // 理由是一条实打出来的判据：`css-conventions.vitest.ts` 的 S30 ⑦ ——
    // 「会被 `hidden` 切的元素，CSS 不许在它自己身上裸写 `display`」。
    // 那条工具栏挂的是 `settings-row`，而 `.settings-row { display: flex }`
    // ⇒ 直接把 `hidden` 写到那条工具栏上是一句**空写**（作者样式压过 UA 的 `[hidden]`），
    //   ⚠ 这一句**刻意不写出那个赋值的字面形状** —— 那把尺子是词法的，
    //     散文里出现一次同形的字面量就会被它当成第 N 处真调用点（它自己的头注也栽过）。
    //   屏幕上它照样在。所以那一整套装进一个**不挂任何类**的包装里，切包装。
    this.localOnly.hidden = false;
  }

  /** 远端那一台答不了（读口没合进来 / 后端太旧 / 答的不是它）⇒ 表收起来，说一句为什么。 */
  private showUnanswered(origin: string): void {
    this.last = null;
    this.copyBtn.disabled = true;
    this.localOnly.hidden = true;
    this.notForThisMachine.hidden = false;
    this.notForThisMachine.replaceChildren(
      `这台机器（${origin}）的足迹还查不了：要那台机器上的后端来答，它这一版还答不了。`,
      makeInfoIcon(REMOTE_UNANSWERED_WHY),
    );
  }

  private onMachineChanged(_origin: Origin): void {
    this.applyOriginGate();
    // 〔ST2〕切了机器 ⇒ 这一块讲的是另一台了 ⇒ 重读（只在已经放过第一发之后；第一发归宿主）。
    if (this.started) void this.refresh();
  }

  async refresh(): Promise<void> {
    const origin = getCurrentMachine();
    // 切机器快过答复时，晚到的那一份不许盖掉当前这台的（与 `cc-bus-section` 那处同一个病）。
    const my = ++this.seq;
    this.applyOriginGate();
    this.body.replaceChildren(makeSkeleton("footprint", "正在扫这台机器上的足迹…"));
    try {
      const r = await readFootprint(origin);
      if (my !== this.seq) return;
      // **校验自己 IPC 的返回形状**（B03 的真 bug：`invoke` 可能 resolve 成 undefined，
      // 于后续 `.length` 当场抛，把整个 section 挂掉）。
      if (!r || !Array.isArray(r.rows) || !Array.isArray(r.settings_scopes)) {
        throw new Error(
          "后端返回的形状不对（rows / settings_scopes 不是数组）",
        );
      }
      if (!answersFor(r, origin)) {
        if (isRemoteOrigin(origin)) {
          this.showUnanswered(origin);
          return;
        }
        throw new Error("后端答的不是本机这一份");
      }
      this.last = r;
      this.copyBtn.disabled = false;
      this.render(r);
      // `$PROFILE` 备份只在本机那一页（`data_paths.rs` 只答本机）。
      if (isLocalOrigin(origin)) await this.loadBackups();
      else this.backups.replaceChildren();
    } catch (e) {
      if (my !== this.seq) return;
      this.last = null;
      this.copyBtn.disabled = true;
      this.body.textContent = `扫描失败：${String(e)}`;
      showActionFailureToast("扫描配置面", String(e));
    }
  }

  /**
   * 〔ST2〕`$PROFILE` 备份那一格。数据来自同一条 `get_data_paths`（`data_paths.rs` 是逐个落盘位置的唯一权威枚举点）。
   * 读不到 ⇒ 说读不到（不许拿「没有备份」糊过去 —— 那是替用户下一个没做过的结论）；
   * 一个备份都没有 ⇒ 整块不出现。
   */
  private async loadBackups(): Promise<void> {
    this.backups.replaceChildren();
    let dirs: { path: string }[];
    try {
      const d = await commands.get_data_paths();
      dirs = Array.isArray(d?.profileBackupDirs) ? d.profileBackupDirs : [];
    } catch (e) {
      const why = document.createElement("div");
      why.className = "settings-hint";
      why.textContent = `读不到 PowerShell profile 备份在哪：${String(e)}`;
      this.backups.appendChild(why);
      return;
    }
    if (dirs.length === 0) return;
    const title = document.createElement("div");
    title.className = "settings-subtitle";
    title.textContent = "PowerShell profile 备份";
    const note = document.createElement("div");
    note.className = "settings-hint";
    note.textContent = "装终端集成时，原来的 profile 先备份到同目录的 .ccm-backup-<时间戳>。想撤回就用它。";
    const list = document.createElement("ul");
    for (const d of dirs) {
      const li = document.createElement("li");
      li.textContent = d.path;
      list.appendChild(li);
    }
    this.backups.append(title, note, list);
  }

  private render(r: ConfigSurfaceReport): void {
    this.meta.textContent = `HOME=${r.home} · ~/.claude 解析为 ${r.claude_config_dir}`;
    // `KR65D2`：「app 该自带而还没有装口」那一格**在屏幕上数得出来**。
    // 〔ST2 · `§11.3.1`〕一格状态 ＋ `[哪 N 项]` 展开看名单。
    const owed = summarizeOwedInstallers(r.rows);
    this.owed.replaceChildren();
    if (owed !== null) {
      const names = owedInstallerNames(r.rows);
      this.owed.append(owed, " ");
      const which = document.createElement("details");
      which.dataset.owedNames = "";
      const sum = document.createElement("summary");
      sum.textContent = `哪 ${names.length} 项`;
      const list = document.createElement("span");
      list.textContent = names.join("、");
      which.append(sum, list);
      this.owed.appendChild(which);
    }
    this.owed.hidden = owed === null;
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
      // 〔ST2 · `§11.4` #3〕「查不动」那一档：区分的后半在 ⓘ 里（只换位置，不删义）。
      if (p.dataset.gap === "unknown") p.appendChild(makeInfoIcon(UNKNOWN_IS_NOT_ABSENT));
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
