/**
 * issue #8：JSONL parentUuid 分叉 → ESC 回退分支检测。
 *
 * **数据背景**：
 * - Claude Code CLI 双击 ESC = 在某条历史 user message 处重新编辑发送
 * - jsonl 表现：被回退的旧分支保留在文件里，新 user record 的 `parentUuid`
 *   指向**回退到的那个历史节点的 parent**（即同一个 parent 下产生第二个 child）
 * - 实测 1297 条记录的真实 jsonl：~3% parent 形成 fork
 *
 * **主线算法详见** `computeMainBranch` —— "fork 点选 latest-descendant 赢家" +
 * "多 root 折叠被 ESC 回撤废弃的首条/重发"（issue #22）。单链 / 无分叉的会话整体
 * on-main 不折叠；fork 把被抛弃兄弟子树标 off-main；多 root 时只折叠死胡同的 plain
 * user root（/compact 的 system root、完整对话历史都保留）。
 *
 * **幂等**（issue #25）：`computeMainBranch` 入口按 uuid 去重（保首见）——行投递是
 * at-least-once（doc/INVARIANTS.md § 25），重复输入不得毒化 Kahn 拓扑。caller 可以
 * 依赖这层兜底。
 *
 * **不在这里处理**：
 * - DOM 折叠 UI → branch-fold.ts
 *
 * **链完整性**（详 [[project-branching-algorithm]] 笔记）：
 * - attachment + system 记录不渲染卡片，但夹在 user/assistant 链中间
 * - 必须 track 它们的 uuid+parentUuid，否则 parent 链断成碎片 → 大量误折叠
 * - extractBranchRecord 显式接受这五种类型（F63 起含 `cc-monitor-unrecognized`
 *   —— 看不懂的记录也是链上的环，同理不能缺席；详见该函数头注释）
 */

export interface BranchRecord {
  uuid: string;
  parentUuid?: string;
  /** ISO 8601 字符串。字典序 = 时间序，可直接 string compare */
  timestamp: string;
  /** issue #22：记录类型（user/assistant/system/attachment）。多 root 分类用。 */
  type: string;
  /** issue #22：该 user 记录是否是 "[Request interrupted by user…]" 打断标记（回撤死胡同信号）。 */
  isInterrupt?: boolean;
  /** issue #36：user 记录的文本内容（trim 后）——队列消息豁免匹配用。非 user 恒缺省。 */
  text?: string;
}

/**
 * 返回**主线** uuid 集合。其他记录在调用方应被识别为"被 ESC 回退"。
 *
 * **算法**：「fork 点选赢家 + 多 root 折叠废弃回撤」
 *  0. 入口按 uuid 去重（issue #25：对 at-least-once 重复投递幂等）
 *  1. 构建 children 索引：parent → [child1, child2, ...]
 *  2. 找出所有 root（parentUuid 为 null 或不在集合里）
 *  3. issue #22：多 root 分类。winner = latestDescTs 最大的 root（当前活跃分支）永远保留；
 *     其余 root 若是 plain user 且子树死胡同（无 assistant 后代 / 最新会话叶子是 interrupt）
 *     → 判为被 ESC 回撤废弃的首条/重发，整棵折叠。system root（/compact）、完整对话 root
 *     （/clear、链断、pre-compact 历史）保留。
 *  4. 对保留的 root 往下走：
 *     - 单 child：直接进，整路 on-main
 *     - 多 child（fork 点 = ESC 回退处）：算每个 child 子树的 latest-descendant-timestamp，
 *       选最大的那个 child 继续走，其他 child 子树**整体**off-main
 *
 * **为什么不用"全局最新 leaf 倒推"**：那样在 /compact 场景会把整棵 pre-compact 树误标
 * off-main（latest leaf 在 post-compact 树里）。现在每个**保留的** root 独立处理；多 root
 * 的"废弃回撤"判定见步骤 3（首条消息回撤是 root 边界问题：被弃首条是 parentUuid=null 的
 * root、不是同父兄弟，旧 fork 检测抓不到，故在 root 层单独判）。
 *
 * **复杂度**：O(N)。children 构建 O(N)；latestDescTs 拓扑序累加 O(N)；walk 主线 O(N)。
 *
 * **为什么全部用迭代**：v2.1.0 用过真递归（dfsLatest + walkMain）。Claude session
 * 的 parent 链典型几乎线性，递归深度 = 链长度，几千条记录就在 WebView2 上炸 stack
 * （RangeError）→ replay drain 整个死掉，前端只渲染前几条。v2.1.1 改自底向上 Kahn
 * 拓扑序算 latestDescTs + while 循环 walk，深度 1 帧。
 *
 * 空记录集 → 空 Set。
 */
