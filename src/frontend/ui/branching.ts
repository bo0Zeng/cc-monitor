/**
 * JSONL parentUuid 分叉 → ESC 回退分支检测。
 *
 * - Claude Code 双击 ESC = 在某条历史 user message 处重新编辑发送；被回退的旧分支留在文件里，
 *   新 user 记录的 `parentUuid` 指向回退到的那个节点的 parent（同一个 parent 下多出一个 child）。
 * - 主线算法见 `computeMainBranch`：fork 点选最新后代那一支；多 root 时只折叠死胡同的 plain user root
 *   （被 ESC 回撤废弃的首条 / 重发），/compact 的 system root 与完整对话历史保留。单链会话整体在主线上、不折叠。
 * - 幂等：入口按 uuid 去重（保首见）—— 行投递是 at-least-once（src/doc/INVARIANTS.md § 25）。
 * - 链完整性：attachment / system 记录不出卡但夹在链中间，必须进链，否则链断成碎片、大量误折叠；
 *   `extractBranchRecord` 收五种（含看不懂的记录 `cc-monitor-unrecognized`）。
 * - DOM 折叠在 branch-fold.ts。
 */

export interface BranchRecord {
  uuid: string;
  parentUuid?: string;
  /** ISO 8601 字符串。字典序 = 时间序，可直接 string compare */
  timestamp: string;
  /** 记录类型（user / assistant / system / attachment）。多 root 分类用。 */
  type: string;
  /** 该 user 记录是否是 "[Request interrupted by user…]" 打断标记（回撤死胡同信号）。 */
  isInterrupt?: boolean;
  /** user 记录的文本内容（trim 后）：队列消息豁免匹配用。非 user 恒缺省。 */
  text?: string;
}

/**
 * 返回**主线** uuid 集合。其他记录在调用方应被识别为"被 ESC 回退"。
 *
 * **算法**：「fork 点选赢家 + 多 root 折叠废弃回撤」
 *  0. 入口按 uuid 去重（对 at-least-once 重复投递幂等）
 *  1. 构建 children 索引：parent → [child1, child2, ...]
 *  2. 找出所有 root（parentUuid 为 null 或不在集合里）
 *  3. 多 root 分类。winner = latestDescTs 最大的 root（当前活跃分支）永远保留；
 *     其余 root 若是 plain user 且子树死胡同（无 assistant 后代 / 最新会话叶子是 interrupt）
 *     → 判为被 ESC 回撤废弃的首条/重发，整棵折叠。system root（/compact）、完整对话 root
 *     （/clear、链断、pre-compact 历史）保留。
 *  4. 对保留的 root 往下走：
 *     - 单 child：直接进，整路 on-main
 *     - 多 child（fork 点 = ESC 回退处）：算每个 child 子树的 latest-descendant-timestamp，
 *       选最大的那个 child 继续走，其他 child 子树**整体**off-main
 *
 * 不用「全局最新 leaf 倒推」：/compact 场景会把整棵 pre-compact 树误标 off-main（latest leaf 在 post-compact 树里）⇒ 每个保留的 root 独立处理。
 * 首条消息被回撤时被弃的首条是 parentUuid = null 的 root、不是同父兄弟 ⇒ 在 root 层单独判（步骤 3）。
 *
 * 复杂度 O(N)；全部迭代（自底向上 Kahn 拓扑序算 latestDescTs ＋ while 循环 walk）：parent 链几乎线性，递归深度 = 链长，几千条就炸栈。
 *
 * 空记录集 → 空 Set。
 */
