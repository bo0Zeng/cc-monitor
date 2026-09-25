// P8a：Claude Code **插件面**的只读枚举。
//
// ## ★★ 这一页刻意**不说**「装了哪些插件」
//
// 摸底实测（08-12）：盘上根本没有那份真相源 —— `settings.json` 与 `~/.claude.json` 里
// `plugin`/`marketplace` 零命中；`~/.claude/plugins/marketplaces/<id>/` 带 `.gcs-sha`、
// **不是 git 仓**而是一份**下载快照**，里面那些目录是快照内容、**不是用户安装的**。
// 本机实测：manifest **声明 276 个**、快照目录里躺着 **39 个**。
//
// ⇒ 一个写着「已装 39 个插件」的界面**会在说假话**。所以这里只说三件读得到的事：
// **有哪些 marketplace、从哪来、它声明了多少个插件**。
// 「哪些插件真的在生效」是待决 `U10d`，界面**不猜**。
//
// ## 三条出口在界面上也必须分得开
//
// | 后端 | 这里显示成 |
// |---|---|
// | `file_absent: true` | 「这台机器**没有**登记任何 marketplace」 |
// | 这一问失败（〔C4b〕经通道） | 「**读不到**……（这不等于「没有」）」 |
// | `declared_plugins: null` | 「**读不到**插件数：<理由>」，**不是「0 个」** |
//
// 中间那条的措辞抄的是 `drift-ledger-section` 那句「读不到就说读不到 ——
// **不显示成「没有漂移」**（那是对用户撒谎）」，同一族的病本轮已收过三次。
//
// ## 只读、按需
//
// 一次问询，**不轮询**（红线，同 `config-surface-section.ts` / `drift-ledger-section.ts`）。
//
// ## 〔RM1b · 第四波〕跟着「当前在看哪台机器」走
//
// 读法搬进了后端（`plugins-marketplaces`，本机后端与远端后端同一个二进制），命令收 `origin` ⇒
// 本节与 MCP 那节一样订阅 `machine-context`：切到哪台就问哪台（切换即重读一次，不轮询）。
// 「这台机器」这几个字说的就是被选中的那一台 —— 本机页与远端页同一套话。
//
// ## 〔C4b · 第四波 4B〕经通道直接问那台机器的后端
//
// 此前是 monitor 的一条 Tauri 命令（`list_plugin_marketplaces`〔散文墓碑〕）：它问那台后端要「恰一行」，
// 再在 Rust 里核一遍形状（拒收未知字段 · 每个字段必填）。今天后端的帧应答**就是成品**（整份 survey），
// 本文件经 `chan.call` 直接问、按形状收（[`decodeSurvey`]：同一套严格口径，挪到了唯一的消费者这里），
// monitor 那条命令与那份核验一起删了。本机与远端同一条路。
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import type { Origin } from "../ipc/origin";

/**
 * 一个 marketplace（后端 `plugins_query::MarketplaceEntry`，键名一字不差）。**每个字段读不出就是 `null`，不编默认值。**
 * 跨语言金样：`tests/__fixtures__/plugins-survey.golden.json`（后端写、[`decodeSurvey`] 读）。
 */
export interface MarketplaceEntry {
  id: string;
  /** 形如 `github:anthropics/claude-plugins-official`。 */
  source: string | null;
  install_location: string | null;
  last_updated: string | null;
  /** 这个 marketplace **声明**的插件数。★★ `null` 的意思是**读不到**，**不是 0**。 */
  declared_plugins: number | null;
  /** `declared_plugins` 为 `null` 时**为什么**读不到 —— 装的就是那条错误原文。 */
  declared_error: string | null;
}

/** 一次枚举的结果。 */
export interface MarketplaceSurvey {
  entries: MarketplaceEntry[];
  /** `known_marketplaces.json` **不存在** ⇒ 这台机器一个 marketplace 都没有（**诚实的空**）。「读不到」到不了这里。 */
  file_absent: boolean;
}

const ENTRY_KEYS = ["declared_error", "declared_plugins", "id", "install_location", "last_updated", "source"];

/**
 * 后端的成品 ⇒ [`MarketplaceSurvey`]。口径与它上一个住址（monitor `plugins.rs` 的 `deny_unknown_fields` ＋ 必填）逐格相同：
 * **多一格、少一格（缺席不等于 `null`）、类型不对** ⇒ 抛「两端契约对不上」—— 两端一漂当场报错，不静默少一格。
 * 给人看的那句不带键名；哪一格不对只进日志。
 */
