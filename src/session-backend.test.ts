/**
 * session-backend.ts 纯函数断言：TMUX_BACKEND 命令语法精确输出 + SESSION_BACKEND 同一性。
 * 跑法：`node src/session-backend.test.ts` 或 `npm run test:session-backend`。
 */

import { readFileSync } from "node:fs";
import { TMUX_BACKEND, SESSION_BACKEND } from "./session-backend.ts";

let failed = 0;
function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed++;
    console.error(`  ✗ ${name}\n      ${e instanceof Error ? e.message : String(e)}`);
  }
}
function eq(a: unknown, b: unknown, msg?: string): void {
  if (a !== b) throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`);
}

console.log("session-backend.test.ts");

test("TMUX_BACKEND.createRunAttach：带 cwd → new-session -c + send-keys + attach", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: { kind: "raw", value: "cc-1234abcd" },
      quotedCwd: "'/home/u/proj'",
      quotedPayload: "'unset X; claude --resume abc'",
    }),
    "tmux new-session -d -s cc-1234abcd -c '/home/u/proj' && " +
      // C14：`create` 不再无条件 attach —— 建失败就不接（原来这里是 `Enter; `）。
      "tmux send-keys -t =cc-1234abcd: 'unset X; claude --resume abc' Enter && " +
      "tmux attach -t =cc-1234abcd:",
  );
});

test("TMUX_BACKEND.createRunAttach：quotedCwd=null → 省 -c 标志", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: { kind: "raw", value: "cc-x" },
      quotedCwd: null,
      quotedPayload: "'p'",
    }),
    "tmux new-session -d -s cc-x && tmux send-keys -t =cc-x: 'p' Enter && tmux attach -t =cc-x:",
  );
});

test("TMUX_BACKEND.createRunAttach：target 可为 posixQuote 名（F53 含空格）", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: { kind: "quoted", value: "my sess" },
      quotedCwd: null,
      quotedPayload: "'p'",
    }),
    "tmux new-session -d -s 'my sess' && tmux send-keys -t '=my sess:' 'p' Enter && tmux attach -t '=my sess:'",
  );
});

test("TMUX_BACKEND.attach：attach -t <target>", () => {
  eq(TMUX_BACKEND.attach({ kind: "quoted", value: "my sess" }), "tmux attach -t '=my sess:'");
  eq(TMUX_BACKEND.attach({ kind: "raw", value: "cc-abc" }), "tmux attach -t =cc-abc:");
});

test("#72 + F03.4甲′ createRunAttach：ccmSid → create 分支插 @ccm_sid + set-titles(new-session 后、send-keys 前)", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: { kind: "raw", value: "cc-1234abcd" },
      quotedCwd: null,
      quotedPayload: "'p'",
      ccmSid: "1234abcd-full-sid",
    }),
    "tmux new-session -d -s cc-1234abcd && " +
      "(tmux set-option -t =cc-1234abcd: @ccm_sid 1234abcd-full-sid 2>/dev/null || true) && " +
      "(tmux set-option -t =cc-1234abcd: set-titles on 2>/dev/null || true) && " +
      "(tmux set-option -t =cc-1234abcd: set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && " +
      "tmux send-keys -t =cc-1234abcd: 'p' Enter && tmux attach -t =cc-1234abcd:",
  );
});

// F03.4 甲′：set-titles-string 从 @ccm_sid 派生（claude 覆盖不了）；**裸值不带双引号**（launch.rs 拒双引号）。
test("F03.4甲′ createRunAttach：set-titles-string 裸值、不含双引号（穿 launch.rs 的 bash -lic）", () => {
  const cmd = TMUX_BACKEND.createRunAttach({
    target: { kind: "raw", value: "cc-x" },
    quotedCwd: null,
    quotedPayload: "'p'",
    ccmSid: "s1",
  });
  eq(cmd.includes("set-titles-string ccm-rbind-#{@ccm_sid}"), true, "从 @ccm_sid 派生");
  eq(cmd.includes('"'), false, "整条命令不含双引号（launch.rs fail-closed）");
});

test("#72 + F03.4甲′ createRunAttach：无 ccmSid → 不插 set-option/set-titles(零回归)", () => {
  const cmd = TMUX_BACKEND.createRunAttach({ target: { kind: "raw", value: "cc-x" }, quotedCwd: null, quotedPayload: "'p'" });
  eq(cmd.includes("set-option"), false);
  eq(cmd.includes("set-titles"), false);
});

test("SESSION_BACKEND === TMUX_BACKEND（阶段①唯一活跃后端）", () => {
  eq(SESSION_BACKEND, TMUX_BACKEND);
});

// F01 漂移守卫（INVARIANTS §31a）：`=名:` 精确目标形态编码在三处——本座、`src-tauri/src/tmux.rs`
// 的 `exact_target()`、以及 `e2e/restart-shims/core.mjs`。shim 是 Tauri IPC 边界的 mock，
// **结构上无法 import Rust，去重不可能**，只能靠守卫钉住：它一旦退回裸目标，e2e 会对
// 「杀错会话 / 按键投错会话」这条整类 bug 假绿（生产已精确、探针仍前缀匹配 → 测不出差异）。
test("F01 漂移守卫：e2e shim 的 tmux 目标与本座同构（=名: 形态）", () => {
  const shim = readFileSync(new URL("../e2e/restart-shims/core.mjs", import.meta.url), "utf8");
  eq(shim.includes("`=${target}:`"), true, "shim 必须用 =名: 精确形态（见 INVARIANTS §31a）");
  eq(
    /\[\s*"send-keys",\s*"-t",\s*target\b/.test(shim),
    false,
    "shim 不得把裸 target 直接当 -t 目标",
  );
  eq(
    /\[\s*"kill-session",\s*"-t",\s*target\s*\]/.test(shim),
    false,
    "kill-session 同上（破坏性动作，前缀命中会杀掉兄弟会话）",
  );
});

if (failed > 0) {
  console.error(`\n${failed} session-backend test(s) failed`);
  throw new Error(`session-backend.test.ts: ${failed} failed`);
}
console.log("\nall session-backend tests passed");

// ★★ P3s-Y2（D 阶段补）：**产 `create` 的路径，名字不许是 `deriveTmuxName` 的直出。**
//
// 拆掉 `or` 之后这条从「保险」变成「必需」：`new-session` 不再吞错，名字撞了就是**建失败**。
// 而撞名由铸造口（`mintTmuxName`，全仓唯一带避让的那个）在**上游**保证不发生。
//
// ⚠ D 阶段核出 DoD 的措辞盖不住实情：三个调用点里**只有一个是新建**，
//   另两个传的是**已有会话名**（`account-restart` 重启原会话 · `fork-flow` 分叉）。
//   ⇒ 判据的人群是「**要新造一个名字**的那些地方」，不是「所有调 create 的地方」。
//
// 钉法：按**源码数据流**——`deriveTmuxName(` 的返回值不许直接流进 `runRemoteLauncher` /
// `runRemoteResumeTmux` 的 name 位；它必须先过 `mintTmuxName(`。
// （形状抄 `history.rs::the_local_launch_tries_the_renderer_before_the_old_path`：钉顺序/数据流，
//  不钉「调用过某函数」——后者「调了但没用返回值」就骗过去了。）
test("P3s-Y2：新造名字的路径，deriveTmuxName 的结果必须过铸造口", () => {
  const files = ["remote-launch-run.ts", "settings/machine-card.ts"];
  let checked = 0;
  for (const f of files) {
    const src = readFileSync(new URL(`./${f}`, import.meta.url), "utf8");
    // ⚠ **按代码行扫，不扫注释** —— 首跑就被自己的注释咬了一口（本会话第六次栽在
    //   「判据被自己要钉的那个名字命中」上）：那几段解释「为什么要过铸造口」的注释里
    //   逐字写着 `deriveTmuxName(`，于是判据把**说明**当成了**违规**。
    for (const line of src.split("\n")) {
      const code = line.trim();
      if (code.startsWith("//") || code.startsWith("*") || code.startsWith("/*")) continue;
      if (!code.includes("deriveTmuxName(")) continue;
      // ⚠ **UI 文案不算产名** —— 第二次假阳：`machine-card` 里那句
      //   「留空则用 ${deriveTmuxName(cwd)}」只是给用户看**建议名**，不流进 name 位。
      //   ⇒ 人群是「**产出一个会被用作会话名的值**」，不是「出现过这个函数」。
      //   判法：那一行要么是赋值/传参（`=` 或 `,` 结尾），要么就是展示。
      if (code.includes("`") && !code.includes("mintTmuxName(")) continue;
      checked += 1;
      if (!code.includes("mintTmuxName(")) {
        const before = code;
        throw new Error(
          `${f}: 有一处 deriveTmuxName( 没被 mintTmuxName( 包住 —— 那是**基名直出**。\n` +
            `C14 拆掉 or 之后 new-session 不再吞错 ⇒ 名字撞了就是建失败给用户看。\n` +
            `而撞名本该由铸造口在上游避掉（issue #76 那一族）。那一行：${before}`,
        );
      }
    }
  }
  // 完备性自检：一处都没扫到 = 抽取器坏了（人群空时「全过」与「没测」长得一样）。
  if (checked < 2) throw new Error(`只扫到 ${checked} 处 deriveTmuxName( —— 抽取器坏了，本条此刻无效`);
});
