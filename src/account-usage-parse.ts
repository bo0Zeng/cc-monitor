/**
 * 🪦 **退役墓碑〔`K-R101` · 2026-09-13〕—— 本模块已从生产路上退役，代码留在盘上。**
 *
 * # 一 · 谁退的它
 *
 * `R59`〔用 09-13〕逐字：「**解析层代码保留, 但是功能先退役**」。**日期：2026-09-13。**
 * 同日 `R58` 逐字：「**要不就直接抓屏给我看好了, 即点击用量后把屏幕预览给我看**」
 * 「**这样也好做其他agent兼容**」。
 * ⇒ 生产路（`src/account-usage.ts` → chip / usage-view / accounts-section）上**零调用**；
 *   点「用量」直接展示 `capture-pane` 抓到的那一屏原文（`account-usage.ts::usageScreenEl`）。
 * ⚠ **退役不是删除**：本文件 ＋ `src/account-usage-parse.vitest.ts` ＋
 *   `src/__fixtures__/usage-capture-2026-07-31.txt` **三样都留在盘上、那套测试继续跑**。
 *
 * # 二 · 复活条件 —— **什么成立了才该把它接回生产路**
 *
 * 🔴 **一句空话都不许写在这里**（件文件逐字点名了那种写法：把复活条件写成「以后……」的
 * 泛泛之词，等于没有复活条件）。这里写的是**可判定的条件** —— 下面这四样里
 * **任何一样被用户要回来**
 * （`R58` 逐条记下的、只给一屏图就会丢掉的东西）：
 *
 *   · **百分比**（一个数字，而不是一屏字）
 *   · **进度条**
 *   · **跨账号排序**
 *   · **「哪个号快满了」**
 *
 * 这四样都要求「把一屏文本压成一个可比较的量」，而那正是本模块做的事。
 * 要回其中任何一样 ⇒ 回 `R59` 重裁，再把 `parseUsageCapture` 接回 `account-usage.ts`。
 *
 * # 三 · 🔴 **它的绿灯从今天起证明的是什么**
 *
 * **「对那份 2026-07-31 的冻结夹具有效」，不证明「对真机 `/usage` 有效」。**
 *
 * 理由现打：那份夹具（`src/__fixtures__/usage-capture-2026-07-31.txt`）是**用户截图的逐字
 * 转录**，而本仓开发环境**不启动真实已认证的 claude 子进程**（消耗真实订阅额度、且与用户
 * 当前会话交互不可控）。退役之后**再没有真实输入流经它**，而 Claude Code 的 `/usage` 格式
 * 会继续漂 —— 它**已经漂过一次**：`Current week (Opus)` → `Current week (Fable)`，
 * 那一整块被静默丢掉。
 *
 * ⇒ **别把「测试还绿着」读成「它还能用」。**
 * ★ 同族：本区那句写死的边界 ——「看不出污染」不等于「验过了没有」。
 */
/**
 * F10（unify-launch，剩余账号 UX）：解析 `/usage` 斜杠命令的 capture-pane 抓屏文本，提取
 * Claude 订阅计划的用量窗口百分比（"plan 窗口%"）。
 *
 * ★ **2026-07-31 已真机验证并据此重写**（E42）。此前这里写着「基于训练知识回忆重建、没有经过
 * 任何真机验证」——那句是诚实的，而它预告的失败**真的发生了**：用户实测报「抓到了屏幕但认不出
 * 格式」。真机抓屏由用户以截图提供（本仓库开发环境**不**启动真实已认证的 claude 子进程，
 * 那会消耗真实订阅额度且与用户当前会话交互不可控），转录存于
 * `src/__fixtures__/usage-capture-2026-07-31.txt`，测试直接读它。
 *
 * 那次验证改掉的**不只是正则字面量，而是形状**：窗口从「硬编码 3 条枚举」改成「结构性扫描」，
 * 因为真机第三块是 `Current week (Fable)` —— 括号里是**会变的模型名**，枚举天然追不上。
 * 详见下面 `BUCKET_HEADER_RE` 的注释。
 *
 * 设计原则：格式漂移时优雅降级到 "unrecognized"，绝不 throw、绝不伪造数据。Rust 侧的探针编排
 * （`account_usage.rs`）完全不理解这里的语义，调用方（`account-usage.ts`/UI）只认这里导出的
 * 判别式类型；格式再变时，**改这一个文件就够**。
 */

export interface AccountUsageBucket {
  /** 人话标签（从原文抓到的最贴近的标签词，**不是枚举**——有多少个窗口、叫什么都会变）。 */
  label: string;
  /** 0-100，已用百分比（不是剩余）。 */
  usedPercent: number;
  /** 重置倒计时/时刻的原始文案，原样透传、不二次结构化解析（措辞变体太多，结构化本身是新的脆弱点）。 */
  resetIn?: string;
}

export type AccountUsageParseResult =
  | { status: "ok"; buckets: AccountUsageBucket[] }
  | { status: "unrecognized"; reason: string; raw?: string }
  | { status: "not-logged-in"; raw?: string }
  | { status: "cli-missing"; raw?: string };

// ---- 解析模式（★ 对照 2026-07-31 真机抓屏夹具，见文件头注） ----

