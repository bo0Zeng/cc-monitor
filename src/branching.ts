/**
 * issue #8：JSONL parentUuid 分叉 → ESC 回退分支检测。
 *
 * **数据背景**：
 * - Claude Code CLI 双击 ESC = 在某条历史 user message 处重新编辑发送
 * - jsonl 表现：被回退的旧分支保留在文件里，新 user record 的 `parentUuid`
 *   指向**回退到的那个历史节点的 parent**（即同一个 parent 下产生第二个 child）
 * - 实测 1297 条记录的真实 jsonl：~3% parent 形成 fork
 *
 * **主线算法详见** `computeMainBranch` —— "只在 fork 点选 latest-descendant 赢家"。
 * 单链 / 多 root / 无分叉的会话整体 on-main 不折叠；只有真正的 fork 才把
 * 被抛弃兄弟子树标 off-main。
 *
 * **不在这里处理**：
 * - DOM 折叠 UI → branch-fold.ts
 *
 * **链完整性**（详 [[project-branching-algorithm]] 笔记）：
 * - attachment + system 记录不渲染卡片，但夹在 user/assistant 链中间
 * - 必须 track 它们的 uuid+parentUuid，否则 parent 链断成碎片 → 大量误折叠
 * - extractBranchRecord 显式接受这四种类型
 */

export interface BranchRecord {
  uuid: string;
  parentUuid?: string;
  /** ISO 8601 字符串。字典序 = 时间序，可直接 string compare */
  timestamp: string;
}

/**
 * 返回**主线** uuid 集合。其他记录在调用方应被识别为"被 ESC 回退"。
 *
 * **算法**：「只在 fork 点选赢家」
 *  1. 构建 children 索引：parent → [child1, child2, ...]
 *  2. 找出所有 root（parentUuid 为 null 或不在集合里）
 *  3. 从每个 root 往下走：
 *     - 单 child：直接进，整路 on-main
 *     - 多 child（fork 点 = ESC 回退处）：算每个 child 子树的 latest-descendant-timestamp，
 *       选最大的那个 child 继续走，其他 child 子树**整体**off-main
 *
 * **为什么不用之前的"全局最新 leaf 倒推"**：那样在 /compact 或多 root 场景会把
 * 整棵 pre-compact 树误标 off-main（因为 latest leaf 在 post-compact 树里）。
 * 现在每个 root 独立处理，"被回退"严格定义为"在同一个 parent 下被其他兄弟抢走"。
 *
 * **复杂度**：O(N)。children 构建 O(N)；每个节点至多被 DFS 访问一次（memoized 子树最大 ts）。
 *
 * 空记录集 → 空 Set。
 */
export function computeMainBranch(records: ReadonlyArray<BranchRecord>): Set<string> {
  if (records.length === 0) return new Set();

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

  // memoize: uuid → 该子树（含自己）的最大 timestamp。
  const latestDescTs = new Map<string, string>();
  function dfsLatest(r: BranchRecord): string {
    const cached = latestDescTs.get(r.uuid);
    if (cached !== undefined) return cached;
    let max = r.timestamp;
    const kids = childrenOf.get(r.uuid);
    if (kids) {
      for (const k of kids) {
        const kts = dfsLatest(k);
        if (kts > max) max = kts;
      }
    }
    latestDescTs.set(r.uuid, max);
    return max;
  }

  // 从每个 root 走主线：fork 点选 latest-descendant-ts 最大的 child
  const onMain = new Set<string>();
  function walkMain(r: BranchRecord): void {
    if (onMain.has(r.uuid)) return; // 环防御
    onMain.add(r.uuid);
    const kids = childrenOf.get(r.uuid);
    if (!kids || kids.length === 0) return;
    if (kids.length === 1) {
      walkMain(kids[0]);
      return;
    }
    // fork：选赢家
    let winner = kids[0];
    let winnerTs = dfsLatest(kids[0]);
    for (let i = 1; i < kids.length; i++) {
      const ts = dfsLatest(kids[i]);
      if (ts > winnerTs) {
        winner = kids[i];
        winnerTs = ts;
      }
    }
    walkMain(winner);
    // 兄弟 kids 不递归 → 它们整个子树都不进 onMain（= off-main）
  }

  for (const root of roots) {
    walkMain(root);
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
 */
export function extractBranchRecord(rec: {
  type: string;
  uuid?: string;
  parentUuid?: string;
  timestamp?: string;
}): BranchRecord | null {
  if (rec.type !== "user" && rec.type !== "assistant" && rec.type !== "attachment" && rec.type !== "system") {
    return null;
  }
  if (!rec.uuid || !rec.timestamp) return null;
  return { uuid: rec.uuid, parentUuid: rec.parentUuid, timestamp: rec.timestamp };
}
