#!/usr/bin/env python3
"""录真 SDK 消息流 → golden vectors。

- 输出目录写死在仓内 `bridge/vectors/`，录出来的东西要随 git 走。
- 每行是「到达时刻 + SDK 消息的结构化转储」，逐字段保真，不做规范化。
- 录的是 SDK 消息流；再由 `reference_encoder.py` 编成帧（`<case>.frames.ndjson`），两份都落仓。

用法
----
    <sdkvenv>/bin/python bridge/tools/record_vectors.py --case text-short

注意：会产生真实 API 调用（每个 case 一次极短对话）。
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
import time
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any

# 输出目录写死在仓内，见模块 docstring。
REPO_VECTORS = Path(__file__).resolve().parent.parent / "vectors"

# 代表性对话。技术 golden 刻意极短：要覆盖的是帧型，不是内容。
CASES: dict[str, dict[str, Any]] = {
    "text-short": {
        "prompt": "Reply with exactly: hello",
        "why": "最小纯文本回复。验 delta / assistant / result 的基本形状。",
    },
    "tool-call": {
        "prompt": "Run the bash command `echo aterm-golden` and tell me its output.",
        "why": "带工具调用。验 tool_use / tool_result / PreToolUse hook 通报。",
    },
    "thinking": {
        "prompt": "Think carefully about why rivers meander, then answer in one sentence.",
        # 光靠 prompt 措辞不会触发 extended thinking，要显式开 `thinking` 配置。
        # `display` 只有 'summarized' / 'omitted' 两个值，不给就是 omitted：
        # thinking_delta 照来，但 `thinking` 是空串，只带 estimated_tokens 与 signature。
        "options": {"thinking": {"type": "enabled", "budget_tokens": 2000, "display": "summarized"}},
        "why": "带 thinking。验 thinking_delta 与 signature_delta（非文本 delta，"
        "bridge 不容忍就是 delta['text'] KeyError）。",
    },
    "auth-fail": {
        "prompt": "hi",
        # 用一个空的 CLAUDE_CONFIG_DIR 制造未登录态，不碰真凭据。
        "env_empty_config": True,
        "why": "认证失败路径。验 AssistantMessage.error='authentication_failed'，"
        "且 **ResultMessage.subtype 仍是 'success'** —— 必须看 is_error 不能看 subtype。",
    },
    "interrupt": {
        "prompt": "Count slowly from 1 to 200, one number per line, with a short comment on each.",
        # 让它长篇输出，收到 N 个 text_delta 后调 client.interrupt()。
        # 验 terminal_reason='aborted_streaming'（「停止生成」靠它）。
        "interrupt_after_deltas": 5,
        "why": "中断路径。验 interrupt() 生效、res.why=aborted_streaming、以及**中断后仍有收口帧**"
        "（UI 不能永远转圈）。",
    },
    "demo-debug": {
        "prompt": "跑测试发现 total_value 算出来不对，帮我看看是怎么回事。",
        # 演示素材，与其余 case 目的不同：给没有服务器的人看 app 长什么样，所以要像真活儿。
        # 在 /tmp/aterm-demo 这个自足的小项目里录（`total_value` 的 `range(len(items)-1)`
        # 少算最后一件），内容与本机无关，脱敏后随 app 分发。
        "cwd": "/tmp/aterm-demo",
        # 演示要有结论：默认 max_turns=2 会停在「我先看看文件」。
        "max_turns": 12,
        # 演示走顺利路径：默认 permission_mode 下 Bash/Edit 都要授权，结尾会变成「需要权限」。
        # 作用域只在 /tmp/aterm-demo。
        "options": {"permission_mode": "acceptEdits"},
        "why": "演示素材（U0）。展示流式正文 + Read/Bash 工具卡 + 一个有结论的排查过程。"
        "⚠️ 与技术 golden 不同：**允许且必须脱敏后编辑**（catalog 会带出录制者的私人命令/skill）。",
    },
    "long-reply": {
        "prompt": "Explain in about 400 words why rivers meander.",
        "why": "长回复。验多段突发（**实测 83 字符/块、块间隔中位 472ms ⇒ 约 176 字符/秒**），供缓释器用虚拟时钟重放。",
    },
}


def _dump(obj: Any, depth: int = 0) -> Any:
    """把 SDK 对象转成可 JSON 化的结构，保字段名与类型信息。

    不用 `repr`：repr 不可机器消费，且跨版本不稳定。
    保 `__type__` 是为了让 Kotlin 侧能按类型断言，而不是靠猜字段。
    """
    if depth > 12:  # 防御性：SDK 对象理论上不会这么深
        return {"__truncated__": True}
    if obj is None or isinstance(obj, (bool, int, float, str)):
        return obj
    if isinstance(obj, (list, tuple)):
        return [_dump(x, depth + 1) for x in obj]
    if isinstance(obj, dict):
        return {str(k): _dump(v, depth + 1) for k, v in obj.items()}
    if is_dataclass(obj) and not isinstance(obj, type):
        out: dict[str, Any] = {"__type__": type(obj).__name__}
        for f in fields(obj):
            out[f.name] = _dump(getattr(obj, f.name), depth + 1)
        return out
    # SDK 的非 dataclass 消息类型（有 __dict__ 的普通对象）
    if hasattr(obj, "__dict__"):
        out = {"__type__": type(obj).__name__}
        for k, v in vars(obj).items():
            if not k.startswith("_"):
                out[k] = _dump(v, depth + 1)
        return out
    return {"__repr__": repr(obj), "__type__": type(obj).__name__}


async def record(case: str) -> Path:
    from claude_agent_sdk import ClaudeAgentOptions, ClaudeSDKClient

    spec = CASES[case]
    out_path = REPO_VECTORS / f"{case}.ndjson"
    REPO_VECTORS.mkdir(parents=True, exist_ok=True)

    extra: dict[str, Any] = dict(spec.get("options") or {})
    if spec.get("cwd"):
        extra["cwd"] = spec["cwd"]  # 演示素材在自足的小项目里录
    if spec.get("env_empty_config"):
        empty = REPO_VECTORS.parent / ".tmp-emptycfg"
        empty.mkdir(parents=True, exist_ok=True)
        extra["env"] = {"CLAUDE_CONFIG_DIR": str(empty)}

    options = ClaudeAgentOptions(
        # 开增量流（stream_event）。
        include_partial_messages=True,
        # 不传 setting_sources=[]：传了会让 CLAUDE.md / settings / skills / 自定义命令全部失效，
        # 命令清单全靠它们。留默认 = CLI 全加载。
        permission_mode=extra.pop("permission_mode", "default"),
        max_turns=spec.get("max_turns", 2),
        **extra,
    )

    lines: list[str] = []
    t0 = time.monotonic_ns()
    n_deltas = 0
    interrupt_at = spec.get("interrupt_after_deltas")
    session_id: str | None = None

    client = ClaudeSDKClient(options)
    # 别用 `async with` 再 connect：__aenter__ 已经 connect 过，
    # 第二次 connect 会泄漏第一个 CLI 子进程。
    await client.connect()
    try:
        await client.query(spec["prompt"])
        async for msg in client.receive_messages():
            lines.append(
                json.dumps(
                    {"t_ns": time.monotonic_ns() - t0, "msg": _dump(msg)},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
            )
            ty = type(msg).__name__
            # 记 session_id，供流式与磁盘记录对账时定位那份记录
            if ty == "SystemMessage" and getattr(msg, "subtype", None) == "init":
                session_id = (getattr(msg, "data", None) or {}).get("session_id")
            elif ty == "ResultMessage":
                session_id = session_id or getattr(msg, "session_id", None)

            if interrupt_at is not None and ty == "StreamEvent":
                ev = getattr(msg, "event", None) or {}
                if (ev.get("delta") or {}).get("type") == "text_delta":
                    n_deltas += 1
                    if n_deltas == interrupt_at:
                        await client.interrupt()

            if ty == "ResultMessage":
                break
    finally:
        await client.disconnect()

    header = json.dumps(
        {
            "__meta__": {
                "case": case,
                "why": spec["why"],
                "prompt": spec["prompt"],
                "recorded_by": "bridge/tools/record_vectors.py (G0)",
                "session_id": session_id,
                "note": "t_ns = 相对首帧的单调纳秒；msg = SDK 消息的结构化转储，保 __type__。"
                "session_id 供「流式 ↔ JSONL 对账」定位磁盘权威副本。",
            }
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
    out_path.write_text(header + "\n" + "\n".join(lines) + "\n", encoding="utf-8")
    return out_path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--case", choices=sorted(CASES), required=True)
    args = ap.parse_args()

    path = asyncio.run(record(args.case))
    n = sum(1 for _ in path.open(encoding="utf-8")) - 1  # 减掉 __meta__ 行
    print(f"✔ {args.case}: {n} 帧 → {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
