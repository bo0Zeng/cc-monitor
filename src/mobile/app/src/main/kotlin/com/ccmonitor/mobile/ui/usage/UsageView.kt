package com.ccmonitor.mobile.ui.usage

import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.QuotaCell
import com.ccmonitor.mobile.core.wire.gen.QuotaKind
import com.ccmonitor.mobile.core.wire.gen.QuotaReadState
import com.ccmonitor.mobile.core.wire.gen.QuotaVerdict
import com.ccmonitor.mobile.core.wire.gen.Usage
import com.ccmonitor.mobile.core.wire.gen.QuotaRead as QuotaReadReply

/**
 * 用量屏要画的那一份：结论两行（[verdict]）或那台那一句（[text]，没记过账 · 一个号都没有 · 读不出）· 每号一张卡（照回包先后）。
 * 读不出 ⇒ [failed] 带复制详情（那一句画成红条）。
 */
data class UsageUi(
    val verdict: QuotaVerdict?,
    val text: String?,
    val failed: Failed?,
    val cards: List<UsageCard>,
) {
    data class Failed(
        val detail: String,
    )
}

/**
 * 一张卡。卡头 [head] 是那台写好的第一行（名 · 类型 · 需登录）；号牌对上账号库那一行（对不上 ⇒ `null`，不画号牌）。
 * 收起时画 [bars]（那台写好的每个窗口那一行一条）或 [usage]（按量号那一格）或 [plain]（没出过数的那几行）。
 * 点开画 [detail]（第一行之后、没画成条也没在收起时画过的行）＋ [sampled]（几点采的，不成行）；都没有 ⇒ 不可展开。
 */
data class UsageCard(
    val account: String,
    val plate: String?,
    val color: Int?,
    val head: List<QuotaCell>,
    val isDefault: Boolean,
    val bars: List<UsageBar>,
    val usage: Usage?,
    val plain: List<List<QuotaCell>>,
    val detail: List<List<QuotaCell>>,
    val sampled: String?,
)

/**
 * 一条计量条 ＝ 那台写好的一个窗口那一行（[cells]：名 · 用量 · 几点重置 · 还剩多久，或名 · 读取中 / 已重置）
 * ＋ 那一窗的条长（0..1；那台说不出百分比 ⇒ `null`，不当 0）· 「1% ≈ 多少」· 数旧（斜纹）。
 */
data class UsageBar(
    val cells: List<QuotaCell>,
    val fill: Float?,
    val perPct: String?,
    val stale: Boolean,
) {
    val label: String get() = cells.firstOrNull()?.text.orEmpty()
}

/** 回包 ⇒ 要画的那一份：只按号名对上两份回包（号牌 · 默认），次序 · 字 · 语气全照回包；条照每个窗口指着的那一行（`row`）画。 */
fun usageView(
    reply: QuotaReadReply,
    accounts: List<AccountRow>,
    defaultAccount: String?,
): UsageUi {
    val byName = accounts.associateBy { it.name }

    fun card(
        account: String,
        rows: List<List<QuotaCell>>,
        build: (UsageCard) -> UsageCard,
    ): UsageCard {
        val a = byName[account]
        return build(
            UsageCard(account, a?.plate, a?.color, rows.firstOrNull().orEmpty(), account == defaultAccount, emptyList(), null, emptyList(), emptyList(), null),
        )
    }
    val seen =
        reply.accounts.map { q ->
            card(q.account, q.rows) { c ->
                val more = q.rows.drop(1)
                val bars =
                    q.windows.orEmpty().mapNotNull { w ->
                        w.row?.let { i -> q.rows.getOrNull(i) }?.let { r ->
                            UsageBar(r, w.pct?.let { p -> p.coerceIn(0, FULL) / FULL.toFloat() }, w.perPctText, q.stale)
                        }
                    }
                // 点开只列条外的行（超额那一行之类），不把画成条的行再画一遍。
                val drawn =
                    q.windows
                        .orEmpty()
                        .mapNotNull { it.row }
                        .toSet()
                val rest =
                    q.rows
                        .withIndex()
                        .filter { (i, _) -> i > 0 && i !in drawn }
                        .map { it.value }
                when {
                    q.kind == QuotaKind.API -> c.copy(usage = q.usage, detail = more, sampled = q.sampled)
                    bars.isEmpty() -> c.copy(plain = more, sampled = q.sampled)
                    else -> c.copy(bars = bars, detail = rest, sampled = q.sampled)
                }
            }
        }
    val unseen = reply.unseen.map { u -> card(u.account, u.rows) { it.copy(plain = u.rows.drop(1)) } }
    val failed = if (reply.state == QuotaReadState.UNREADABLE) UsageUi.Failed(reply.detail.orEmpty()) else null
    return UsageUi(reply.verdict, reply.text.takeIf { reply.verdict == null }, failed, seen + unseen)
}

private const val FULL = 100
