/**
 * F10（unify-launch，剩余账号 UX）：解析 `/usage` 斜杠命令的 capture-pane 抓屏文本，提取
 * Claude 订阅计划的用量窗口百分比（"plan 窗口%"）。
 *
 * ★ 本文件的正则模式基于训练知识对 Claude Code CLI `/usage` 命令公开呈现形态的回忆重建，
 * **没有经过任何真机验证**——本仓库的开发环境里不应该启动一个真实已认证的 claude 子进程去
 * 测试（会消耗真实 API/订阅额度、且与当前会话交互不可控），见 `.claude/planned-build/
 * unify-launch/features/F10-remaining-account-ux.md` §0/§7。上线前必须按该文件 §7 的真机
 * 验证清单核实，并按需重写下面的 `LABEL_PATTERNS`/`PERCENT_RE`/`RESET_RE`。
 *
 * 设计原则：格式漂移时优雅降级到 "unrecognized"，绝不 throw、绝不伪造数据。这是唯一需要在
 * 真机验证后重写的文件——Rust 侧的探针编排（`account_usage.rs`）完全不理解这里的语义，
 * 调用方（`account-usage.ts`/UI）只认这里导出的判别式类型，不需要跟着改。
 */

export interface AccountUsageBucket {
  /** 人话标签（从原文抓到的最贴近的标签词，不是枚举——真机验证前有多少个窗口、叫什么都不确定）。 */
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

// ---- 训练知识猜测的正则模式（★ 非真机验证，见文件头注） ----

const LABEL_PATTERNS: { label: string; re: RegExp }[] = [
  { label: "会话窗口（约 5 小时）", re: /current\s+session|session\s+usage/i },
  { label: "每周窗口（全部模型）", re: /current\s+week(?!.*opus)|weekly\s+usage/i },
  { label: "每周窗口（Opus）", re: /current\s+week.*opus|opus.*week/i },
];

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
  for (const { label, re } of LABEL_PATTERNS) {
    const idx = lines.findIndex((l) => re.test(l));
    if (idx === -1) continue;
    const windowLines = lines.slice(idx, idx + BLOCK_LOOKAHEAD_LINES + 1).join("\n");
    const pctMatch = PERCENT_RE.exec(windowLines);
    if (!pctMatch) continue;
    const resetMatch = RESET_RE.exec(windowLines);
    buckets.push({
      label,
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
