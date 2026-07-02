/**
 * 解析并渲染 user 消息里的 `!` bash 模式标记（Batch4-F16）。
 *
 * 用户在 Claude Code CLI 里键入 `!cmd` 时，CLI 写两类 user 记录到 JSONL：
 *
 *   输入：  <bash-input>npm install && npm run build</bash-input>
 *   输出：  <bash-stdout>added 35 packages...</bash-stdout><bash-stderr>...</bash-stderr>
 *
 * 输出记录 stdout/stderr 可任一为空（实测样本有 stdout 空 + stderr 非空）。
 * 标签内容是 HTML 实体转义过的（实测样本含 `&gt;`），提取后需反转义。
 *
 * 容错优先（faithful 渲染底线，同 diff 卡"异常回退原 JSON"哲学）：
 * 识别不了 → 返回 null，走普通 user 气泡原样展示，绝不吞内容。
 * 渲染全部 createElement + textContent，零 innerHTML。
 */

export interface BashInput {
  command: string;
}

export interface BashOutput {
  stdout: string;
  stderr: string;
}

/** CLI 写标签内容时转义的已知实体。单趟解码（map 驱动），避免 `&amp;lt;` 被二次解成 `<`。 */
const ENTITY_MAP: Record<string, string> = {
  lt: "<",
  gt: ">",
  quot: '"',
  "#39": "'",
  amp: "&",
};

export function unescapeEntities(s: string): string {
  return s.replace(/&(lt|gt|quot|#39|amp);/g, (_, k: string) => ENTITY_MAP[k]);
}

/** 整段内容仅为一个 `<bash-input>…</bash-input>` 时命中；否则 null 回退。 */
export function parseBashInput(text: string): BashInput | null {
  const t = text.trim();
  const open = "<bash-input>";
  const close = "</bash-input>";
  if (!t.startsWith(open) || !t.endsWith(close) || t.length <= open.length + close.length) {
    return null; // 含空命令（纯理论边界，真实 CLI 不产生）→ 回退原样
  }
  const inner = t.slice(open.length, t.length - close.length);
  if (inner.includes(close)) return null; // 畸形嵌套 → 回退原样
  const command = unescapeEntities(inner).trim();
  if (!command) return null; // 纯空白命令同样回退
  return { command };
}

/** 从 text 开头取一个 `<tag>…</tag>` 段（indexOf 定位闭标签——内容可含 `<`）。 */
function takeLeadingTag(
  text: string,
  tag: string,
): { content: string; rest: string } | null {
  const open = `<${tag}>`;
  const close = `</${tag}>`;
  if (!text.startsWith(open)) return null;
  const i = text.indexOf(close);
  if (i < 0) return null;
  return { content: text.slice(open.length, i), rest: text.slice(i + close.length) };
}

/**
 * 整段内容仅由 `<bash-stdout>` / `<bash-stderr>` 段组成时命中。
 * 段序/缺段宽容（实测总以 stdout 开头，但不赌死）；有识别不了的残余 → null 回退。
 */
export function parseBashOutput(text: string): BashOutput | null {
  let rest = text.trim();
  if (!rest.startsWith("<bash-stdout>") && !rest.startsWith("<bash-stderr>")) {
    return null;
  }
  let stdout = "";
  let stderr = "";
  while (rest.length > 0) {
    const out = takeLeadingTag(rest, "bash-stdout");
    if (out) {
      stdout += out.content;
      rest = out.rest.trim();
      continue;
    }
    const err = takeLeadingTag(rest, "bash-stderr");
    if (err) {
      stderr += err.content;
      rest = err.rest.trim();
      continue;
    }
    return null; // 残余不是这两类标签 → 整体回退
  }
  return { stdout: unescapeEntities(stdout), stderr: unescapeEntities(stderr) };
}

// ============ DOM 渲染（下面依赖 document，node 纯函数测试不要碰） ============

/** 输出 pre 超过该行数先折叠只展示头部（bash 输出通常短，阈值取小） */
const OUTPUT_COLLAPSE_LINES = 30;
const OUTPUT_HEAD_LINES = 20;

/** 终端风格命令卡：❯ npm install && npm run build */
export function buildBashInputCard(
  input: BashInput,
  timestamp: string,
  formatTime: (iso: string) => string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-bash-input";

  const prompt = document.createElement("span");
  prompt.className = "bash-prompt";
  prompt.textContent = "❯";
  card.appendChild(prompt);

  const cmd = document.createElement("code");
  cmd.className = "bash-cmd";
  cmd.textContent = input.command;
  card.appendChild(cmd);

  const ts = document.createElement("span");
  ts.className = "bash-ts";
  ts.textContent = formatTime(timestamp);
  card.appendChild(ts);

  return card;
}

/** stdout/stderr 输出卡：stderr 红色调标注，超长折叠（沿 block-body-show-full 按钮惯例）。 */
export function buildBashOutputCard(
  output: BashOutput,
  timestamp: string,
  formatTime: (iso: string) => string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-bash-output";

  const header = document.createElement("div");
  header.className = "bash-output-header";
  const icon = document.createElement("span");
  icon.className = "bash-prompt";
  icon.textContent = "▤";
  const label = document.createElement("span");
  label.className = "bash-output-label";
  label.textContent = "bash 输出";
  const ts = document.createElement("span");
  ts.className = "bash-ts";
  ts.textContent = formatTime(timestamp);
  header.append(icon, label, ts);
  card.appendChild(header);

  const stdout = output.stdout.trim();
  const stderr = output.stderr.trim();
  if (stdout) {
    card.appendChild(buildOutputPre(stdout, "bash-stdout-body"));
  }
  if (stderr) {
    const errLabel = document.createElement("div");
    errLabel.className = "bash-stderr-label";
    errLabel.textContent = "✖ stderr";
    card.appendChild(errLabel);
    card.appendChild(buildOutputPre(stderr, "bash-stderr-body"));
  }
  if (!stdout && !stderr) {
    const empty = document.createElement("div");
    empty.className = "bash-output-empty";
    empty.textContent = "（无输出）";
    card.appendChild(empty);
  }
  return card;
}

function buildOutputPre(text: string, cls: string): HTMLElement {
  const pre = document.createElement("pre");
  pre.className = `bash-output-body ${cls}`;
  const lines = text.split("\n");
  if (lines.length <= OUTPUT_COLLAPSE_LINES) {
    pre.textContent = text;
    return pre;
  }
  pre.textContent = lines.slice(0, OUTPUT_HEAD_LINES).join("\n");
  const wrap = document.createElement("div");
  wrap.className = "block-body-truncated-wrap";
  wrap.appendChild(pre);
  const expand = document.createElement("button");
  expand.type = "button";
  expand.className = "block-body-show-full";
  expand.textContent = `显示完整输出 (${lines.length} 行)`;
  expand.addEventListener(
    "click",
    () => {
      pre.textContent = text;
      expand.remove();
    },
    { once: true },
  );
  wrap.appendChild(expand);
  return wrap;
}
