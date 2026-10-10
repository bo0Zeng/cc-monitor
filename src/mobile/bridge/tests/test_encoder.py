"""pytest 侧：编码器（SDK 消息 → 帧）在真实录制上的断言。

与 Kotlin 侧的测试吃同一份 `bridge/vectors/`，两端的解析不会悄悄分家。

跑法（纯 stdlib + pytest）::

    python3 -m pytest bridge/tests/ -q
"""

from __future__ import annotations

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
VECTORS = ROOT / "vectors"
sys.path.insert(0, str(ROOT / "tools"))

from reference_encoder import TR_MAX_CHARS, FrameEncoder, encode_stream  # noqa: E402

CASES = ["text-short", "tool-call", "thinking", "long-reply", "auth-fail", "interrupt"]


def _rows(case: str, kind: str) -> list[dict]:
    """读 golden。`kind` = 'ndjson'（SDK 流）或 'frames.ndjson'（帧）。"""
    p = VECTORS / f"{case}.{kind}"
    assert p.is_file(), (
        f"golden 缺失：{p}。它必须随 git 提交"
    )
    out = []
    for line in p.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        obj = json.loads(line)
        if "__meta__" in obj:
            continue
        out.append(obj)
    return out


def _frames(case: str) -> list[dict]:
    return [r["frame"] for r in _rows(case, "frames.ndjson")]


# ==========================================================================
# 一、编码器本身：确定性 + 零丢弃
# ==========================================================================


@pytest.mark.parametrize("case", CASES)
def test_encoder_is_deterministic(case: str) -> None:
    """重跑编码器必须逐字节复现已提交的 frames。

    改 `reference_encoder` 若改变了已有映射，这里会红；真要改就连 golden 一起重生成，让 diff 看得见。
    """
    sdk_msgs = [r["msg"] for r in _rows(case, "ndjson")]
    assert encode_stream(sdk_msgs) == _frames(case), (
        f"{case}: 编码器输出与已提交的 golden 不一致。"
        "若这是有意的映射变更，请重跑 reference_encoder.py 并把 diff 一起提交。"
    )


@pytest.mark.parametrize("case", CASES)
def test_no_frame_is_dropped(case: str) -> None:
    """每条 SDK 消息至少产出一帧。

    静默丢帧的表现是界面永远等不到 `res`、日志里什么都没有。
    这条只覆盖录到过的消息形状；构造的畸形输入见 `test_encoder_never_returns_empty_for_odd_shapes`。
    """
    enc = FrameEncoder()
    for r in _rows(case, "ndjson"):
        assert enc.feed(r["msg"]), f"{case}: SDK 消息 {r['msg'].get('__type__')} 被丢弃了（零帧产出）"


def test_encoder_never_returns_empty_for_odd_shapes() -> None:
    """用构造的畸形输入补 golden 覆盖不到的分支。

    - `UserMessage.content` 的类型是 `str | list[ContentBlock]`
    - `AssistantMessage.content` 可以是空列表
    - content 列表元素不是 dict 时不许抛异常
    """
    odd = [
        {"__type__": "UserMessage", "content": "纯字符串正文"},
        {"__type__": "UserMessage", "content": []},
        {"__type__": "UserMessage", "content": None},
        {"__type__": "UserMessage", "content": ["裸字符串元素"]},
        {"__type__": "AssistantMessage", "content": []},
        {"__type__": "AssistantMessage", "content": ["裸字符串元素"]},
        {"__type__": "SomeFutureMessageType", "whatever": 1},
        {"__type__": "SystemMessage", "subtype": "brand_new", "data": {"a": 1}},
    ]
    enc = FrameEncoder()
    for msg in odd:
        frames = enc.feed(msg)  # 不得抛
        assert frames, f"{msg['__type__']}/{msg.get('content')!r} 产出了零帧——静默丢消息"


def test_unknown_delta_subtypes_do_not_crash() -> None:
    """未知 delta 子类型必须容忍。

    已知 4 种：text_delta / input_json_delta / thinking_delta / signature_delta，
    后两种没有 `text` 字段。再加一个虚构的子类型，守住以后新增的也不崩。
    """
    enc = FrameEncoder()
    for dt in ("text_delta", "input_json_delta", "thinking_delta", "signature_delta", "totally_new"):
        msg = {
            "__type__": "StreamEvent",
            "event": {"type": "content_block_delta", "index": 0, "delta": {"type": dt}},
        }
        assert enc.feed(msg), f"{dt} 应产出帧而非被丢弃"


