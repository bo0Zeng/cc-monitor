/**
 * `P9`：把 `doc/INVARIANTS.md §31`「最终形态第①条」从**散文里的一条手工 grep**变成机检。
 *
 * # 那条规矩逐字
 *
 * > 1. **前端绝不硬编码后端命令**（不准出现可执行的字面 `tmux attach` / `tmux new-session` /
 * >    `tmux send-keys`）→ **问一层要**（阶段②问 daemon，阶段①问前端座 `src/session-backend.ts`）。
 *
 * 它守的是用户 2026-07-15 那句原话：「**我不能接受这边产生的会话那边看不到。**」
 * 会话后端是**机器的属性**，不是界面的属性 —— 桌面用 abduco 起、手机只会 tmux ⇒ 接不上，
 * 会话池当场劈成两半。
 *
 * # 为什么要机检：那条门禁**已经失效过一次，且失效得毫无声息**
 *
 * `§31` 里写的门禁是一条手工命令：
 * `grep -nE "tmux (new-session|send-keys|attach)" src/remote-launch.ts`。
 * 它**只盯一个文件**。而 `remote-launch-run.ts` 是后来从那个文件拆出去的 ——
 * 门禁没跟着拆，于是 `remote-launch-run.ts` 里躺着一句手写的
 * `tmux attach -t '=<name>:'`（`P9` 摸底 08-12 逮到，已改成问座要）。
 *
 * ⇒ **散文门禁会跟着文件拆分一起腐**，而这条腐坏没有任何东西会报。本文件补的就是这个。
 *
 * # 人群与豁免（每一条都得说清为什么，别当成许可清单）
 *
 * | 排除 | 为什么不算「前端硬编码后端命令」 |
 * |---|---|
 * | `session-backend.ts` | **它就是那一层** —— 命令语法的家在这里，这正是第①条要的 |
 * | `*.test.ts` / `*.vitest.ts` | 判据里的**期望串**：它们钉的正是座出的命令长什么样 |
 * | 注释行（`//` `*` 开头） | 解释「这条路径长什么样」是**文档**，删了反而更糟 |
 * | `src-tauri/` 与 daemon | **不是前端**。第①条管的是前端；Rust 侧另有 `tmux_daemon_gate_guard` 那 23 条 |
 *
 * ⚠ **豁免注释行有代价，如实登记**：把一句可执行的命令写进注释就绕过了本条。
 * 那是本仓所有「剥注释」类守卫共有的失效模式，且反过来（不剥）会更糟 ——
 * 会把解释性文档一律禁掉，逼人删掉理由。⇒ 靠人读 diff 兜，不假装钉住了。
 *
 * ⚠ **本条不读自己**：扫的是 `src/**.ts` 里**排掉** `.vitest.ts` 的那些，
 * 而本文件正是 `.vitest.ts`（同 `scanning-guard-registry` 立的那条判准：
 * 「遍历者靠什么读不到自己？扫的树不含自己」）。
 */
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";

const SRC = resolve(__dirname);

/** 可执行的后端动词。**只列会改变会话状态或接管终端的那些**，不是所有 tmux 子命令。 */
const BACKEND_VERBS = [
  "new-session",
  "send-keys",
  "attach",
  "kill-session",
  "rename-session",
] as const;

function tsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = resolve(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "generated") continue; // 生成物，不是人写的
      out.push(...tsFiles(p));
      continue;
    }
    if (!name.endsWith(".ts")) continue;
    if (name.endsWith(".test.ts") || name.endsWith(".vitest.ts")) continue;
    if (name === "session-backend.ts") continue;
    out.push(p);
  }
  return out;
}

/**
 * 一行里有没有**可执行的**后端命令字面量（注释行不算，理由见头注）。
 *
 * ★★ 「动词后面**紧跟一个旗标**」是**首跑逼出来的收窄**：第一版只认 `tmux <动词>`，
 * 当场误报在一句 **toast 文案**上 —— `success: "已拉起 tmux attach"`。
 * 那不是命令，是给用户看的话。**一条会对文案报警的守卫活不过三天**：
 * 下一个被它拦住的人会去把它删掉或加白名单，而那时它连真违反也不拦了。
 *
 * ⚠ 代价如实登记：**不带旗标的调用会漏**（如裸 `tmux attach`）。
 * 本仓五个动词的真实用法**全部**紧跟旗标（`attach -t` / `new-session -d` /
 * `send-keys -t` / `kill-session -t` / `rename-session -t`），所以今天这个代价是空的；
 * 哪天真出现裸调用，这条会**假绿** —— 别把它读成「一条都没漏」。
 */
function offendingVerbs(line: string): string[] {
  const t = line.trim();
  if (t.startsWith("//") || t.startsWith("*") || t.startsWith("/*")) return [];
  return BACKEND_VERBS.filter((v) => new RegExp(`tmux\\s+${v}\\s+-`).test(line));
}

describe("§31 最终形态第①条：前端不许硬编码会话后端命令", () => {
  const files = tsFiles(SRC);

  it("抽取器自检：真的扫到了东西（否则下面那条是空转的）", () => {
    // 这一条挡的是「人群被过滤空了 ⇒ 判据恒绿」——
    // 本仓 `node-suite-registry-guard` 记的正是这一族：「把测试删光 ⇒ 静默绿」。
    expect(files.length).toBeGreaterThan(30);
    const total = files.reduce((n, f) => n + readFileSync(f, "utf8").split("\n").length, 0);
    expect(total).toBeGreaterThan(5000);
  });

  it("★★ 前端生产段里一条可执行的 tmux 命令都不许有", () => {
    const hits: string[] = [];
    for (const f of files) {
      const lines = readFileSync(f, "utf8").split("\n");
      lines.forEach((line, i) => {
        for (const v of offendingVerbs(line)) {
          hits.push(`${f.slice(SRC.length + 1)}:${i + 1}  tmux ${v}`);
        }
      });
    }
    expect(
      hits,
      `前端硬编码了会话后端命令 —— §31 最终形态第①条逐字禁这件事，理由是` +
        `「会话后端是机器的属性，不是界面的属性」：桌面用 abduco 起、手机只会 tmux 就接不上。\n` +
        `改成问 \`SESSION_BACKEND\` 要（阶段①）或问 daemon 要（阶段②）。\n` +
        `命中：\n${hits.join("\n")}`,
    ).toEqual([]);
  });

  it("★ 座本身**必须**还留着那些命令 —— 否则上一条会因为「哪儿都没有」而假绿", () => {
    // 反向自检：把语法从座里也删光，上一条照样绿，而那时功能已经坏了。
    const seat = readFileSync(resolve(SRC, "session-backend.ts"), "utf8");
    for (const v of ["new-session", "send-keys", "attach"]) {
      expect(seat, `座里少了 tmux ${v} —— 命令语法的家就是这里`).toContain(`tmux ${v}`);
    }
  });

  it("★ 阶段② 的边界还写在座上（删了它下一个人会以为「再加一个 const」就行）", () => {
    const seat = readFileSync(resolve(SRC, "session-backend.ts"), "utf8");
    expect(seat).toContain("阶段②不是「再加一个返回 shell 串的 const」");
    // abduco 没有 send-keys 是整个阶段②的形状约束，不是一句顺带的话。
    expect(seat).toContain("send-keys");
    expect(seat).toContain("abduco");
  });
});
