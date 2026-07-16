/**
 * branching.ts 主线算法（computeMainBranch）断言脚本。issue #8 + #22 + #25。
 *
 * 跑法：`npm run test:branching`（node ≥23.6 原生跑 TS；`npx tsx` 也可）。
 * 同 cards/diff.test.ts：零 node 依赖（不 import node:assert / 不用 process），失败
 * throw → 进程非零退出作 pre-push 门禁；`tsc --noEmit` 也会类型检查本文件。
 */

import {
  computeMainBranch,
  exemptQueuedLeaves,
  extractBranchRecord,
  type BranchRecord,
} from "./branching.ts";

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
function rec(
  uuid: string,
  parentUuid: string | undefined,
  ts: string,
  type: string,
  isInterrupt = false,
): BranchRecord {
  return { uuid, parentUuid, timestamp: ts, type, isInterrupt };
}
/** computeMainBranch → 排序后的 on-main uuid 数组，便于断言。 */
function onMain(records: BranchRecord[]): string[] {
  return [...computeMainBranch(records)].sort();
}
function eqSet(actual: string[], expected: string[], msg: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify([...expected].sort());
  if (a !== e) throw new Error(`${msg}: expected ${e}, got ${a}`);
}

console.log("branching.test.ts");

// 1. 单链单 root → 全 on-main（skill / 命令也是接在链上的 user，等同此例）
test("single chain → all on-main", () => {
  const r = [
    rec("A", undefined, "t01", "user"),
    rec("B", "A", "t02", "assistant"),
    rec("C", "B", "t03", "user"),
    rec("D", "C", "t04", "assistant"),
  ];
  eqSet(onMain(r), ["A", "B", "C", "D"], "single chain");
});

// 2. 树内 fork（issue #8 既有行为不回归）→ latest-descendant 晚的赢、早的兄弟折叠
test("fork inside tree → latest sibling wins (existing #8 behavior)", () => {
  const r = [
    rec("A", undefined, "t01", "user"),
    rec("B", "A", "t02", "assistant"),
    rec("C1", "B", "t03", "user"), // 被回退的兄弟
    rec("C2", "B", "t05", "user"), // 赢家
  ];
  eqSet(onMain(r), ["A", "B", "C2"], "fork");
});

// 3. issue #22 flavor a：首条消息 claude 回复前就回撤 → 无 assistant 的废弃 root 折叠
test("#22 first-msg retract before response → abandoned root folded", () => {
  const r = [
    rec("R1", undefined, "t01", "user"), // 废弃首条（无子、无 assistant）
    rec("R2", undefined, "t03", "user"), // 重发
    rec("A2", "R2", "t04", "assistant"),
  ];
  eqSet(onMain(r), ["R2", "A2"], "#22 flavor a");
});

// 4. issue #22 flavor b：dfaf8554 式三连回撤（打断后回撤）→ 折叠前 2、留最后
test("#22 triple retract (interrupted) → fold first two, keep last", () => {
  const r = [
    rec("R1", undefined, "t01", "user"),
    rec("A1", "R1", "t02", "assistant"),
    rec("I1", "A1", "t03", "user", true), // 打断叶子
    rec("R2", undefined, "t04", "user"),
    rec("A2", "R2", "t05", "assistant"),
    rec("I2", "A2", "t06", "user", true),
    rec("R3", undefined, "t07", "user"), // 当前活跃
    rec("A3", "R3", "t08", "assistant"),
  ];
  eqSet(onMain(r), ["R3", "A3"], "#22 flavor b");
});

// 5. /compact（关键不误折）：system 边界 root + pre-compact 完整对话 user root 全保留
test("/compact multi-root → all kept (system root + pre-compact history)", () => {
  const r = [
    rec("P1", undefined, "t01", "user"), // pre-compact 首条
    rec("PA", "P1", "t02", "assistant"), // pre-compact 完整对话（末尾 assistant、非打断）
    rec("S", undefined, "t03", "system"), // /compact 边界 = system root
    rec("CS", "S", "t04", "user"), // 续接的 isCompactSummary user
    rec("CA", "CS", "t05", "assistant"),
  ];
  eqSet(onMain(r), ["CA", "CS", "P1", "PA", "S"], "/compact");
});

// 6. 链断 root（parentUuid 指向集合外、被裁祖先）的完整对话 → 不误折
test("chain-break root (trimmed ancestor) complete conversation → kept", () => {
  const r = [
    rec("X", "MISSING", "t01", "user"), // parent 不在集合 → 视为 root
    rec("XA", "X", "t02", "assistant"),
    rec("Y", undefined, "t03", "user"),
    rec("YA", "Y", "t04", "assistant"),
  ];
  eqSet(onMain(r), ["X", "XA", "Y", "YA"], "chain-break");
});

