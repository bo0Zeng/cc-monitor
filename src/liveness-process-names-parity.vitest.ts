/**
 * audit-0805 F13 下半：**`livenessProcessNames` 的跨语言 token 对拍**。
 *
 * # 报告说「2 份、无对拍」，核实之后是「比那更麻烦，但麻烦的地方不在值上」
 *
 * 两侧的 token 今天**相同**（`claude` / `node`），但**判法三处不同**，而且是**刻意的**：
 *
 * | | 前端 `tmux-sessions.ts::isClaudeTmuxCommand` | daemon `watcher.rs::add_time_verdict` |
 * |---|---|---|
 * | 问的问题 | **正向识别**：这个 pane 在跑 claude 吗 | **否证式**：这个 pid 明显**不是** claude 吗 |
 * | 匹配 | `Set.has(cmd)` 精确 | `to_lowercase().contains()` 子串 |
 * | 输入 | tmux 的 `pane_current_command` | `/proc/<pid>/cmdline` |
 * | 缺数据 | 不命中 ⇒ **不算** claude 会话（据此过滤 attach/kill/preview） | 空/读不到 ⇒ **`Alive`**（放行） |
 *
 * 缺数据时默认相反**不是 bug**：前端在做「该不该给用户这个按钮」（宁可不给），
 * daemon 在做「要不要把这个会话当冒名者扔掉」（宁可放行，别误杀活会话）。
 * ⇒ **本条不要求两边统一判法**，那样会把其中一边改错。
 *
 * # 那本条钉什么
 *
 * 钉**共享的那一点**：两侧认的 token 集合。今天 daemon 那两个是**内联字面量**
 * （`watcher.rs` 里写死的 `"claude"` / `"node"`），压根没引用任何共享表 ⇒
 * **一侧加一个 token，另一侧不会有任何信号**。
 *
 * ⚠ **今天没有权威方**：`agent-profile-golden.tsv`（「agent 适配表的唯一真相源」）
 * 只有 4 个 key（`default_launcher` / `resume_kind` / `resume_token` / `nested_env`），
 * **不含这一项**，`AgentAdapter` trait 也没有「判活进程名」这个方法。
 * 收成一份归 `daemon-api` **F11**；本区（照 LEDGER 那行）只把**对拍**钉上，
 * 并在诊断里**点名权威方今天缺位**——不点名的话，下一个看到红灯的人不知道该改哪边。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { AGENT_PROFILE } from "./agent-profile";

const REPO = resolve(__dirname, "..");

/** 剥掉整行注释 —— 两侧的头注里都写着这些 token。 */
function stripLineComments(src: string): string {
  return src
    .split("\n")
    .filter((l) => {
      const t = l.trimStart();
      return !(t.startsWith("//") || t.startsWith("*") || t.startsWith("/*") || t.startsWith("#"));
    })
    .join("\n");
}

/** daemon 侧判活词表今天的住址（`S3` 08-14 从 `watcher.rs` 搬来）。 */
const DAEMON_LIVENESS = "remote-daemon-proto/src/agents/claudecode/liveness.rs";

/**
 * 从 daemon 的判活词表里抠出 token。
 *
 * ⚠〔`S3` 08-14〕原来抠的是 `watcher.rs::add_time_verdict` 里那段**内联**的
 * `lower.contains("claude") && lower.contains("node")`。`S3` 把它搬进了
 * `agents/claudecode/liveness.rs::cmdline_may_be_agent` —— 本条**当场红**
 *（抽取器自检那格先红，正是它存在的理由：搬走 ⇒ 零命中，而零命中会变成绿）。
 *
 * 搬迁的**副作用是好的**：原来还要小心避开同函数前面那个 `bg-spare` 拦截
 *（守护池备用进程，不是判活白名单）；现在词表自成一个函数，抠的范围天然是准的。
 * bg-spare 那条自检仍留着 —— 它现在钉的是「别把 watcher 的别的逻辑也搬进词表」。
 */