export function decodeSurvey(v: unknown): MarketplaceSurvey {
  const bad = (what: string): never => {
    console.warn(`[plugins-section] plugins-marketplaces 的应答形状不对：${what}`);
    throw new Error("插件市场清单的形状对不上（后端与界面版本不一致？重装那台机器的后端试试）");
  };
  if (v === null || typeof v !== "object" || Array.isArray(v)) return bad("不是一个对象");
  const o = v as Record<string, unknown>;
  if (Object.keys(o).sort().join(",") !== "entries,file_absent") return bad(`顶层的键是 ${Object.keys(o).sort().join(",")}`);
  if (typeof o.file_absent !== "boolean" || !Array.isArray(o.entries)) return bad("顶层两格的类型不对");
  const optStr = (x: unknown): x is string | null => x === null || typeof x === "string";
  const entries = o.entries.map((raw, i): MarketplaceEntry => {
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return bad(`第 ${i} 条不是对象`);
    const e = raw as Record<string, unknown>;
    if (Object.keys(e).sort().join(",") !== ENTRY_KEYS.join(",")) return bad(`第 ${i} 条的键是 ${Object.keys(e).sort().join(",")}`);
    const n = e.declared_plugins;
    const nOk = n === null || (typeof n === "number" && Number.isInteger(n) && n >= 0);
    if (typeof e.id !== "string" || !nOk || ![e.source, e.install_location, e.last_updated, e.declared_error].every(optStr)) {
      return bad(`第 ${i} 条有一格类型不对`);
    }
    return e as unknown as MarketplaceEntry;
  });
  return { entries, file_absent: o.file_absent };
}

/** 这一问的期限：与它上一个住址（monitor `frame_query::LINES_BUDGET`）同值 —— 读两份小 JSON ＋ 回程。 */
const SURVEY_BUDGET_MS = 30_000;

/** 问那台机器的后端：登记了哪些 marketplace。失败抛一句给人看的话（这一节把它挂在「读不到」那一行）。 */
export async function fetchSurvey(origin: Origin): Promise<MarketplaceSurvey> {
  try {
    const budget = budgetWithin(SURVEY_BUDGET_MS);
    const body = jsonBody({});
    const reply = await chan.call(origin, "plugins-marketplaces", body, budget);
    return decodeSurvey(readJson(reply));
  } catch (e) {
    throw new Error(saidOf(e, "那台机器上的后端版本旧，还读不了插件市场（重装后端之后就有）"));
  }
}

/**
 * 插件数那一格的文案。**纯函数，可单测。**
 *
 * ★ `null` 与 `0` 必须给出**不同**的话：两者都渲染成「0 个」正是本件要防的那句假话。
 */
export function declaredPluginsText(e: MarketplaceEntry): string {
  if (e.declared_plugins === null) {
    const why = e.declared_error ?? "（没说原因 —— 那是个 bug）";
    return `读不到插件数：${why}`;
  }
  return `声明 ${e.declared_plugins} 个插件`;
}

/** 更新时间给人看的形态；读不出就说读不出，不填今天。 */
export function lastUpdatedText(e: MarketplaceEntry): string {
  if (!e.last_updated) return "更新时间：未记";
  const d = new Date(e.last_updated);
  if (Number.isNaN(d.getTime())) return `更新时间：${e.last_updated}`;
  return `更新时间：${d.toLocaleString()}`;
}

export class PluginsSection {
  readonly element: HTMLElement;
  private body!: HTMLElement;
  /**
   * 代次守卫〔D 阶段补审〕。
   *
   * 这一页有**两个**入口会发请求：构造时读一次、「重新读取」按钮。
   * 慢的那次回来会**盖掉**后点的那次 —— 用户点了刷新却看见旧结果，而且没有任何提示。
   * 同族的病本轮已在 `P6b`（切机器时迟到的枚举覆盖新机器的清单）与
   * `P7b`（两个按钮各自异步）上各修过一次；`panorama.ts` 的 `searchSeq` 是更早的先例。
   */
  private seq = 0;
  /** 〔RM1b〕要看哪台（`machine-context` 的口径：`null` = 本机）。`loadNow()` 之前只记不读。 */
  private wanted = getCurrentMachine();
  private loaded = false;