// 7. audit 实测漏折：打断后回撤的 root，末尾还跟了 /model 等 system local-command（ts 更晚）
//    → 仍应折叠（按"最新会话叶子是 interrupt"判，忽略尾随 system meta）
test("#22 retract with trailing system /command after interrupt → still folded", () => {
  const r = [
    rec("R1", undefined, "t01", "user"),
    rec("A1", "R1", "t02", "assistant"),
    rec("I1", "A1", "t03", "user", true), // 打断（最新会话叶子）
    rec("S1", "I1", "t04", "system"), // 末尾 /model 命令（ts 更晚但非会话）
    rec("S2", "S1", "t05", "system"), // /model 输出
    rec("R2", undefined, "t06", "user"), // 重发（活跃）
    rec("A2", "R2", "t07", "assistant"),
  ];
  eqSet(onMain(r), ["A2", "R2"], "trailing system after interrupt");
});

// 8. 单个死胡同 root 是唯一 root = winner → 必须保留（别折叠唯一内容）
test("single dead-end root is winner → kept", () => {
  eqSet(onMain([rec("R1", undefined, "t01", "user")]), ["R1"], "single dead-end winner");
});

// 9. 空集 → 空
test("empty → empty", () => {
  eqSet(onMain([]), [], "empty");
});

// ===== issue #25：重复投递（at-least-once）幂等性 =====
// 投递层可能换 seq 重投同一记录（watcher 截断重读）。修复前重复记录毒化 Kahn：
// childrenOf 同一 child 计两次 → remaining 扣不到 0 → 重复点全部祖先 leftover
// fallback（latestDescTs=自身、hasAssistant=false）→ fork/多 root 误判大段折叠。

// 10. root 级毒化（整段历史误折的最小复现）：pre-compact 完整对话里 1 条 attachment
//     重投 → 修复前 user root 祖先链 leftover、被当"无 assistant 死胡同"整棵折叠
test("#25 duplicated attachment in pre-compact tree → result unchanged", () => {
  const r = [
    rec("R1", undefined, "t01", "user"), // pre-compact 首条
    rec("A1", "R1", "t02", "assistant"),
    rec("ATT", "A1", "t03", "attachment"),
    rec("U2", "ATT", "t04", "user"),
    rec("A2", "U2", "t05", "assistant"), // 完整对话，非打断
    rec("S", undefined, "t06", "system"), // /compact 边界 root
    rec("CS", "S", "t07", "user"),
    rec("CA", "CS", "t08", "assistant"), // 全局最新 → S 是 winner root
  ];
  const clean = onMain(r);
  eqSet(clean, ["A1", "A2", "ATT", "CA", "CS", "R1", "S", "U2"], "clean baseline");
  // 同一条 attachment 重投一次（换 seq 在 BranchRecord 层不可见 = 同对象再来一遍）
  eqSet(onMain([...r, rec("ATT", "A1", "t03", "attachment")]), clean, "dup attachment");
});

// 11. fork 级毒化（"尾段 N 条"误折的最小复现）：fork 赢家子树里 1 条 attachment
//     重投 → 修复前赢家祖先 leftover、latestDescTs 退化为自身 ts、输给早兄弟
test("#25 duplicated attachment inside fork winner subtree → winner unchanged", () => {
  const r = [
    rec("A", undefined, "t01", "user"),
    rec("B", "A", "t02", "assistant"),
    rec("C2", "B", "t04", "user"), // 真赢家（子树最新 t08）
    rec("ATT", "C2", "t06", "attachment"),
    rec("D2", "ATT", "t08", "assistant"),
    rec("C1", "B", "t05", "user", true), // 被回退兄弟（自身 ts 晚于 C2 自身 t04，毒化后会反超）
  ];
  const clean = onMain(r);
  eqSet(clean, ["A", "B", "ATT", "C2", "D2"], "clean baseline");
  eqSet(onMain([...r, rec("ATT", "C2", "t06", "attachment")]), clean, "dup in winner subtree");
});