# ==========================================================================
# 二、块归属可构造 + 全文覆盖成立
# ==========================================================================


@pytest.mark.parametrize("case", CASES)
def test_every_full_text_block_carries_a_constructible_key(case: str) -> None:
    """块的键 `m#i` 必须能从帧本身构造出来。

    `i` 每条消息从 0 重来（tool-call 一轮里 `i=0` 出现两次，指两个不同的块），
    没有 `m` 就不知道全文帧该覆盖哪个块。
    """
    for fr in _frames(case):
        if fr["t"] in ("at", "tt", "tu", "bs", "d", "td", "ij", "be"):
            assert fr.get("m"), f"{case}: {fr['t']} 帧缺 m（message_id），块归属无法构造"
            assert fr.get("i") is not None, f"{case}: {fr['t']} 帧缺 i"


@pytest.mark.parametrize("case", CASES)
def test_full_text_blocks_align_with_block_starts(case: str) -> None:
    """每个 `at`/`tt`/`tu` 的 `m#i` 必须对得上某个 `bs` 的 `m#i`，否则全文帧不知道该覆盖谁。

    例外是 `auth-fail`：认证失败时没有流（零个 `bs`），`AssistantMessage` 直接带 error 到达。
    """
    starts = {(f.get("m"), f.get("i")) for f in _frames(case) if f["t"] == "bs"}
    if not starts:
        assert case == "auth-fail", f"{case} 竟然一个 bs 都没有"
        return
    for fr in _frames(case):
        if fr["t"] in ("at", "tt", "tu"):
            assert (fr["m"], fr["i"]) in starts, f"{case}: {fr['t']} 帧 {fr['m']}#{fr['i']} 找不到对应 bs"


@pytest.mark.parametrize("case", ["text-short", "tool-call", "thinking", "long-reply", "interrupt"])
def test_delta_concat_equals_full_text(case: str) -> None:
    """增量拼接与 `at`/`tt` 全文逐字相等，包括 interrupt。

    「增量丢了不用补、全文无条件覆盖」靠的就是这条。它只吃仓内 golden，换机器、CI 上照样跑；
    依赖磁盘会话记录的那条是 `test_stream_matches_jsonl_for_recorded_sessions`。
    """
    deltas: dict[tuple, list[str]] = defaultdict(list)
    full: dict[tuple, str] = {}
    for fr in _frames(case):
        key = (fr.get("m"), fr.get("i"))
        if fr["t"] in ("d", "td", "ij"):
            deltas[key].append(fr["x"])
        elif fr["t"] in ("at", "tt"):
            full[key] = fr["x"]

    compared = 0
    for key, text in full.items():
        if key not in deltas:
            continue  # 没有 delta 的块（如 display=omitted 的空 thinking）不比
        compared += 1
        joined = "".join(deltas[key])
        assert joined == text, (
            f"{case}: 块 {key[0]}#{key[1]} 的 delta 拼接({len(joined)} 字符)"
            f" ≠ 全文({len(text)} 字符)。全文覆盖增量的前提不成立"
        )
    assert compared, f"{case}: 一个块都没比到，测试形同虚设"


def test_interrupt_leaves_unclosed_block_but_still_closes_with_res() -> None:
    """中断会留下永不收口的块（`bs` 多于 `be`），但仍以 `res` 收口。

    装配端必须在 `res` 到达时封口所有开着的块，否则界面上永远挂着一个转圈的块。
    断言的是「确实会这样」，别当 bug 修掉。
    """
    types = [f["t"] for f in _frames("interrupt")]
    assert types.count("bs") > types.count("be"), "中断应留下未封口的块"
    assert types[-1] == "res", "但必须以 res 收口"
    res = _frames("interrupt")[-1]
    assert res["ok"] is False and res["why"] == "aborted_streaming"


# ==========================================================================
# 三、字段级纪律
# ==========================================================================