  constructor() {
    this.element = this.build();
    // ST1「延后加载」：构造期不读 —— 见 `loadNow()`。
    // 〔RM1b〕切机器：已经放过第一发的话就当场问新那一台（同 `McpSection`；store 同值不通知）。
    subscribeMachine((origin) => {
      this.wanted = origin;
      if (this.loaded) void this.refresh();
    });
  }

  /**
   * ST1「延后加载」（`设计/70 §5.3` 判据 2：**子页内容只在该子页可见时才发 I/O**）：
   * 构造期不再发 I/O；宿主（`panel.ts`）在**某台机器的子页第一次可见**时调它。
   * 重开设置后宿主会再调一次（重开要看新读数）。
   */
  loadNow(): void {
    this.loaded = true;
    void this.refresh();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless plugins-section";

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    // ⚠ 这段话是本页的正题，**不是装饰**：它是「界面不许声称安装/启用」那条 DoD
    // 在用户那一侧的兑现。改它之前先读模块头注。
    hint.textContent =
      "这一页列出这台机器上登记的 Claude Code marketplace，以及每个 marketplace " +
      "自己声明的插件数。它回答的是「有哪些插件可以装」，" +
      "不是「装了哪些 / 启用了哪些」—— 后者今天在盘上没有真相源，" +
      "与其猜一个数字给你看，不如说清这一点。只读、按需读一次，不后台轮询。";
    root.appendChild(hint);

    const bar = document.createElement("div");
    bar.className = "settings-row";
    const refreshBtn = document.createElement("button");
    refreshBtn.className = "btn";
    refreshBtn.textContent = "重新读取";
    refreshBtn.addEventListener("click", () => void this.refresh());
    bar.appendChild(refreshBtn);
    root.appendChild(bar);

    this.body = document.createElement("div");
    this.body.className = "plugins-body";
    root.appendChild(this.body);
    return root;
  }

  private async refresh(): Promise<void> {
    const mine = ++this.seq;
    let survey: MarketplaceSurvey;
    try {
      survey = await fetchSurvey(this.wanted);
    } catch (e) {
      if (mine !== this.seq) return; // 迟到的失败也不许盖掉新结果
      // 读不到就说读不到 —— **不显示成「一个都没有」**（那是对用户撒谎）。
      this.body.textContent = "";
      const err = document.createElement("div");
      err.className = "settings-hint plugins-error";
      err.textContent = `读不到 marketplace 登记表：${e instanceof Error ? e.message : String(e)}（这不等于「没有」）`;
      this.body.appendChild(err);
      return;
    }
    if (mine !== this.seq) return;
    this.render(survey);
  }

  private render(survey: MarketplaceSurvey): void {
    this.body.textContent = "";
    if (survey.file_absent) {
      const none = document.createElement("div");
      none.className = "settings-hint plugins-empty";
      none.textContent =
        "这台机器没有登记任何 marketplace（`~/.claude/plugins/known_marketplaces.json` 不存在）。";
      this.body.appendChild(none);
      return;
    }
    if (survey.entries.length === 0) {
      const none = document.createElement("div");
      none.className = "settings-hint plugins-empty";
      none.textContent = "登记表在，但里面一个 marketplace 都没有。";
      this.body.appendChild(none);
      return;
    }
    for (const e of survey.entries) {
      const row = document.createElement("div");
      row.className = "plugins-row";

      const name = document.createElement("div");
      name.className = "plugins-row-id";
      name.textContent = e.id;
      row.appendChild(name);

      const count = document.createElement("div");
      // 读不到时挂一个不一样的类名：它在界面上**不该长得像一个数字**。
      count.className =
        e.declared_plugins === null ? "plugins-row-count plugins-row-count-unknown" : "plugins-row-count";
      count.textContent = declaredPluginsText(e);
      row.appendChild(count);

      const meta = document.createElement("div");
      meta.className = "settings-hint plugins-row-meta";
      meta.textContent = [
        e.source ? `来源：${e.source}` : "来源：未记",
        e.install_location ? `落点：${e.install_location}` : "落点：未记",
        lastUpdatedText(e),
      ].join(" · ");
      row.appendChild(meta);

      this.body.appendChild(row);
    }
  }
}
