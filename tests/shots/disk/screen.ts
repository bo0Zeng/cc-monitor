/**
 * 替身窗格里印的那一屏（原样的终端输出：SGR 颜色 ＋ 光标定位）。真后端抓屏 · 实时画面读的就是它，颜色段由后端解出来。
 * 窗格的尺寸见 `real/pool.mjs`（[`PANE_COLS`] × [`PANE_ROWS`]）。
 */

/** 一段字带一种样子：`fg` 是 SGR 的前景色（`"32"` 绿 · `"90"` 暗 · `"34"` 蓝 · `"35"` 品红 · `"38;2;r;g;b"` 真彩）；`bold` 粗。 */
type Run = string | { t: string; fg?: string; bold?: boolean };

const ESC = "\x1b[";

function line(runs: Run[]): string {
  return runs
    .map((r) => {
      if (typeof r === "string") return r;
      const sgr = [r.bold ? "1" : "", r.fg ?? ""].filter(Boolean).join(";");
      return `${ESC}${sgr}m${r.t}${ESC}0m`;
    })
    .join("");
}

/** 一屏：从左上角起逐行印，最后光标停在 `cursor`（行 · 列，1 起）。 */
function screen(rows: Run[][], cursor: [number, number]): string {
  return `${ESC}H${ESC}2J${rows.map(line).join("\r\n")}${ESC}${cursor[0]};${cursor[1]}H`;
}

const dim = (t: string): Run => ({ t, fg: "90" });
const ok = (t: string): Run => ({ t, fg: "32" });
const tool = (name: string, arg: string): Run[] => [ok("●"), " ", { t: name, bold: true }, arg];

/** Claude Code 跑着的样子：前面几条命令的输出、正在构建、输入框、底下的权限档。 */
export const CLAUDE_SCREEN = screen(
  [
    [{ t: ">", bold: true }, " 订单服务调用库存接口时偶尔超时，帮我加上重试（指数退避）和整体超时，顺便补测试。"],
    [],
    [ok("●"), " 调用点只有两处，都经过 InventoryClient：在它里面统一做重试 ＋ 退避 ＋ 整体超时。"],
    [],
    [ok("●"), " 先跑一遍现有的测试，确认基线是绿的。"],
    [],
    tool("Bash", "(pytest -q tests/orders)"),
    [dim("  ⎿  "), dim("collected 57 items")],
    [dim("     57 passed in 3.12s")],
    [],
    tool("Read", "(src/orders/clients.py)"),
    [dim("  ⎿  "), dim("Read 42 lines")],
    [],
    tool("Edit", "(src/orders/clients.py)"),
    [dim("  ⎿  "), dim("Updated "), { t: "src/orders/clients.py", fg: "34" }, dim(" with 18 additions and 3 removals")],
    [],
    tool("Bash", "(pytest -q)"),
    [dim("  ⎿  213 passed in 9.41s")],
    [],
    tool("Bash", "(npm run build)"),
    [dim("  ⎿  Running…")],
    [dim("     vite v6.2.0 building for production...")],
    [dim("     transforming (812) "), { t: "src/orders/client.ts", fg: "34" }],
    ["     ", ok("✓"), " 812 modules transformed."],
    [],
    [{ t: "✶ Building…", fg: "38;2;217;119;87" }, " ", dim("(14s · ↑ 1.2k tokens · esc to interrupt)")],
    [],
    [dim("╭──────────────────────────────────────────────╮")],
    [dim("│"), " >                                            ", dim("│")],
    [dim("╰──────────────────────────────────────────────╯")],
    ["  ", { t: "⏵⏵ accept edits on", fg: "35" }, " ", dim("(shift+tab to cycle)")],
  ],
  [29, 5],
);

/** 起会话那一行在 shell 里报错（启动器认不出参数）；下一行的提示符是窗格里落回的那个 shell 自己印的。 */
export const LAUNCH_FAILED_SCREEN = screen([["$ ccm -- claude --modle opus"], ["claude: error: unknown option '--modle'"], ["(Did you mean --model?)"]], [4, 1]);