export function computeMainBranch(rawRecords: ReadonlyArray<BranchRecord>): Set<string> {
  if (rawRecords.length === 0) return new Set();

  // issue #25：入口按 uuid 去重（保首见）——算法对重复输入必须幂等。
  // 投递层是 at-least-once（违反此约束见 doc/INVARIANTS.md § 25）：watcher 截断
  // 重读（watcher.rs::process_file）会把整个文件换新 seq 重投，tab.seenSeqs（#17）
  // 只防同 seq。重复记录一旦进入下面的 childrenOf，同一 child 被计两次 → Kahn 的
  // remaining 永远扣不到 0 → 重复点的全部祖先落 leftover fallback（latestDescTs=
  // 自身、hasAssistant=false 全错）→ fork 赢家/多 root 分类误判，最坏整段历史被当
  // ESC 回撤折叠（实测 1 条重复 attachment 即可折掉 1541/4331 条）。
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
  // issue #22：多 root 分类信号，随 latestDescTs 一趟自底向上算出——
  //   latestConvTs / latestConvIsInterrupt：子树里**会话记录**（user/assistant）中 ts
  //     最大那条、及它是不是 interrupt 打断叶子。**只看会话记录**：末尾尾随的 system
  //     local-command（/model、/config 等）ts 可能更晚，但不代表对话还在继续——判"对话
  //     是否停在打断处"要忽略这些尾随 meta 记录（audit 实测 dfaf8554 踩过）。
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
  while (queue.length > 0) {
    const r = queue.shift()!;
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
  // 入口已按 uuid 去重，重复输入不会再走到这里——若仍非空，大声留证（issue #25：
  // leftover 的 fallback 信号是错的，会引发误折叠；这行 warn 是异常输入的第一证人，
  // 带前几个 uuid 方便指认；live 模式每条新记录都重算，真出环会反复打——刷屏本身
  // 也是"赶紧来修"的信号，不去抖）。
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

  // 迭代 walk 主线：原来 walkMain 是 tail-recursive（每次只下钻一条路径），
  // 直接改 while 循环，深度 1 帧。
  // issue #22：多 root 分类。winner = latestDescTs 最大的 root = 当前活跃分支，永远保留。
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
 * 从 JSONL 记录抽 BranchRecord（uuid+parentUuid+timestamp）。返回 null 表示
 * 该记录无 uuid，不参与链。
 *
 * 接受 user / assistant / attachment / system —— 这四种在 jsonl 里有 uuid+parentUuid
 * 字段；其他类型（ai-title / file-history-snapshot / last-prompt / permission-mode）
 * 没 uuid 不进链。
 *
 * **F63 (issue #49)** 加第五种 `cc-monitor-unrecognized`：后端 `parser.rs::salvage`
 * 对**看不懂的记录**抢救出的原文 + 身份（不是真实 jsonl 里的类型，是我们自造的
 * 信封，故带 `cc-monitor-` 前缀防撞）。它必须进链——否则等于没救：一条记录缺席，
 * 它的 children 的 parentUuid 就指向集合外，落到 `:100-106` 的 roots → `:48-50`
 * 死胡同 plain user root 整棵折叠。**无身份的自然被下面 `!rec.uuid` 挡掉**，正是
 * 我们要的（本机实测 7 个未知 type 全无 uuid，故今天这条路一个都不会进链）。
 */
export function extractBranchRecord(rec: {
  // **C04c**：`| null` 是线上的实情。这些字段在 Rust 侧是 `Option<T>` 且**没有**
  // `skip_serializing_if` ⇒ `None` 序列化成**显式 null**、不是省略。
  // 下面 `!rec.uuid` / `?? ""` 这些真值判断本来就吃得下 null，只是此前的类型没说实话
  // ——生成的 `JsonlRecord` 一接进来就把这个谎揭出来了（`tsc` 报在这条调用上）。
  type: string;
  uuid?: string | null;
  parentUuid?: string | null;
  timestamp?: string | null;
  message?: { content?: unknown };
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
    // **C04c 的边界归一化**：`BranchRecord` 是前端自己的分支图模型（不是线上类型，
    // 同账本第 4 行「IR 是前端的意图模型」）⇒ 不把线上的 null 传染进去，在入口处收成 undefined。
    parentUuid: rec.parentUuid ?? undefined,
    timestamp: rec.timestamp,
    type: rec.type,
    // issue #22：只有 user 记录可能是回撤打断叶子；其他类型恒 false。
    isInterrupt: rec.type === "user" && userText.startsWith("[Request interrupted by user"),
    // issue #36：队列消息豁免匹配用
    text: rec.type === "user" && userText ? userText.trim() : undefined,
  };
}

/** user 记录的文本内容提取（string content 或首个 text block）。issue #22/#36 共用。 */
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
 * issue #36：队列消息豁免——CC 处理输入队列消息时"内容上消费、链上遗弃"（回复
 * 链挂在 interrupt 叶下继续，队列消息记录永久裸叶），fork 输家判定会把它误判成
 * "被 ESC 回退"折叠。CC 自己写的 `queue-operation` enqueue 记录带 content——
 * 非主线的**裸 user 叶**（无任何子女）且文本命中 enqueue 集合 → 并回 main 集合
 * （保留显示）。重发弃稿不在 enqueue 集合，既有折叠判据不受影响。
 * 纯函数；两份真实样本 fixture 见测试（0cbbdbae 队列形态 / 7196b2f9 重发形态）。
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
