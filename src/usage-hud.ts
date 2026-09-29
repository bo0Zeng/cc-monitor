/**
 * F88b（#52）：用量 HUD chip——挂 status-bar，显**活跃会话 context 占用%**（近似：最新一轮
 * assistant 记录的 input+cache token ÷ 模型上限）。逼近上限（≥80%）高亮预警——最可行动的实时信号。
 *
 * 〔STC · `设计/90 §4` 阶段 C〕数据来自后端的会话事实（`history-facts` 的 `usage`：文件序最后一条带有效 usage 的 assistant 记录），
 * 经 `TabManager.onActiveUsageChanged` 喂进来；上限表与百分比仍在前端（`views/context-limit.ts`，那是排版）。
 * 从前它由 `onLine` 旁路一条一条攒（`设计/50 §2` ① 那格写的「纯前端、零后端」指的是那时候），F5 之后只看得见重放那一截。
 * 事实要不到 ⇒ `setUnavailable(原因)`：chip 显示 `ctx —`、提示里说原因（`设计/05 §14.3`「不可用，不是空表」）。
 * **只 token 不 $**（用户 2026-07-17 拍板）。模型上限表在 `views/context-limit.ts`（未知模型显 `?`，不显错%）。
 *
 * 「今日 token」= 后续项（需跨会话聚合，非纯前端；本刀先聚焦 context% 这个最高价值信号）。
 */

import { contextPercent, normalizeModel, readContextLimits, type ContextLimitOverrides } from "./views/context-limit";
import { loadConfig } from "./config";
import s from "./usage-hud.module.css";
import { copyText } from "./copy-table";

export class UsageHud {
  /** 挂 status-bar 的 chip。 */
  readonly summaryElement: HTMLButtonElement;
  private model: string | null = null;
  private promptTokens: number | null = null;
  /** 〔STC〕活跃会话的会话事实要不到的原因（`null` = 可用）。非空时压过上面两格（那两格可能是旧的）。 */
  private unavailable: string | null = null;
  /** 模型上限用户覆盖表（config.json `contextLimits`）——纠正标准 1M 模型串被当 200k 的 ctx% 误报。 */
  private limitOverrides: ContextLimitOverrides = {};

  constructor() {
    void this.loadLimitOverrides();
    const btn = document.createElement("button");
    btn.type = "button";
    // 复用 status-bar chip 基类（同 agents-panel 的 "status-tasks status-agents" 范式），
    // 本组件自己的 delta（tabular-nums + 高位预警）住 `usage-hud.module.css`（CSS Modules，类名哈希）。
    btn.className = `status-tasks ${s.chip}`;
    btn.style.display = "none"; // 无活跃会话/无 usage 时隐藏
    // `设计/50`：它当初点开的跨会话聚合视图已退役 ⇒ chip 变**纯只读**，不再挂任何监听。
    // 元素仍是 <button>（`.status-tasks` 那套 chip 样式建在 button 上），只把光标改回默认，
    // 免得它看起来还像能点。
    btn.style.cursor = "default";
    this.summaryElement = btn;
  }

  /** 读 config.json `contextLimits`（模型子串→上限 tokens）覆盖表，纠正 1M 模型 ctx% 误报。失败静默用默认表。 */
  /** 〔FIX4〕设置页改了覆盖表（`SETTINGS_APPLIED_EVENT`）⇒ 主窗口重读一次。 */
  async loadLimitOverrides(): Promise<void> {
    try {
      const cfg = (await loadConfig()) as Record<string, unknown>;
      this.limitOverrides = readContextLimits(cfg["contextLimits"]);
      this.render(); // 覆盖表迟到 / 改过 → 重算已显的 chip
    } catch {
      /* 用内置默认上限表 */
    }
  }

  /**
   * 更新活跃会话的最新 prompt token + model（→ 重算 context%）。
   * `promptTokens` = null（无活跃会话 / 活跃会话尚无带 usage 的 assistant 记录）→ 隐藏 chip。
   */
  setActive(model: string | null, promptTokens: number | null): void {
    this.model = model;
    this.promptTokens = promptTokens;
    this.render();
  }

  /** 〔STC〕活跃会话的会话事实要不到 ⇒ 说出来（`null` = 又可用了 / 切到了可用的会话）。 */
  setUnavailable(reason: string | null): void {
    this.unavailable = reason;
    this.render();
  }

  private render(): void {
    const btn = this.summaryElement;
    if (this.unavailable !== null) {
      btn.style.display = "";
      btn.textContent = "ctx —";
      btn.title = copyText("usageHud.render.factsUnavailable", { reason: this.unavailable });
      btn.classList.remove(s.high);
      return;
    }
    if (this.promptTokens == null) {
      btn.style.display = "none";
      btn.classList.remove(s.high); // 隐藏时清干净状态，防下次 show 前残留
      return;
    }
    btn.style.display = "";
    const pct = contextPercent(this.model, this.promptTokens, this.limitOverrides);
    const tok = this.promptTokens.toLocaleString("en-US");
    if (pct == null) {
      btn.textContent = "ctx ?";
      btn.title = copyText("usageHud.render.unknownLimit", { model: normalizeModel(this.model), tok });
      btn.classList.remove(s.high);
      return;
    }
    const rounded = Math.round(pct);
    btn.textContent = `ctx ${rounded}%`;
    btn.title = copyText("usageHud.render.approx", { rounded, model: normalizeModel(this.model), tok });
    btn.classList.toggle(s.high, rounded >= 80); // 逼近自动 compact 时预警
  }
}