def test_result_ok_uses_is_error_not_subtype() -> None:
    """`res.ok` 取 `not is_error`，不看 `subtype`。

    未登录时 `ResultMessage.subtype` 仍是 `'success'`，看 subtype 会把认证失败当成功。
    """
    (frame,) = FrameEncoder().feed(
        {"__type__": "ResultMessage", "subtype": "success", "is_error": True, "session_id": "s1"}
    )
    assert frame["t"] == "res"
    assert frame["ok"] is False, "subtype='success' 但 is_error=True 时，ok 必须为 false"


def test_tool_result_uses_ok_with_same_polarity_as_res() -> None:
    """`tr.ok` 与 `res.ok` 同名同极性。

    两个同义字段极性相反时，读的人必然搞混，工具失败会被读成成功。
    """
    enc = FrameEncoder()
    (fail,) = enc.feed(
        {
            "__type__": "UserMessage",
            "content": [{"__type__": "ToolResultBlock", "tool_use_id": "t1", "is_error": True, "content": "boom"}],
        }
    )
    (good,) = enc.feed(
        {
            "__type__": "UserMessage",
            "content": [{"__type__": "ToolResultBlock", "tool_use_id": "t2", "content": "fine"}],
        }
    )
    assert fail["ok"] is False and good["ok"] is True
    assert "err" not in fail, "工具结果不许带反极性的 err 字段"


def test_tool_result_is_truncated_at_the_documented_threshold() -> None:
    """`tr.x` 超过 4096 字符截断，并带 `trunc.n`（全文长度）。

    一轮的体积大头是 tool_result，不截就全推给手机。录到的 case 没有超过阈值的工具结果，只能构造输入。
    """
    big = "x" * (TR_MAX_CHARS + 500)
    (fr,) = FrameEncoder().feed(
        {
            "__type__": "UserMessage",
            "content": [{"__type__": "ToolResultBlock", "tool_use_id": "t1", "content": big}],
        }
    )
    assert len(fr["x"]) == TR_MAX_CHARS
    assert fr["trunc"]["n"] == len(big), "截断后必须告知全文长度，否则手机不知道有没有省略"


def test_result_keeps_permission_denials_and_usage() -> None:
    """`res` 不许只挑几个字段：没被特化的字段进 `raw`。

    `permission_denials` 是工具被拒的唯一数据落点。
    """
    (fr,) = FrameEncoder().feed(
        {
            "__type__": "ResultMessage",
            "is_error": False,
            "session_id": "s1",
            "duration_ms": 1234,
            "usage": {"input_tokens": 10},
            "permission_denials": [{"tool_name": "Bash"}],
            "structured_output": {"z": 1},
        }
    )
    assert fr["dur"] == 1234 and fr["usage"] == {"input_tokens": 10}
    assert fr["den"] == [{"tool_name": "Bash"}]
    assert fr["raw"]["structured_output"] == {"z": 1}, "未被吸收的字段必须进 raw，不许静默丢"


def test_catalog_lives_in_init_frame() -> None:
    """命令清单在 `init` 帧里：命令、skill、工具、MCP、agent、plugin。

    剩余字段（例如 `capabilities`）留在 `raw`。
    """
    init = next(f for f in _frames("text-short") if f["t"] == "init")
    for k in ("cmds", "skills", "tools", "mcp", "agents", "plugins"):
        assert isinstance(init[k], list), f"init 帧缺 catalog 字段 {k}"
    assert init["cmds"] and init["tools"]
    assert "capabilities" in init["raw"], "init 的剩余字段必须留在 raw"


def test_subagent_output_is_distinguishable() -> None:
    """子 agent（Task 工具）的正文必须能与主对话分开。

    `parent_tool_use_id` 是唯一标识，丢了它子 agent 的输出会整段插进主对话流。
    录到的 case 都没有 Task 子 agent，只能构造输入。
    """
    (fr,) = FrameEncoder().feed(
        {
            "__type__": "AssistantMessage",
            "message_id": "msg_1",
            "parent_tool_use_id": "toolu_parent",
            "content": [{"__type__": "TextBlock", "text": "子 agent 的话"}],
        }
    )
    assert fr["p"] == "toolu_parent"


