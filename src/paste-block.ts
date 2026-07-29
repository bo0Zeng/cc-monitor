// T03：「这段文本你得自己贴进某个配置文件」——统一的输出面 + 复制按钮 + 三句话。
//
// ## 为什么是三个消费者，而不是计划里写的两个
//
// MASTERPLAN 与 STATUS 都写「两个真实消费者」。实际数下来族 A 有 **3 处**
// （`launcher-diagnostics.ts` 的别名生成器、`cc-bus-hooks-section.ts` 的钩子片段、
// `remote-section.ts` 的 `CCM_WRAPPER_SNIPPET`），而第三处**行为和另两处不一致**：
// 它没有粘后指引，而且**复制失败时把错误吞进 `console.warn`**——用户点了「复制」，
// 按钮不变、没有任何提示，然后去粘贴，粘到的是上一次剪贴板里的东西。
// 这不是风格不一致，是一个真缺陷，且**只有把三处放到一起数才看得见**。
//
// ## 抽的是槽位，不是内容
//
// 三处真正共有的只有「输出面 + 复制按钮 + 校验门 + 三句话」。
// 变体选择器（7 个表单控件 / 2 选项 select / 没有）和文本生成（TS / Rust / `?raw` import）
// **各不相同，不上提**——上提就是把三件不相干的事装进一个盒子。
//
// `mergeNote` 刻意是**自由文本而不是枚举**：按枚举设计数下来是 `Append`（2 个用户）+
// `MergeIntoKey`（**只有 1 个用户**），后者不够本工作区的 ≥2 判据；而钩子那句
// 「**合并**，不是整份覆盖——那里可能还有别的工具的钩子」是 load-bearing 的，
// 换成通用措辞就丢信息，套到 `.bashrc` 上又是错的。
// **共享的是「必须说一句合并语义」这个槽，不是那句话本身。**（同 T01 拒绝「探测机制」：
// 机制留各家，位置统一。）
//
// ## 原先这里有个 `warning` 槽，T03 审计后**移回消费者自己那儿**
//
// 逐字段数消费者：`text` / 三句话 3 个、`invalidReason` 2 个、`multiline` 2 个、
// **`warning` 只有 1 个**（钩子片段那一处）。而同一个 commit 里我用"只有 1 个用户"
// 否掉了 `MergeIntoKey` 枚举变体——**尺子不能一边松一边紧**。
// 它已经搬去 `cc-bus-hooks-section.ts` 自己渲染（那里也才是它该被测试钉住的地方：
// 审计实测删掉 A2 的 warning 接线，56 项全绿，而我在 commit message 里写的
// 「UI 侧另有测试钉住它真的上屏」**是假的**）。
//
// ## 本文件没有、也不得有任何写入路径
//
// 只产出待贴文本 + 复制到剪贴板。写用户的 `~/.bashrc` / `~/.claude/settings.json`
// 是用户明确定过调的红线，有测试守着。
import { showActionFailureToast } from "./error-toast";

export interface PasteSpec {
  /** 待贴文本。**实时求值**——别名随表单变、钩子随形态选择变、wrapper 恒定。 */
  text: () => string;
  /** 贴到哪。三个消费者都必须说。 */
  target: string;
  /** 怎么合并（追加一个函数 / 合并进某个键 / …）。三个消费者都必须说，各写自己的。 */
  mergeNote: string;
  /** 怎样才生效（source / 开新终端 / 新开会话）。三个消费者都必须说。 */
  activation: string;
  /** 校验门：返回非 `null` 表示这段文本**还不能贴**，理由给用户看。2/3 个消费者需要。 */
  invalidReason?: (text: string) => string | null;
  /** 多行输出用 `textarea`，单行用 `input`。 */
  multiline?: boolean;
  /** 输出面行数（仅 `multiline`）。 */
  rows?: number;
  /** 附加 class，便于各消费者保留自己原有的样式钩子。 */
  className?: string;
}

