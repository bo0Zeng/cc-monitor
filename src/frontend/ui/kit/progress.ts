/**
 * 进度（C14）：确定进度的细条 ＋ 不确定进度的 14px 转圈。
 *
 * 转圈不到 300ms 不画、画了至少留 400ms（防闪）由 [`delayedSpinner`] 管。
 */
import s from "./progress.module.css";

/** 14px 转圈（纯装饰，读屏器不念；进行中的字由所在控件给）。 */
export function spinner(): HTMLSpanElement {
  const sp = document.createElement("span");
  sp.className = s.progressSpinner;
  sp.setAttribute("aria-hidden", "true");
  return sp;
}

/** 确定进度：4px 条 ＋ 旁边 `3.2 / 10 MB` 那样的读数（tabular 数字）。`ratio` 取 0–1。 */
export function progressBar(ratio: number, readout: string): { root: HTMLDivElement; set(ratio: number, readout: string): void } {
  const root = document.createElement("div");
  root.className = s.progressRow;
  root.setAttribute("role", "progressbar");
  root.setAttribute("aria-valuemin", "0");
  root.setAttribute("aria-valuemax", "100");
  const track = document.createElement("div");
  track.className = s.progressTrack;
  const fill = document.createElement("div");
  fill.className = s.progressFill;
  track.appendChild(fill);
  const text = document.createElement("span");
  text.className = s.progressReadout;
  root.append(track, text);
  const set = (r: number, t: string): void => {
    const v = Math.max(0, Math.min(1, r));
    fill.style.transform = `scaleX(${v})`;
    root.setAttribute("aria-valuenow", String(Math.round(v * 100)));
    text.textContent = t;
  };
  set(ratio, readout);
  return { root, set };
}