export function computeMainBranch(rawRecords: ReadonlyArray<BranchRecord>): Set<string> {
  if (rawRecords.length === 0) return new Set();

  // 入口按 uuid 去重（保首见）：投递层是 at-least-once（src/doc/INVARIANTS.md § 25）。
  // 重复记录进了 childrenOf 会让同一 child 计两次 → Kahn 的 remaining 扣不到 0 → 祖先全落 leftover，
  // fork 赢家与多 root 分类全错，最坏整段历史被当 ESC 回撤折掉。
  const seenUuids = new Set<string>();
  const records: BranchRecord[] = [];
  for (const r of rawRecords) {
    if (!seenUuids.has(r.uuid)) {
      seenUuids.add(r.uuid);
      records.push(r);
    }
  }

  const byUuid = new Map<string, BranchRecord>();
  for (const r of records) {
    byUuid.set(r.uuid, r);
  }

  // parent uuid → children records。注意"parent 在集合里"才入这张表；
  // parentUuid 指向集合外（如 attachment 链断、被裁过的祖先）的记录视为 root。
  const childrenOf = new Map<string, BranchRecord[]>();
  const roots: BranchRecord[] = [];
  for (const r of records) {
    if (r.parentUuid && byUuid.has(r.parentUuid)) {
      const arr = childrenOf.get(r.parentUuid);
      if (arr) arr.push(r);
      else childrenOf.set(r.parentUuid, [r]);
    } else {
      roots.push(r);
    }
  }

  // 迭代算 latestDescTs：Kahn 风格自底向上 —— 先处理 leaves，再处理它们的 parent
  // remaining[uuid] = 该 uuid 还有几个 child 未处理。0 时它本身可以被处理（child 的 ts 都 ready）。
  const latestDescTs = new Map<string, string>();
  // 多 root 分类信号，随 latestDescTs 一趟自底向上算出：
  //   latestConvTs / latestConvIsInterrupt：子树里会话记录（user / assistant）中 ts 最大那条、及它是不是 interrupt 打断叶子。
  //     只看会话记录：尾随的 system local-command（/model、/config）ts 可能更晚，但不代表对话还在继续。
  //   subtreeHasAssistant：子树里有没有 assistant 记录。
  const latestConvTs = new Map<string, string>();
  const latestConvIsInterrupt = new Map<string, boolean>();
  const subtreeHasAssistant = new Map<string, boolean>();
  const remaining = new Map<string, number>();
  const queue: BranchRecord[] = [];
  for (const r of records) {
    const c = childrenOf.get(r.uuid)?.length ?? 0;
    if (c === 0) {
      queue.push(r);
    } else {
      remaining.set(r.uuid, c);
    }
  }
  // 头下标出队，不用 `shift()`（V8 的 left-trim 快路不是无条件的，
  // 长数组上每次 `shift` 退化成整份搬移）。每条记录恰好进队一次 ⇒ 数组不必压缩，长度 ≤ N。
  for (let qh = 0; qh < queue.length; qh++) {
    const r = queue[qh];
    let max = r.timestamp;
    const rIsConv = r.type === "user" || r.type === "assistant";
    // 只统计会话记录（user/assistant）的最新叶子；"" = 自身非会话、暂无会话候选
    let convTs = rIsConv ? r.timestamp : "";
    let convIsInterrupt = rIsConv ? (r.isInterrupt ?? false) : false;
    let hasAssistant = r.type === "assistant";
    const kids = childrenOf.get(r.uuid);
    if (kids) {
      for (const k of kids) {
        const kts = latestDescTs.get(k.uuid);
        if (kts !== undefined && kts > max) max = kts;
        const kConvTs = latestConvTs.get(k.uuid) ?? "";
        if (kConvTs > convTs) {
          convTs = kConvTs;
          convIsInterrupt = latestConvIsInterrupt.get(k.uuid) ?? false;
        }
        if (subtreeHasAssistant.get(k.uuid)) hasAssistant = true;
      }
    }
    latestDescTs.set(r.uuid, max);
    latestConvTs.set(r.uuid, convTs);
    latestConvIsInterrupt.set(r.uuid, convIsInterrupt);
    subtreeHasAssistant.set(r.uuid, hasAssistant);
    // 通知 parent：少一个 pending child
    if (r.parentUuid) {
      const p = byUuid.get(r.parentUuid);
      if (p) {
        const next = (remaining.get(p.uuid) ?? 1) - 1;
        if (next <= 0) {
          remaining.delete(p.uuid);
          queue.push(p);
        } else {
          remaining.set(p.uuid, next);
        }
      }
    }
  }
  // remaining 不空 = 环（理论不可能，jsonl append-only 保证 parent 早于 child；防御）。
  // 入口已按 uuid 去重；仍非空就大声留证（leftover 的信号是错的、会误折叠），带前几个 uuid 方便指认。不去抖：真出环时刷屏本身就是信号。
  if (remaining.size > 0) {
    const sample = [...remaining.keys()].slice(0, 3).join(", ");
    console.warn(
      `[branching] Kahn leftover=${remaining.size}（输入含环？首批: ${sample}）——折叠信号已退化为自身 ts，可能误折叠`,
    );
  }
  // fallback 用自身 ts，避免后续 walkMain 拿到 undefined
  for (const [uuid] of remaining) {
    const r = byUuid.get(uuid);
    if (r) {
      latestDescTs.set(uuid, r.timestamp);
      const conv = r.type === "user" || r.type === "assistant";
      latestConvTs.set(uuid, conv ? r.timestamp : "");
      latestConvIsInterrupt.set(uuid, conv ? (r.isInterrupt ?? false) : false);
      subtreeHasAssistant.set(uuid, r.type === "assistant");
    }
  }

  // 迭代 walk 主线（每次只下钻一条路径 ⇒ while 循环，深度 1 帧）。
  // 多 root 分类：winner = latestDescTs 最大的 root = 当前活跃分支，永远保留。
  // 其余 root 中，**plain user** 且子树是死胡同（无 assistant 后代，或**最新会话叶子**是
  // interrupt 打断；末尾尾随的 /model 等 system 命令不算）→ 判为"被 ESC 回撤废弃的
  // 首条/重发"，整棵折叠（不进 onMain）。
  // system root（/compact 边界）、完整对话 root（/clear、链断祖先、pre-compact 历史）保留。
  let winner: BranchRecord | undefined;
  let winnerTs = "";
  for (const root of roots) {
    const ts = latestDescTs.get(root.uuid) ?? root.timestamp;
    if (winner === undefined || ts > winnerTs) {
      winner = root;
      winnerTs = ts;
    }
  }

  const onMain = new Set<string>();
  for (const root of roots) {
    if (root !== winner && root.type === "user") {
      const hasAssistant = subtreeHasAssistant.get(root.uuid) ?? false;
      const latestIsInterrupt = latestConvIsInterrupt.get(root.uuid) ?? false;
      if (!hasAssistant || latestIsInterrupt) {
        continue; // 废弃 ESC 回撤 root → 整棵折叠
      }
    }
    let cursor: BranchRecord | undefined = root;
    while (cursor) {
      if (onMain.has(cursor.uuid)) break; // 环防御
      onMain.add(cursor.uuid);
      const kids = childrenOf.get(cursor.uuid);
      if (!kids || kids.length === 0) break;
      if (kids.length === 1) {
        cursor = kids[0];
        continue;
      }
      // fork：选 latest-descendant-ts 最大的 child；兄弟子树整体 off-main
      let winner = kids[0];
      let winnerTs = latestDescTs.get(kids[0].uuid) ?? kids[0].timestamp;
      for (let i = 1; i < kids.length; i++) {
        const ts = latestDescTs.get(kids[i].uuid) ?? kids[i].timestamp;
        if (ts > winnerTs) {
          winner = kids[i];
          winnerTs = ts;
        }
      }
      cursor = winner;
    }
  }

  return onMain;
}

