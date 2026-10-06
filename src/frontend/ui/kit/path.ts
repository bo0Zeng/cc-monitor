/**
 * 路径的**显示形**：家目录那一截写成 `~`（`/home/u/.cc-monitor/accounts/a` ⇒ `~/.cc-monitor/accounts/a`）。
 * 路径与家目录都是那台后端给的原样；这里只做这一步缩写，不解析、不规范化。家目录不知道 / 路径不在它下面 ⇒ 原样。
 */
export function homeShort(path: string, home: string | null | undefined): string {
  if (!home) return path;
  const h = home.replace(/[\\/]+$/, "");
  if (h === "") return path;
  if (path === h) return "~";
  const next = path.charAt(h.length);
  if (path.startsWith(h) && (next === "/" || next === "\\")) return `~${path.slice(h.length)}`;
  return path;
}
