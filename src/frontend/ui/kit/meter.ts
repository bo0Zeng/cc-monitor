/**
 * 计量条（C22）：一个量用了多少（与进度条分开）。
 *
 * 态由后端判，界面只按态上色：常态中性 · 到阈值 · 被拒 · 数旧（斜纹）· 无采样（虚线轨）。
 */
import s from "./meter.module.css";

export type MeterState = "normal" | "near" | "refused" | "stale" | "none";

export interface MeterSpec {
  /** 窗口名（`5h` · `7d`）。 */
  label: string;
  /** 0–1；无采样时不用。 */
  ratio: number;
  state: MeterState;
  /** 右侧读数（`63%`，取整由调用方给）。 */
  value: string;
  /** 再右（`↻18:30`）。 */
  reset?: string;
}

export function meter(spec: MeterSpec): HTMLDivElement {
  const root = document.createElement("div");
  root.className = s.meter;
  root.dataset.state = spec.state;
  const label = document.createElement("span");
  label.className = s.meterLabel;
  label.textContent = spec.label;
  const track = document.createElement("span");
  track.className = s.meterTrack;
  const fill = document.createElement("span");
  fill.className = s.meterFill;
  if (spec.state !== "none") fill.style.transform = `scaleX(${Math.max(0, Math.min(1, spec.ratio))})`;
  track.appendChild(fill);
  const value = document.createElement("span");
  value.className = s.meterValue;
  value.textContent = spec.value;
  root.append(label, track, value);
  if (spec.reset) {
    const r = document.createElement("span");
    r.className = s.meterReset;
    r.textContent = spec.reset;
    root.appendChild(r);
  }
  return root;
}
