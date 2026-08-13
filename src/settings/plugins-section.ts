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
// | `invoke` 抛错 | 「**读不到**……（这不等于「没有」）」 |
// | `declared_plugins: null` | 「**读不到**插件数：<理由>」，**不是「0 个」** |
//
// 中间那条的措辞抄的是 `drift-ledger-section` 那句「读不到就说读不到 ——
// **不显示成「没有漂移」**（那是对用户撒谎）」，同一族的病本轮已收过三次。
//
// ## 只读、按需
//
// 一次 `invoke`，**不轮询**（红线，同 `config-surface-section.ts` / `drift-ledger-section.ts`）。
import { commands } from "../ipc/commands";
import type { MarketplaceEntry } from "../generated/MarketplaceEntry";
import type { MarketplaceSurvey } from "../generated/MarketplaceSurvey";

export type { MarketplaceEntry, MarketplaceSurvey };

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

  constructor() {
    this.element = this.build();
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
      "自己**声明**的插件数。它回答的是「有哪些插件可以装」，" +
      "**不是**「装了哪些 / 启用了哪些」—— 后者今天在盘上没有真相源，" +
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
      survey = await commands.list_plugin_marketplaces();
    } catch (e) {
      if (mine !== this.seq) return; // 迟到的失败也不许盖掉新结果
      // 读不到就说读不到 —— **不显示成「一个都没有」**（那是对用户撒谎）。
      this.body.textContent = "";
      const err = document.createElement("div");
      err.className = "settings-hint plugins-error";
      err.textContent = `读不到 marketplace 登记表：${String(e)}（这不等于「没有」）`;
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
