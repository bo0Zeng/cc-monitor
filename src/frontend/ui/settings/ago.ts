/** 采样多久前：`3m` · `2h` · `1d`（账号页与「文件与数据」离线那台的警告条同一种说法）。 */
import { copyText } from "../copy-table";

export function agoText(ms: number): string {
  const m = Math.max(0, Math.round(ms / 60_000));
  if (m < 60) return copyText("acctPage.ago.minutes", { n: m });
  const h = Math.round(m / 60);
  if (h < 48) return copyText("acctPage.ago.hours", { n: h });
  return copyText("acctPage.ago.days", { n: Math.round(h / 24) });
}