def test_streaming_deltas_carry_subagent_ownership() -> None:
    """五个流式帧（`bs`/`d`/`td`/`ij`/`be`）也带 `p`，不只是全文帧。

    `parent_tool_use_id` 在每条 stream 行的外层，与 `event` 同级。流式帧不带 `p` 时，
    子 agent 的增量按主流块路由，主块已显示的正文会被清掉重来。
    golden 里没有子 agent，只能构造输入；形状照抄 SDK 转储的外层键集
    `(__type__, event, parent_tool_use_id, session_id, uuid)`。CLI 侧键名不同，`p` 同样在外层。
    """
    enc = FrameEncoder()

    def line(event: dict) -> dict:
        return {"__type__": "StreamEvent", "parent_tool_use_id": "toolu_sub", "event": event}

    enc.feed(line({"type": "message_start", "message": {"id": "m1"}}))
    got = {}
    for ev in (
        {"type": "content_block_start", "index": 0, "content_block": {"type": "text"}},
        {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "x"}},
        {"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "y"}},
        {"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{"}},
        {"type": "content_block_stop", "index": 0},
    ):
        (fr,) = enc.feed(line(ev))
        got[fr["t"]] = fr.get("p")
    assert got == {"bs": "toolu_sub", "d": "toolu_sub", "td": "toolu_sub", "ij": "toolu_sub", "be": "toolu_sub"}, got


def test_a_subagent_message_start_does_not_steal_the_main_flow_message_id() -> None:
    """当前 message_id 按 `p` 分桶。

    共用一个字段时这个交错序列会出错：主流 message_start(m-main) → 子 agent message_start(m-sub)
    → 主流增量被标成 m-sub，上层当成换了消息，把主块已显示的正文清掉重来。
    判据落在后果上：主流那帧仍属于 m-main。
    """
    enc = FrameEncoder()

    def start(mid: str, p: str | None) -> dict:
        return {
            "__type__": "StreamEvent",
            "parent_tool_use_id": p,
            "event": {"type": "message_start", "message": {"id": mid}},
        }

    def delta(p: str | None) -> dict:
        return {
            "__type__": "StreamEvent",
            "parent_tool_use_id": p,
            "event": {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "x"},
            },
        }

    enc.feed(start("m-main", None))
    (before,) = enc.feed(delta(None))
    assert before["m"] == "m-main", "前提：没有子 agent 掺和时本就该是 m-main"

    enc.feed(start("m-sub", "toolu_x"))  # 子 agent 插进来

    (after,) = enc.feed(delta(None))
    assert after["m"] == "m-main", f"主流的 m 被子 agent 冲掉了：{after['m']}"
    (sub,) = enc.feed(delta("toolu_x"))
    assert sub["m"] == "m-sub", "子 agent 那侧也要记对，否则等于把两条流合成一条"


def test_thinking_text_is_summarized_not_empty() -> None:
    """thinking 内容最多是摘要，不开 `display` 就一个字都没有。

    `ThinkingConfig.display` 只有 summarized / omitted，默认 omitted；光靠 prompt 措辞也不会触发 extended thinking。
    """
    td = [f for f in _frames("thinking") if f["t"] == "td"]
    assert td, "thinking case 应有 thinking_delta"
    assert sum(len(f.get("x", "")) for f in td) > 0, f"thinking 内容为空 ⇒ display 退回了 omitted"


# ==========================================================================
# 四、文档与编码器对账：解析 PROTOCOL.md
# ==========================================================================


def test_protocol_doc_matches_encoder() -> None:
    """`PROTOCOL.md` 的帧字段块必须与编码器在 golden 上的实际产出一致。

    文档里维护一个机器可读的字段块，这里解析它并与 golden 里出现过的字段对账，
    改了编码器忘了改文档就会红。
    """
    doc = (ROOT / "PROTOCOL.md").read_text(encoding="utf-8")
    m = re.search(r"```frame-fields\n(.*?)```", doc, re.S)
    assert m, "PROTOCOL.md 里找不到 ```frame-fields 块，文档对账失效"

    documented = {}
    for line in m.group(1).splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        tag, _, fields = line.partition(":")
        documented[tag.strip()] = set(fields.split())

    actual: dict[str, set[str]] = defaultdict(set)
    for case in CASES:
        for fr in _frames(case):
            actual[fr["t"]] |= set(fr.keys()) - {"t"}

    assert set(documented) == set(actual), (
        f"帧型名单对不上：文档有而 golden 无 {set(documented) - set(actual)}；"
        f"golden 有而文档无 {set(actual) - set(documented)}"
    )
    for tag in sorted(actual):
        assert documented[tag] == actual[tag], (
            f"帧 `{tag}` 字段对不上：文档 {sorted(documented[tag])} vs 实际 {sorted(actual[tag])}"
        )


