/** 「这台上的 cc-monitor」那一折里的一行（机器页卡头里那几项共用一个样子）。 */
/**
 * 「这台上的 cc-monitor」里的一行：标签与下一行说明在左、控件在右。
 * 说明可以是一格会变的元素（跟着名字 / 读数换）。
 */
export function ccRow(label: string, help: string | HTMLElement, controls: HTMLElement[]): HTMLElement {
  const row = document.createElement("div");
  row.className = "machine-cc-row";
  const text = document.createElement("div");
  text.className = "machine-cc-text";
  const l = document.createElement("div");
  l.className = "machine-cc-label";
  l.textContent = label;
  const h = typeof help === "string" ? document.createElement("div") : help;
  h.classList.add("machine-cc-help");
  if (typeof help === "string") h.textContent = help;
  text.append(l, h);
  const ctl = document.createElement("div");
  ctl.className = "machine-cc-controls";
  ctl.append(...controls);
  row.append(text, ctl);
  return row;
}