// 12. 全文件重投（截断重读的真实形态：整个文件换 seq 再来一遍）→ 结果完全不变。
//     第二遍用重新构造的对象（真实重投是反序列化出的新对象，不依赖对象同一性）。
test("#25 full re-delivery (records doubled) → idempotent", () => {
  const build = () => [
    rec("R1", undefined, "t01", "user"),
    rec("A1", "R1", "t02", "assistant"),
    rec("I1", "A1", "t03", "user", true),
    rec("R2", undefined, "t04", "user"),
    rec("A2", "R2", "t05", "assistant"),
    rec("S", undefined, "t06", "system"),
    rec("CS", "S", "t07", "user"),
  ];
  const r = build();
  eqSet(onMain([...r, ...build()]), onMain(r), "doubled = single");
});



// === issue #36：队列消息豁免（exemptQueuedLeaves）——真实样本双 fixture ===

function urec(uuid: string, parentUuid: string | undefined, ts: string, text?: string, isInterrupt = false): BranchRecord {
  return { uuid, parentUuid, timestamp: ts, type: "user", isInterrupt, text };
}

test("#36A 队列形态（0cbbdbae 实样）：命中 enqueue 集合的裸 user 叶被豁免保留", () => {
  // X(assistant) ├─ b(队列消息, 永久裸叶) └─ tr → int(interrupt) → reply
  const records: BranchRecord[] = [
    rec("X", undefined, "T01", "assistant"),
    urec("b", "X", "T02", "这是登录门户"),
    urec("tr", "X", "T02"),
    urec("int", "tr", "T03", undefined, true),
    rec("reply", "int", "T04", "assistant"),
  ];
  const main = new Set(["X", "tr", "int", "reply"]);
  const out = exemptQueuedLeaves(records, main, new Set(["这是登录门户"]));
  if (!out.has("b")) throw new Error("b 应被豁免");
  if (out.size !== 5) throw new Error(`原主线不该受影响, got size=${out.size}`);
});

test("#36B 重发形态（7196b2f9 实样）：弃稿不在 enqueue 集合照旧折叠 + 空集合零拷贝", () => {
  const records: BranchRecord[] = [
    rec("P", undefined, "T01", "system"),
    urec("a1", "P", "T02", "为什么是这样的?"),
    urec("a2", "P", "T03", "为什么是这样的?"),
    urec("win", "P", "T04", "为什么是这样的?"),
    rec("resp", "win", "T05", "assistant"),
  ];
  const main = new Set(["P", "win", "resp"]);
  const out = exemptQueuedLeaves(records, main, new Set());
  if (out !== main) throw new Error("空豁免集合应零拷贝返回原 main");
  const out2 = exemptQueuedLeaves(records, main, new Set(["别的内容"]));
  if (out2.has("a1") || out2.has("a2")) throw new Error("弃稿不该被豁免");
});

test("#36C 命中集合但有子女（挂了 interrupt 叶）→ 非裸叶不豁免（真被回退过）", () => {
  const records: BranchRecord[] = [
    rec("X", undefined, "T01", "assistant"),
    urec("q", "X", "T02", "排队过又被中断"),
    urec("qi", "q", "T03", undefined, true),
    urec("w", "X", "T04", "其他"),
    rec("wr", "w", "T05", "assistant"),
  ];
  const out = exemptQueuedLeaves(records, new Set(["X", "w", "wr"]), new Set(["排队过又被中断"]));
  if (out.has("q")) throw new Error("非裸叶不该被豁免");
});

// === F63 (issue #49)：看不懂的记录进链 —— 防孤儿化 ===

test("F63 extractBranchRecord 接受 cc-monitor-unrecognized（带身份）", () => {
  const r = extractBranchRecord({
    type: "cc-monitor-unrecognized",
    uuid: "u5",
    parentUuid: "u4",
    timestamp: "T05",
  });
  if (!r) throw new Error("带 uuid+timestamp 的 unrecognized 必须进链");
  if (r.uuid !== "u5" || r.parentUuid !== "u4") throw new Error("身份必须原样带出");
  if (r.isInterrupt) throw new Error("非 user 恒非 interrupt");
});

test("F63 无身份的 unrecognized 照旧不进链（本机 7 个未知 type 全属此类）", () => {
  // 实测 mode/agent-name/pr-link/… 8774 条全无 uuid —— 抢救了原文，但不进链，
  // 因为它们本来就不在 parent 链上。:273 的 !uuid 守卫就是干这个的。
  const r = extractBranchRecord({ type: "cc-monitor-unrecognized", timestamp: "T01" });
  if (r !== null) throw new Error("无 uuid 不该进链");
});

