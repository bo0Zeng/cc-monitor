#!/usr/bin/env python3
"""参考编码器：SDK 消息 → aterm 帧。

帧字段的可执行规格。`bridge/PROTOCOL.md` 的帧字段块由 `test_protocol_doc_matches_encoder`
与本编码器的产物对账；两者冲突时以本文件为准，并修文档。手机侧的 `CliFrameEncoder`
吃的是 CLI 的 stream-json，信封类消息拆包不同，产出的帧相同。

数据流
------
    record_vectors.py  →  vectors/<case>.ndjson         (SDK 消息流，真实录制)
                                  │
                          reference_encoder.py
                                  ▼
                          vectors/<case>.frames.ndjson  (帧，golden)
                                  │
                    ┌─────────────┴─────────────┐
              pytest 侧断言               Kotlin 侧测试
                                     （同一份数据）

有状态
------
message_id 只在 `message_start` 里给一次，后续的 `bs`/`d`/`be` 只有 `index`，
而 `index` 每条消息从 0 重来。块的键是 `m#i`，所以编码器要记住当前 message_id。

取舍
----
1. 未知帧型不丢弃，落 `ev` 透传。静默丢帧的表现是界面永远等不到 `res`、日志里什么都没有。
2. 未知 delta 子类型要容忍。`thinking_delta` / `signature_delta` 没有 `text` 字段。
3. `ok` 一律取 `not is_error`，不看 `subtype`：未登录时 `subtype` 仍是 'success'。
   `tr` 与 `res` 用同一个字段名、同一个极性。
4. 每个特化帧带 `raw`，放没被特化字段吸收的原始字段，不丢语义。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

VECTORS = Path(__file__).resolve().parent.parent / "vectors"

PROTO_VERSION = 1

# `tr.x` 截断阈值。一轮的体积大头是 tool_result，不是增量。
# 与 `CliFrameEncoder.TR_MAX_CHARS` 同值，改一个要改另一个。
TR_MAX_CHARS = 4096


def _fr(t: str, **kw: Any) -> dict[str, Any]:
    """构造一帧，丢掉值为 None 的键。

    `t` 用短名、丢 None 键，都是为了压信封开销：帧很碎，键名占的比例很高。
    """
    return {"t": t, **{k: v for k, v in kw.items() if v is not None}}


def _rest(d: dict[str, Any], *taken: str) -> dict[str, Any] | None:
    """取出未被特化字段吸收的剩余部分，供 `raw` 兜底。全被吸收则返回 None（配合 drop-none）。"""
    rest = {k: v for k, v in d.items() if k not in taken and k != "__type__"}
    return rest or None


class FrameEncoder:
    """把 SDK 消息流映射成帧流。有状态（见模块 docstring）。

    用法::

        enc = FrameEncoder()
        frames = [f for msg in stream for f in enc.feed(msg)]
    """

    def __init__(self) -> None:
        # 当前 message_id，由 `message_start` 设置，供 bs/d/td/ij/be 标注归属。
        # 按 `parent_tool_use_id` 分桶（None = 主流，非 None = 对应子 agent）：
        # 共用一个字段时，子 agent 的 `message_start` 会冲掉主流的 message_id，
        # 主流随后的增量被标上子 agent 的 `m`，下游当成换了消息，把已显示的正文清掉重来。
        self._m_by_p: dict[str | None, str | None] = {}
        # 每个 message_id 已由 AssistantMessage 交付过的块数（见 `_block_index`）。
        self._delivered: dict[str, int] = {}

    # -- 各消息类型 ---------------------------------------------------------

    def _system(self, msg: dict[str, Any]) -> list[dict[str, Any]]:
        sub = msg.get("subtype")
        data = msg.get("data") or {}
        if sub != "init":
            # 其余 subtype（例如 'status'）不特化，走 ev 透传。
            return [_fr("ev", k=f"system:{sub}", raw=data)]
        # 命令清单：命令、skill、工具、agent、MCP 都在这里。
        taken = (
            "session_id", "model", "cwd", "slash_commands", "skills",
            "tools", "mcp_servers", "agents", "plugins",
        )
        return [
            _fr(
                "init",
                sid=data.get("session_id"),
                model=data.get("model"),
                cwd=data.get("cwd"),
                cmds=data.get("slash_commands") or [],
                skills=data.get("skills") or [],
                tools=data.get("tools") or [],
                mcp=data.get("mcp_servers") or [],
                agents=data.get("agents") or [],
                plugins=data.get("plugins") or [],
                # init 之外拿不到这些字段（例如 claude_code_version），剩余字段全留着。
                raw=_rest(data, *taken),
            )
        ]

    def _stream(self, msg: dict[str, Any]) -> list[dict[str, Any]]:
        e = msg.get("event") or {}
        et = e.get("type")
        # 子 agent（Task）的归属在外层行上，与 `event` 同级。主流里它是 None，
        # `_fr` 丢 None 键，所以主流的帧里不出现 `p`。
        p = msg.get("parent_tool_use_id")

        if et == "message_start":
            # 唯一能拿到 message_id 的地方。
            self._m_by_p[p] = ((e.get("message") or {}).get("id")) or None
            return [_fr("ev", k="stream:message_start", raw=e)]

        if et == "content_block_start":
            blk = e.get("content_block") or {}
            return [
                _fr(
                    "bs",
                    m=self._m_by_p.get(p),
                    p=p,
                    i=e.get("index"),
                    bt=blk.get("type"),
                    id=blk.get("id"),
                    name=blk.get("name"),
                )
            ]

        if et == "content_block_delta":
            d = e.get("delta") or {}
            dt = d.get("type")
            i = e.get("index")
            # 按子类型取值，不无条件取 d["text"]（signature_delta / thinking_delta 没有它）
            if dt == "text_delta":
                return [_fr("d", m=self._m_by_p.get(p), p=p, i=i, x=d.get("text", ""))]
            if dt == "thinking_delta":
                return [_fr("td", m=self._m_by_p.get(p), p=p, i=i, x=d.get("thinking", ""))]
            if dt == "input_json_delta":
                return [_fr("ij", m=self._m_by_p.get(p), p=p, i=i, x=d.get("partial_json", ""))]
            # signature_delta 及以后新增的：透传
            return [_fr("ev", k=f"delta:{dt}", raw=d)]

        if et == "content_block_stop":
            # 注意：`be` 不保证到达（中断时在飞的块收不到）。它只是渲染时机提示，
            # 正文以 `at`/`tt` 为准，一轮是否结束以 `res` 为准。
            return [_fr("be", m=self._m_by_p.get(p), p=p, i=e.get("index"))]

        return [_fr("ev", k=f"stream:{et}", raw=e)]

    def _block_index(self, m: str | None) -> int:
        """AssistantMessage 里那个块在流里的 `content_block` index。

        不能用 `enumerate(content)`：SDK 每个块单独发一条 AssistantMessage，`content` 长度恒为 1，
        下标永远是 0，thinking 块与 text 块会撞成同一个键。同一个 message_id 下块按顺序逐条到达，
        与流里 `bs` 的 index 0,1,2… 一一对应，所以按「这条消息已交付几个块」计数。
        """
        key = m or ""
        idx = self._delivered.get(key, 0)
        self._delivered[key] = idx + 1
        return idx

    def _assistant(self, msg: dict[str, Any]) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        # 子 agent 的正文靠它区分，丢了会整段插进主对话流。
        # 要先读它：下面校正 message_id 按它分桶（见 `_m_by_p`）。
        parent = msg.get("parent_tool_use_id")
        # AssistantMessage 自带 message_id，比流里记的可信，顺手校正。
        m = msg.get("message_id") or self._m_by_p.get(parent)
        if msg.get("message_id"):
            self._m_by_p[parent] = msg["message_id"]

        for blk in msg.get("content") or []:
            idx = self._block_index(m)
            bt = blk.get("__type__") if isinstance(blk, dict) else None
            if bt == "TextBlock":
                # m + i 指明它覆盖哪个块。
                out.append(_fr("at", m=m, i=idx, p=parent, x=blk.get("text", "")))
            elif bt == "ToolUseBlock":
                out.append(
                    _fr(
                        "tu",
                        m=m,
                        i=idx,
                        p=parent,
                        id=blk.get("id"),
                        name=blk.get("name"),
                        inp=blk.get("input"),
                    )
                )
            elif bt == "ThinkingBlock":
                out.append(_fr("tt", m=m, i=idx, p=parent, x=blk.get("thinking", "")))
            else:
                # 元素不是 dict 也不能崩。
                out.append(_fr("ev", k=f"block:{bt}", i=idx, raw=blk if isinstance(blk, dict) else {"v": blk}))

        # 认证失败在这里暴露，而 ResultMessage.subtype 仍是 'success'
        if msg.get("error"):
            out.append(_fr("err", m=m, code=msg.get("error")))
        return out

    def _user(self, msg: dict[str, Any]) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        content = msg.get("content")
        parent = msg.get("parent_tool_use_id")

        # `UserMessage.content` 的类型是 `str | list[ContentBlock]`，两种都要出帧。
        if isinstance(content, str):
            return [_fr("ut", p=parent, x=content)]
        if not isinstance(content, list):
            return [_fr("ev", k="user:unknown-content", raw={"v": repr(content)})]

        for blk in content:
            if not isinstance(blk, dict):
                out.append(_fr("ev", k="user-block:non-dict", raw={"v": repr(blk)}))
                continue
            if blk.get("__type__") == "ToolResultBlock":
                out.append(self._tool_result(blk, parent))
            else:
                out.append(_fr("ev", k=f"user-block:{blk.get('__type__')}", raw=blk))
        return out or [_fr("ev", k="user:empty", raw={})]

    def _tool_result(self, blk: dict[str, Any], parent: str | None) -> dict[str, Any]:
        content = blk.get("content")
        text = content if isinstance(content, str) else json.dumps(content, ensure_ascii=False)
        trunc = None
        if len(text) > TR_MAX_CHARS:
            trunc = {"n": len(text)}  # 全文长度
            text = text[:TR_MAX_CHARS]
        return _fr(
            "tr",
            id=blk.get("tool_use_id"),
            p=parent,
            # 与 res.ok 同名同极性。
            ok=not blk.get("is_error"),
            x=text,
            trunc=trunc,
        )

    def _result(self, msg: dict[str, Any]) -> list[dict[str, Any]]:
        taken = (
            "session_id", "is_error", "total_cost_usd", "num_turns",
            "terminal_reason", "api_error_status", "duration_ms", "usage",
            "permission_denials", "model_usage",
        )
        return [
            _fr(
                "res",
                sid=msg.get("session_id"),
                # 取 not is_error，不看 subtype（未登录时 subtype 仍是 success）
                ok=not msg.get("is_error"),
                cost=msg.get("total_cost_usd"),
                turns=msg.get("num_turns"),
                why=msg.get("terminal_reason"),
                api=msg.get("api_error_status"),
                dur=msg.get("duration_ms"),
                usage=msg.get("usage"),
                # 工具被拒的唯一数据落点。
                den=msg.get("permission_denials") or None,
                mu=msg.get("model_usage"),
                raw=_rest(msg, *taken),
            )
        ]

    # -- 分发 ---------------------------------------------------------------

    def feed(self, msg: dict[str, Any]) -> list[dict[str, Any]]:
        """把一条 SDK 消息映射成 1..N 帧。永不返回空列表。"""
        ty = msg.get("__type__")
        if ty == "SystemMessage":
            return self._system(msg)
        if ty == "StreamEvent":
            return self._stream(msg)
        if ty == "AssistantMessage":
            return self._assistant(msg) or [_fr("ev", k="assistant:empty", raw=_rest(msg) or {})]
        if ty == "UserMessage":
            return self._user(msg)
        if ty == "ResultMessage":
            return self._result(msg)
        if ty == "RateLimitEvent":
            # 正常路径上就会来，不只是被限流时
            return [_fr("rl", raw=_rest(msg) or {})]
        # HookEventMessage 及一切未知类型：透传
        return [_fr("ev", k=f"sdk:{ty}", raw=_rest(msg) or {})]


def encode_stream(msgs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """整条流一次性编码（编码器是有状态的，必须按序喂）。"""
    enc = FrameEncoder()
    return [fr for msg in msgs for fr in enc.feed(msg)]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--case", required=True)
    args = ap.parse_args()

    src = VECTORS / f"{args.case}.ndjson"
    dst = VECTORS / f"{args.case}.frames.ndjson"
    rows = [json.loads(line) for line in src.open(encoding="utf-8") if line.strip()]

    # 首行应当是 __meta__，但别硬取 rows[0]：不是的话会吞掉第一条 SDK 消息。
    meta: dict[str, Any] = {}
    sdk_rows = []
    for r in rows:
        if "__meta__" in r:
            meta = r["__meta__"]
        else:
            sdk_rows.append(r)
    if not meta:
        print(f"⚠️  {src.name} 没有 __meta__ 行（继续，但 golden 会缺来源信息）", file=sys.stderr)

    lines = [
        json.dumps(
            {
                "__meta__": {
                    **meta,
                    "encoded_by": "bridge/tools/reference_encoder.py (G0)",
                    "proto": PROTO_VERSION,
                    "note": "t_ns 沿用源；frame 是 aterm 帧。两端（pytest / Kotlin）吃同一份。",
                }
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    ]
    enc = FrameEncoder()
    n = 0
    for r in sdk_rows:
        for fr in enc.feed(r["msg"]):
            lines.append(
                json.dumps(
                    {"t_ns": r["t_ns"], "frame": fr}, ensure_ascii=False, separators=(",", ":")
                )
            )
            n += 1
    dst.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"✔ {args.case}: {len(sdk_rows)} SDK 消息 → {n} 帧 → {dst.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