function daemonTokens(): string[] {
  const src = stripLineComments(readFileSync(resolve(REPO, DAEMON_LIVENESS), "utf8"));
  const anchor = "pub(crate) fn cmdline_may_be_agent";
  const at = src.indexOf(anchor);
  expect(
    at,
    `在 ${DAEMON_LIVENESS} 里找不到 \`${anchor}\` —— 判活词表被改写或又搬走了，本条会零命中地绿`,
  ).toBeGreaterThan(-1);
  const tail = src.slice(at, at + 400);
  const stop = tail.indexOf("\n}");
  const region = stop > 0 ? tail.slice(0, stop) : tail;
  return [...region.matchAll(/lower\.contains\("([^"]+)"\)/g)].map((m) => m[1]).sort();
}

describe("livenessProcessNames 跨语言 token 对拍（audit-0805 F13，E3）", () => {
  it("★ 抽取器自检：两侧都真的抠出了 token（否则下面是零命中地绿）", () => {
    const ts = [...AGENT_PROFILE.livenessProcessNames].sort();
    const rs = daemonTokens();
    expect(ts.length, "前端 livenessProcessNames 抠出来是空的").toBeGreaterThan(0);
    expect(
      rs.length,
      "daemon 那条白名单分支抠出来是空的 —— 抽取器坏了，下面的对拍会两边都空而变绿",
    ).toBeGreaterThan(0);
    // bg-spare 那条是**另一件事**（守护池备用进程拦截），混进来就说明抠的范围太宽。
    expect(rs, "抠到了 bg-spare —— 抠的范围越界了，那不是判活白名单").not.toContain("bg-spare");
  });

  it("★ 两侧认的进程名必须一致", () => {
    const ts = [...AGENT_PROFILE.livenessProcessNames].sort();
    const rs = daemonTokens();
    expect(
      rs,
      "★ 前端与 daemon 认的判活进程名漂开了。\n" +
        `  前端 src/agent-profile.ts::livenessProcessNames = ${JSON.stringify(ts)}\n` +
        `  daemon ${DAEMON_LIVENESS}::cmdline_may_be_agent = ${JSON.stringify(rs)}\n` +
        "⚠ **今天没有权威方** —— `agent-profile-golden.tsv` 只有 4 个 key，不含这一项；\n" +
        "  `AgentAdapter` trait 也没有「判活进程名」这个方法。收成一份归 `daemon-api` F11。\n" +
        "  在那之前：**两边都要改**，并回来把这条对拍的期望一起改。\n" +
        "⚠ 别顺手把两边的**判法**也统一了：前端是正向识别（缺数据 ⇒ 不算 claude），\n" +
        "  daemon 是否证式（缺数据 ⇒ 放行）。方向相反是刻意的，统一会把其中一边改错。",
    ).toEqual(ts);
  });

  it("★ daemon 那侧仍是「否证式 + 缺数据放行」—— 这条方向不许被顺手改掉", () => {
    const src = stripLineComments(readFileSync(resolve(REPO, DAEMON_LIVENESS), "utf8"));
    const at = src.indexOf("pub(crate) fn cmdline_may_be_agent");
    const region = src.slice(at, at + 400);
    expect(
      region.includes("lower.trim().is_empty()"),
      "daemon 的 cmdline 分支不再先判「cmdline 非空」—— 那意味着**读不到 cmdline 就当冒名者扔掉**，" +
        "会误杀活会话。缺数据放行是这一侧刻意的方向（前端相反）。",
    ).toBe(true);
    const caller = stripLineComments(
      readFileSync(resolve(REPO, "remote-daemon-proto/src/observe/watcher.rs"), "utf8"),
    );
    const callAt = caller.indexOf("cmdline_may_be_agent(&lower)");
    expect(
      callAt,
      "watcher 不再调 `cmdline_may_be_agent` —— 判活词表的消费点搬走了而本条没跟",
    ).toBeGreaterThan(-1);
    expect(
      caller.slice(callAt, callAt + 200).includes("Imposter"),
      "daemon 的 cmdline 分支不再产出 Imposter —— 它是**否证式**的：命中白名单不代表活着，" +
        "只有明显不像才判死。改成正向识别会与前端撞成同一套语义，而两边问的不是同一个问题。",
    ).toBe(true);
  });

  it("前端那侧仍是精确匹配（不是子串）—— 与 daemon 的子串刻意不同", () => {
    const src = stripLineComments(readFileSync(resolve(REPO, "src/tmux-sessions.ts"), "utf8"));
    expect(
      src.includes("livenessProcessNames.has("),
      "前端不再用 `Set.has` 精确匹配 —— 改成子串会让 `claude-helper` 之类的进程被认成 claude 会话，" +
        "而这一侧的输入是 tmux 的 `pane_current_command`（本来就是干净的命令名）。",
    ).toBe(true);
  });
});