/** 两个 Set 内容是否相同（顺序无关）。重建 fold UI 前判等用。 */
export function setsEqual(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const x of a) {
    if (!b.has(x)) return false;
  }
  return true;
}

/**
 * 从 JSONL 记录抽 BranchRecord（uuid ＋ parentUuid ＋ timestamp）；没有 uuid ⇒ null，不进链。
 *
 * 收 user / assistant / attachment / system（这四种带 uuid ＋ parentUuid），外加 `cc-monitor-unrecognized`
 * （后端 `parser.rs::salvage` 对看不懂的记录抢救出的原文 ＋ 身份）：它缺席的话它的 children 指向集合外、
 * 成了 root，被当死胡同整棵折掉。没有身份的由 `!rec.uuid` 挡掉。
 */
export function extractBranchRecord(rec: {
  // `| null`：这些字段在 Rust 侧是 `Option<T>` 且没有 `skip_serializing_if` ⇒ 线上是显式 null。
  type: string;
  uuid?: string | null;
  parentUuid?: string | null;
  timestamp?: string | null;
  message?: { content?: unknown };
  /** user 记录是谁说的（后端判好的成品）；只读「是不是中断标记」。 */
  userText?: { speaker: { kind: string } };
}): BranchRecord | null {
  if (
    rec.type !== "user" &&
    rec.type !== "assistant" &&
    rec.type !== "attachment" &&
    rec.type !== "system" &&
    rec.type !== "cc-monitor-unrecognized"
  ) {
    return null;
  }
  if (!rec.uuid || !rec.timestamp) return null;
  const userText = rec.type === "user" ? contentText(rec.message?.content) : "";
  return {
    uuid: rec.uuid,
    // `BranchRecord` 是前端自己的分支图模型（不是线上类型）⇒ 线上的 null 在入口处收成 undefined。
    parentUuid: rec.parentUuid ?? undefined,
    timestamp: rec.timestamp,
    type: rec.type,
    // 只有 user 记录可能是回撤打断叶子；其他类型恒 false。
    // 「是不是 ESC 中断标记」只读成品（整条恰是它才算，标记后面跟着真话的不算）。
    isInterrupt: rec.type === "user" && rec.userText?.speaker.kind === "interrupt",
    // 队列消息豁免匹配用
    text: rec.type === "user" && userText ? userText.trim() : undefined,
  };
}

/** user 记录的文本内容提取（string content 或首个 text block）。打断判定与队列豁免共用。 */
function contentText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    const block = content.find(
      (b) => b && typeof b === "object" && typeof (b as { text?: unknown }).text === "string",
    );
    return (block as { text?: string } | undefined)?.text ?? "";
  }
  return "";
}

/**
 * 队列消息豁免：CC 处理输入队列消息时「内容上消费、链上遗弃」（回复挂在 interrupt 叶下继续，队列消息成了永久裸叶），
 * fork 输家判定会把它当「被 ESC 回退」折掉。非主线的裸 user 叶且文本命中 enqueue 集合 ⇒ 并回主线（照常显示）；重发弃稿不在 enqueue 集合，照折。纯函数。
 */
export function exemptQueuedLeaves(
  records: ReadonlyArray<BranchRecord>,
  main: Set<string>,
  queuedContents: ReadonlySet<string>,
): Set<string> {
  if (queuedContents.size === 0) return main;
  const hasChild = new Set<string>();
  for (const r of records) {
    if (r.parentUuid) hasChild.add(r.parentUuid);
  }
  let out = main;
  for (const r of records) {
    if (
      r.type === "user" &&
      !main.has(r.uuid) &&
      !hasChild.has(r.uuid) &&
      r.text !== undefined &&
      queuedContents.has(r.text)
    ) {
      if (out === main) out = new Set(main); // copy-on-write
      out.add(r.uuid);
    }
  }
  return out;
}