/** 一个待贴块的操作句柄——消费者在自己的表单变化时调 `refresh()`。 */
export interface PasteBlock {
  element: HTMLElement;
  /** 重新求值 `text()` / `warning()` 并刷新输出面。 */
  refresh: () => void;
  /** 当前输出面上的文本（测试与消费者都可能要读）。 */
  value: () => string;
}

/**
 * 三句话缺任一就**抛错**。允许为空就等于允许退化回 `remote-section.ts` 那个形态
 * ——而那正是这个组件存在的理由。
 */
function requireThreeSentences(spec: PasteSpec): void {
  for (const [k, v] of [
    ["target", spec.target],
    ["mergeNote", spec.mergeNote],
    ["activation", spec.activation],
  ] as const) {
    if (!v || !v.trim()) {
      throw new Error(
        `PasteSpec.${k} 不能为空——「贴到哪 / 怎么合并 / 怎样才生效」三句话是这个组件存在的理由`,
      );
    }
  }
}

export function buildPasteBlock(spec: PasteSpec): PasteBlock {
  requireThreeSentences(spec);

  const root = document.createElement("div");
  root.className = `paste-block${spec.className ? ` ${spec.className}` : ""}`;

  const out: HTMLInputElement | HTMLTextAreaElement = spec.multiline
    ? document.createElement("textarea")
    : document.createElement("input");
  out.readOnly = true;
  out.className = "settings-input paste-block-out";
  if (spec.multiline) (out as HTMLTextAreaElement).rows = spec.rows ?? 8;
  else (out as HTMLInputElement).type = "text";
  root.appendChild(out);

  // 三句话**必须上屏**，不是只在 toast 里出现一次就算说过了
  // （T02 教训：纯函数被断言 ≠ 它上了屏——两个核心列删掉，15 条测试全绿）。
  const where = document.createElement("div");
  where.className = "paste-block-target";
  where.textContent = `贴到：${spec.target}`;
  root.appendChild(where);

  const merge = document.createElement("div");
  merge.className = "paste-block-merge";
  merge.textContent = spec.mergeNote;
  root.appendChild(merge);

  const act = document.createElement("div");
  act.className = "paste-block-activation";
  act.textContent = `生效条件：${spec.activation}`;
  root.appendChild(act);

  const btnRow = document.createElement("div");
  btnRow.className = "settings-row settings-row-end paste-block-actions";
  const copyBtn = document.createElement("button");
  copyBtn.type = "button";
  copyBtn.className = "settings-btn settings-btn-secondary paste-block-copy";
  copyBtn.textContent = "复制";
  btnRow.appendChild(copyBtn);
  root.appendChild(btnRow);

  const refresh = (): void => {
    out.value = spec.text();
  };

  copyBtn.addEventListener("click", () => {
    const v = out.value;
    const bad = spec.invalidReason?.(v) ?? null;
    if (bad !== null) {
      // **拒绝时绝不碰剪贴板**：把中文提示或半成品写进剪贴板，用户粘出去就是坏配置。
      showActionFailureToast("还不能贴", bad, {
        level: "info",
        durationMs: 4000,
      });
      return;
    }
    const clip = navigator.clipboard;
    if (!clip) {
      showActionFailureToast("复制失败", "剪贴板不可用，手动选中复制。", {
        level: "error",
      });
      return;
    }
    void clip.writeText(v).then(
      () =>
        showActionFailureToast(
          "已复制",
          `贴到 ${spec.target}。${spec.mergeNote} 生效条件：${spec.activation}`,
          { level: "info", durationMs: 6000 },
        ),
      // **不许吞进 console**（这就是 A3 的缺陷）——用户点了按钮就得知道结果。
      () =>
        showActionFailureToast("复制失败", "剪贴板不可用，手动选中复制。", {
          level: "error",
        }),
    );
  });

  refresh();
  return { element: root, refresh, value: () => out.value };
}
