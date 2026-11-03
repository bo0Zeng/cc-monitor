package com.ccmonitor.mobile.bench

import android.os.Bundle
import androidx.activity.ComponentActivity
import com.ccmonitor.mobile.cache.LocalCache
import com.ccmonitor.mobile.cache.RawPage
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.claude.link.wire.SyncHand
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Block
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Draw
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.ccmonitor.mobile.core.wire.gen.MdItem
import com.ccmonitor.mobile.core.wire.gen.Speaker
import com.ccmonitor.mobile.core.wire.gen.UserText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.koin.android.ext.android.get

/**
 * 只进量测那两族构建类型（`nonMinified*` 录 profile · `benchmark*` 量冷启），正式包 · 调试包 · 冒烟包都不带。
 * 把一台造的机器和一条长会话写进本地库：冷启落在那台的新建会话（主屏）；抽屉 → 会话 里有那条长会话，点开照缓存画、不连机器。
 * `:baselineprofile` 每一组量之前 `am start -W` 它一次（同一份写两遍结果一样）；写完自己关。
 *
 * - 机器：TEST-NET 地址（192.0.2.1，RFC 5737，不可达）、不带钥匙；记成上次那台。
 * - 会话清单：`tests/__fixtures__/history-list.golden.json`（后端那份金样，构建时拷进这两族的 assets），顶层第一条 claude 会话改名成 [TITLE]。
 * - 那条长会话的记录：[TURNS] 轮问答，答里有段落 · 标题 · 列表 · 代码块；字是 ASCII 占位，不是真记录。
 */
class BenchSeedActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        runBlocking(Dispatchers.IO) { seed() }
        finish()
    }

    private suspend fun seed() {
        get<HostRepository>().save(Host(id = HOST, label = HOST, host = "192.0.2.1", username = HOST))
        get<SettingsRepository>().setLastHost(HOST)
        val cache = get<LocalCache>()
        val golden = assets.open(GOLDEN_ASSET).use { RawJson.read(it.reader().readText()) }
        val hand = SyncHand().apply(checkNotNull(HistoryList.patch(golden)) { "$GOLDEN_ASSET 不是 history-list 的回包形" })
        // 挑一条列表顶层就画得出的（不是分叉出来的、没藏、claude 那一家）。
        val sid =
            hand.order.first { k ->
                val r = hand.items.getValue(k) as Map<*, *>
                r["forkedFromSessionId"] == null && r["hidden"] == null && r["agent"] == AGENT
            }
        val row = (hand.items.getValue(sid) as Map<*, *>) +mapOf("title" to TITLE, "label" to TITLE, "aiTitle" to TITLE, "customTitle" to null)
        cache.list(HOST).keep(hand.copy(items = hand.items + (sid to row)))
        var end = 0L
        val rows =
            records().map { r ->
                end += BYTES_PER_LINE
                HistoryRead.row(end, Wire.args<LineRecord>(r))
            }
        cache.chat(HOST).replace(sid, listOf(RawPage(0L, end, rows)), null)
    }

    private fun records(): List<LineRecord> =
        (1..TURNS).flatMap { n ->
            val ask = "turn $n: walk through the frame decoder and say where the copy happens"
            listOf(
                LineRecord.Said(
                    agent = AGENT,
                    id = "q-$n",
                    draw = Draw.CARD,
                    human = true,
                    who = UserText(Speaker.Human, ask),
                    blocks = listOf(Block.Text(ask, ask.length.toLong())),
                ),
                LineRecord.Reply(
                    agent = AGENT,
                    id = "a-$n",
                    draw = Draw.CARD,
                    human = false,
                    autoReply = false,
                    blocks = listOf(Block.Prose(answer(n), "")),
                ),
            )
        }

    private fun answer(n: Int): Doc =
        Doc(
            listOf(
                MdBlock.H(2, listOf(text("Step $n: decoder path"))),
                MdBlock.P(
                    listOf(
                        text("The reader pulls a frame off the socket, checks the length prefix, then hands the slice to "),
                        code("decode_frame()"),
                        text(". Nothing is copied until "),
                        bold("the payload is split"),
                        text(" into records; each record keeps a borrowed view into the same buffer."),
                    ),
                ),
                MdBlock.List(
                    null,
                    true,
                    listOf(
                        item("length prefix is read as u32, big endian"),
                        item("payload over the cap is rejected before allocation"),
                        item("records are yielded lazily, one per line"),
                    ),
                ),
                MdBlock.Code(
                    "rust",
                    "fn decode_frame(buf: &[u8]) -> Result<Frame<'_>, Error> {\n" +
                        "    let len = u32::from_be_bytes(buf[..4].try_into()?) as usize;\n" +
                        "    if len > CAP { return Err(Error::TooLong(len)); }\n" +
                        "    Ok(Frame { body: &buf[4..4 + len], seq: $n })\n" +
                        "}",
                    false,
                ),
                MdBlock.P(listOf(text("So the only copy on this path is the final "), code("to_owned()"), text(" when a record leaves the session."))),
            ),
            emptyList(),
        )

    private fun text(s: String): Inl = Inl.Text(s)

    private fun code(s: String): Inl = Inl.InlNode(InlNode.C(s))

    private fun bold(s: String): Inl = Inl.InlNode(InlNode.B(listOf(Inl.Text(s))))

    private fun item(s: String) = MdItem(null, listOf(MdBlock.P(listOf(text(s)))))

    companion object {
        const val HOST = "bench"
        const val TITLE = "bench-long-session"
        const val GOLDEN_ASSET = "bench/history-list.golden.json"
        private const val AGENT = "claude"
        private const val TURNS = 60
        private const val BYTES_PER_LINE = 1_200L
    }
}