// 真实管线：原始 jsonl 记录 → extractBranchRecord（含 F63 白名单）→ computeMainBranch。
// 走这条 = 端到端守 F63 的白名单改动（直接构造 BranchRecord 会绕过白名单，测不到）。
// 形状是 extractBranchRecord 的入参（type/uuid/parentUuid/timestamp）。
function pipelineOnMain(
  raw: Array<{ type: string; uuid?: string; parentUuid?: string; timestamp?: string }>,
): string[] {
  const branchRecords = raw
    .map((r) => extractBranchRecord(r))
    .filter((r): r is BranchRecord => r !== null);
  return [...computeMainBranch(branchRecords)].sort();
}

test("F63 unrecognized 经 extractBranchRecord 进链、不断链", () => {
  // u1(用户提问) → X(看不懂的记录) → a1(assistant 回复)：X 是链上必经的一环。
  // 走真实管线：白名单认 X → 整条单链全 on-main。
  const chain = [
    { type: "user", uuid: "u1", timestamp: "T01" },
    { type: "cc-monitor-unrecognized", uuid: "X", parentUuid: "u1", timestamp: "T02" },
    { type: "assistant", uuid: "a1", parentUuid: "X", timestamp: "T03" },
  ];
  eqSet(pipelineOnMain(chain), ["X", "a1", "u1"], "X 经白名单进链 → 单链全 on-main");
});

// ★ F63 真正防的误折叠 —— 有真对照（认 X vs 拒 X 结果不同），且**端到端经白名单**。
//
// 拓扑经 scratchpad/verify_topology.ts + mutation test 实证，不是手算：
//   u1(用户提问) → X(带 uuid/parentUuid 的看不懂记录) → a1(assistant 回复)
//
// - 白名单认 X（F63 后）：u1→X→a1 单链，{u1,X,a1} 全 on-main。
// - 白名单拒 X（= F63 之前 / 把 :283 那行 mutation 掉）：extractBranchRecord(X)=null →
//   X 不进链 → a1 的 parentUuid="X" 不在集合 → a1 孤儿 root，ts 最新自成 winner 保留；
//   而 **u1 失去唯一后代通道，退化成无 assistant 后代的 plain user 死胡同 root →
//   :48-50 判「被 ESC 回撤废弃」→ 折叠**。结果 on-main = {a1}，**用户的提问 u1 消失了**。
//
// 关键洞察：缺一条中间记录，受害的是它的 **parent**（失去后代链变死胡同），
// 不是它的 child（child 孤儿化后常因 ts 最新自成 winner 而保留）。
//
// 因为走 pipelineOnMain（经 extractBranchRecord），把 :283 白名单里的
// "cc-monitor-unrecognized" 去掉，本测试会挂 —— 已用 mutation test 证过它非装饰。
//
// 诚实标注：这需要「未知 type 带 uuid/parentUuid」——本机真实数据里这种样本
// 当前为 0（Phase B 实证：7 个未知 type 的 uuid 全为空），故这是**前瞻护栏**，
// 对应 Claude 未来发一个带链身份的新记录类型的那天。且只在此精确拓扑（被删节点
// 是 user→其唯一 assistant 后代的必经中间点）触发 —— 印证了 F63 是保险不是流血伤口。
test("F63 ★ 白名单拒 unrecognized → 上游用户提问被误折（这就是 F63 要防的）", () => {
  const raw = [
    { type: "user", uuid: "u1", timestamp: "T01" },
    { type: "cc-monitor-unrecognized", uuid: "X", parentUuid: "u1", timestamp: "T02" },
    { type: "assistant", uuid: "a1", parentUuid: "X", timestamp: "T03" },
  ];
  // F63 后：X 经白名单进链，用户提问 u1 保留。
  eqSet(pipelineOnMain(raw), ["X", "a1", "u1"], "F63 后 u1 提问必须保留");

  // F63 前（模拟白名单没认它 = 去掉 X）：u1 被误折。
  const withoutX = raw.filter((r) => r.uuid !== "X");
  const on2 = pipelineOnMain(withoutX);
  if (on2.includes("u1")) {
    throw new Error(
      "对照失效：X 不进链时 u1 本应被误折（若此处 u1 仍在，说明 branching 语义变了，" +
        "F63 的论据要重估 —— 见 branching.ts:24 的预警注释）",
    );
  }
  eqSet(on2, ["a1"], "X 不进链 → 只剩孤零零的回复 a1，提问 u1 丢失");
});

if (failed > 0) {
  console.error(`\n${failed} branching test(s) failed`);
  throw new Error(`branching.test.ts: ${failed} failed`);
}
console.log("\nall branching tests passed");