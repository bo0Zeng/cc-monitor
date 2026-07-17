/**
 * F88b（#52）：用量 HUD chip——挂 status-bar，显**活跃会话 context 占用%**（近似：最新一轮
 * assistant 记录的 input+cache token ÷ 模型上限）。逼近上限（≥80%）高亮预警——最可行动的实时信号。
 *
 * 纯前端、零后端：数据来自 live 流（TabManager onLine 捕获活跃会话最新 assistant 的 usage+model）。
 * **只 token 不 $**（用户 2026-07-17 拍板）。模型上限表在 `views/pricing.ts`（未知模型显 `?`，不显错%）。
 *
 * 「今日 token」= 后续项（需跨会话聚合，非纯前端；本刀先聚焦 context% 这个最高价值信号）。
 */

import { contextPercent, normalizeModel } from "./views/pricing";

export class UsageHud {
  /** 挂 status-bar 的 chip。 */
  readonly summaryElement: HTMLButtonElement;
  private model: string | null = null;
  private promptTokens: number | null = null;

  constructor() {
    const btn = document.createElement("button");
    btn.type = "button";
    // 复用 status-bar chip 基类（同 agents-panel 的 "status-tasks status-agents" 范式），
    // usage-hud-chip 只叠 delta（tabular-nums + .is-high 预警）。
    btn.className = "status-tasks usage-hud-chip";
    btn.style.display = "none"; // 无活跃会话/无 usage 时隐藏
    this.summaryElement = btn;
  }

  /** 点击 chip 的行为（main.ts 注入：打开用量视图）。 */
  onClick(handler: () => void): void {
    this.summaryElement.addEventListener("click", handler);
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

  private render(): void {
    const btn = this.summaryElement;
    if (this.promptTokens == null) {
      btn.style.display = "none";
      btn.classList.remove("is-high"); // 隐藏时清干净状态，防下次 show 前残留
      return;
    }
    btn.style.display = "";
    const pct = contextPercent(this.model, this.promptTokens);
    const tok = this.promptTokens.toLocaleString("en-US");
    if (pct == null) {
      btn.textContent = "ctx ?";
      btn.title = `活跃会话 context 占用：模型「${normalizeModel(this.model)}」上限未知（${tok} tokens）`;
      btn.classList.remove("is-high");
      return;
    }
    const rounded = Math.round(pct);
    btn.textContent = `ctx ${rounded}%`;
    btn.title = `活跃会话 context 占用 ≈ ${rounded}%（${normalizeModel(this.model)}，最新一轮 ${tok} tokens ÷ 模型上限）。近似值。`;
    btn.classList.toggle("is-high", rounded >= 80); // 逼近自动 compact 时预警
  }
}
