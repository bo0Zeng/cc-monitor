/**
 * F01 真机验收的**输入源**：从生产渲染链取命令串（不手搓等价命令）。
 *
 * 走 `launch-render-driver.ts`：生产 `plan*` → 生产 `buildCliRenderRequest` → 生产 Rust 命令 ——
 * 本套件验的就是生产那一行（`ccm …`；就地 resume 那一格外层包一层 tmux）。
 *
 * 为什么必须从生产链取：上一轮修复三门禁全绿却让 `send-keys` 完全失效——因为黄金串只断言
 * 「我写出了打算写的字符串」，从不断言「这条命令在 tmux 上的效果」。手搓等价命令会重蹈覆辙。
 *
 * launcher 用 `CCMPROBE`（纯字母词，过 ccm 的 launcher 那道闸）而非 `claude`：真 claude 会启动并
 * 重绘/清屏，把打进去的载荷盖掉 → 「兄弟会话未被污染」的 grep 会给出**假 PASS**（本坑真实踩过）。
 * `CCMPROBE` 报 command not found 后留在屏幕上，可靠可 grep。
 *
 * 输出：每行 `<key>\t<生产命令串>`，供 `tmux-target-acceptance.sh` 消费。
 */
import {
  planResumeTmux,
  planResumeIntoExistingTmux,
  planAttach,
  planLauncher,
} from "../../src/frontend/ui/launch-requests.ts";
import { renderCmdViaProduction } from "./launch-render-driver.ts";

const out: Record<string, string> = {
  // 新建自己的 tmux（兄弟名 cc-p1-2 已存在时，绝不能碰它）
  resumeTmux: renderCmdViaProduction(planResumeTmux("p1", "", "CCMPROBE", "cc-p1")),
  // 往「已存在的 cc-p1」就地 send-keys —— cc-p1 不存在时必须失败，绝不能落进 cc-p1-2
  resumeIntoExisting: renderCmdViaProduction(planResumeIntoExistingTmux("p1", "cc-p1", "CCMPROBE")),
  attach: renderCmdViaProduction(planAttach("cc-p1")),
  // 起新会话（posixQuote 名路径，与上面的裸名路径是两条不同的引号分支）
  launcher: renderCmdViaProduction(planLauncher("", "cc-p1", "CCMPROBE")),
};
for (const [k, v] of Object.entries(out)) console.log(`${k}\t${v}`);