# ==========================================================================
# 五、流与磁盘会话记录对账（依赖本机会话文件，缺了自动跳过）
# ==========================================================================


def test_stream_matches_jsonl_for_recorded_sessions() -> None:
    """流里的助手文本与磁盘会话记录逐字一致：磁盘记录是正文的权威副本，断线恢复靠它。

    注意：这条依赖本机 `$CLAUDE_CONFIG_DIR` 下还留着那些会话文件，清理后自动跳过，
    不能当唯一防线。自包含的那条是 `test_delta_concat_equals_full_text`。
    """
    from cross_validate import validate  # noqa: PLC0415

    checked = 0
    for case in CASES:
        r = validate(case)
        if "skip" in r:
            continue
        checked += 1
        assert r["text_identical"], (
            f"{case}: 流式 {r['stream_chars']} 字符 ≠ JSONL {r['jsonl_chars']} 字符。"
            "靠磁盘记录补齐的前提不成立，必须查清再继续。"
        )
    if checked == 0:
        pytest.skip("本机已无对应会话文件，无法对账（不视为失败）")


# ──────────────────────────────────────────────────────────────────────────
# demo-debug：vectors 里唯一被编辑过的文件，规矩与技术 golden 相反
# ──────────────────────────────────────────────────────────────────────────
# 技术 golden 逐字节忠实、绝不编辑；演示素材要随 app 分发给用户，必须脱敏。
# 目的不同规则就不同 —— 混成一条要么污染 golden，要么泄露隐私。
# 下面三条由测试守住这个区分。


def _demo_rows():
    p = VECTORS / "demo-debug.frames.ndjson"
    assert p.is_file(), "演示素材缺失"
    return [json.loads(x) for x in p.read_text(encoding="utf-8").splitlines() if x.strip()]


def test_demo_vector_is_excluded_from_golden_assertions():
    """被编辑过的文件不许进逐字节断言：那等于断言自己的编辑。

    `CASES` 是显式列表，这条让「不在列表里」成为被守住的事实。
    """
    assert "demo-debug" not in CASES
    meta = _demo_rows()[0]["__meta__"]
    assert meta.get("sanitized") is True, "演示素材必须自带 sanitized 标记，否则下个人会把它当 golden"


def test_demo_vector_carries_no_recorder_identity():
    """演示素材随 app 分发，不许带录制者的身份信息。

    最容易漏的是 `ls -la` 属主列里的裸用户名：只按家目录的路径形状扫扫不到它。
    """
    blob = (VECTORS / "demo-debug.frames.ndjson").read_text(encoding="utf-8")
    for leak in ("demo-skill-a", "demo-skill-b", "demo-tools"):
        assert leak not in blob, f"演示素材泄漏了录制者身份：{leak!r}"
    owners = set(re.findall(r"[0-9]+ ([A-Za-z_][A-Za-z0-9_-]*) +[A-Za-z_][A-Za-z0-9_-]* +[0-9]+ [A-Z][a-z]{2} ", blob))
    assert owners and owners <= {"you", "root"}, f"ls -la 属主列只许是占位名：{owners!r}"
    assert not re.search(r"/home/(?!u/|you/|USER/)[A-Za-z0-9_-]+/", blob), "家目录只许是占位名"
    assert not re.search(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.(?:edu|com|cn)\b", blob.replace("@example.com", "")), "不许带邮箱"


def test_demo_vector_catalog_is_generic_not_personal():
    """演示素材 `init` 帧的命令清单只留 Claude Code 内建命令，不带录制者的个人配置。"""
    init = next(r["frame"] for r in _demo_rows() if "frame" in r and r["frame"].get("t") == "init")
    assert init["skills"] == [], "私人 skill 必须清空"
    assert len(init["cmds"]) < 20, f"命令数 {len(init['cmds'])} 仍是私人规模"
    assert set(init["cmds"]) <= {
        "clear", "compact", "config", "context", "cost", "help", "init",
        "login", "logout", "mcp", "model", "review", "status", "usage",
    }, "只许留 Claude Code 内建、人人都有的命令"
    assert not any(t.startswith("mcp__") for t in init["tools"]), "私人 MCP 工具必须清掉"