/**
 * ★ 2026-07-31 真机验证后重写（本文件头注要求的那次验证，用户提供了真实抓屏）。
 *
 * **真实形态**（`src/__fixtures__/usage-capture-2026-07-31.txt`，逐字转录）：
 * ```
 * Current session
 * ███████░░░░  12% used
 * Resets 2:20am (America/Los_Angeles)
 *
 * Current week (all models)
 * ████████░░░░  59% used
 * Resets Jul 31, 10pm (America/Los_Angeles)
 *
 * Current week (Fable)          ← ★ 括号里是**模型名**，会变
 * ██░░░░░░░░░░  8% used
 * Resets Jul 31, 9:59pm (America/Los_Angeles)
 * ```
 *
 * **旧版为什么错**：它把三个窗口**硬编码成枚举**，第三条写死 `/current\s+week.*opus/`。
 * 而真机第三块是 `Current week (Fable)` ⇒ **那一整块被静默丢掉**（实测：旧版对这份真实
 * 输出返回 `ok` 但只有 2 个 bucket，用户看不出少了一个）。
 * 而且本文件 `AccountUsageBucket.label` 的注释**本来就写着**「从原文抓到的最贴近的标签词，
 * **不是枚举** —— 有多少个窗口、叫什么都不确定」。旧版没照那句做。
 *
 * ⇒ 改成**结构性识别**：凡是「独占一行的 `Current session` / `Current week (…)`」都算一个窗口，
 * 标签从原文取。以后 Anthropic 加一个窗口、改一个模型名，这里都不用动。
 */
const BUCKET_HEADER_RE = /^\s*(Current\s+(?:session|week)(?:\s*\([^)]*\))?)\s*$/i;

/** 把原文标签译成人话，**括号里的模型名原样保留**（它是会变的那部分）。 */
function displayLabel(rawHeader: string): string {
  const m = /^\s*Current\s+(session|week)\s*(?:\(([^)]*)\))?\s*$/i.exec(rawHeader);
  if (!m) return rawHeader.trim();
  const base = m[1].toLowerCase() === "session" ? "会话窗口" : "每周窗口";
  const inner = (m[2] ?? "").trim();
  if (!inner) return base;
  if (/^all\s+models$/i.test(inner)) return `${base}（全部模型）`;
  return `${base}（${inner}）`;
}

const PERCENT_RE = /(\d{1,3})\s*%/;
const RESET_RE = /reset[s]?\s*(?:at|in)?\s*:?\s*([^\n]{1,40})/i;

const CLI_MISSING_RE = /command not found|is not recognized as an internal|no such file or directory/i;
const NOT_LOGGED_IN_RE = /sign in|log in with|paste the code|console\.anthropic\.com|enter your api key/i;

/** 在标签命中行往后数几行的窗口里找百分比/重置文案（标签与数字通常相邻但不一定同一行）。 */
const BLOCK_LOOKAHEAD_LINES = 5;

/**
 * 纯函数：输入 capture-pane 抓到的屏幕文本，输出判别式结果。绝不 throw——任何解析不出的
 * 情况都落 "unrecognized"，不是异常。
 */
export function parseUsageCapture(raw: string): AccountUsageParseResult {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { status: "unrecognized", reason: "抓到的屏幕内容为空（可能探测超时或会话未就绪）" };
  }
  if (CLI_MISSING_RE.test(trimmed)) {
    return { status: "cli-missing", raw: trimmed };
  }
  if (NOT_LOGGED_IN_RE.test(trimmed)) {
    return { status: "not-logged-in", raw: trimmed };
  }

  const lines = trimmed.split("\n");
  const buckets: AccountUsageBucket[] = [];
  // 结构性扫描：每个「独占一行的 Current …」开一个窗口，往后数几行找 % 与 Resets。
  // **不去重、不限个数** —— 有几个就报几个（旧版硬编码 3 条，多了报不出、改名就丢）。
  for (let i = 0; i < lines.length; i++) {
    const header = BUCKET_HEADER_RE.exec(lines[i]);
    if (!header) continue;
    // **窗口遇到下一个 header 就截断**（Phase G 审计）：不截的话，某一块的 `%` 行
    // 因为渲染被截而缺席时，前瞻会一路吃到**下一块**的百分比与 Resets，
    // 产出一个张冠李戴的 bucket —— 有数字、`status:"ok"`，正是本文件明令禁止的
    // 「宁可说不知道，也不给错数字」的反面。截断后那一块直接不出现（缺就是缺）。
    const rawWindow = lines.slice(i + 1, i + 1 + BLOCK_LOOKAHEAD_LINES);
    const nextHeader = rawWindow.findIndex((l) => BUCKET_HEADER_RE.test(l));
    const windowLines = (
      nextHeader === -1 ? rawWindow : rawWindow.slice(0, nextHeader)
    ).join("\n");
    const pctMatch = PERCENT_RE.exec(windowLines);
    if (!pctMatch) continue;
    const resetMatch = RESET_RE.exec(windowLines);
    buckets.push({
      label: displayLabel(header[1]),
      usedPercent: Number(pctMatch[1]),
      resetIn: resetMatch ? resetMatch[1].trim() : undefined,
    });
  }
  if (buckets.length > 0) {
    return { status: "ok", buckets };
  }

  // 弱兜底：没有任何标签命中，但全文本里裸有一个百分比+重置文案 → 认不出具体是哪个窗口，
  // 但至少能给用户一个数字，好过什么都不显示。
  const pctMatch = PERCENT_RE.exec(trimmed);
  const resetMatch = RESET_RE.exec(trimmed);
  if (pctMatch && resetMatch) {
    return {
      status: "ok",
      buckets: [{ label: "用量（未识别具体窗口）", usedPercent: Number(pctMatch[1]), resetIn: resetMatch[1].trim() }],
    };
  }

  return {
    status: "unrecognized",
    reason: "抓到了屏幕但认不出格式，可能是 Claude Code 版本更新导致 /usage 输出变了",
    raw: trimmed.slice(0, 500),
  };
}
