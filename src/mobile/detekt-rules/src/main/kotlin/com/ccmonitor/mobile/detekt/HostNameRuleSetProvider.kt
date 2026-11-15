package com.ccmonitor.mobile.detekt

import dev.detekt.api.Config
import dev.detekt.api.Entity
import dev.detekt.api.Finding
import dev.detekt.api.Rule
import dev.detekt.api.RuleName
import dev.detekt.api.RuleSet
import dev.detekt.api.RuleSetId
import dev.detekt.api.RuleSetProvider
import org.jetbrains.kotlin.psi.KtBinaryExpression
import org.jetbrains.kotlin.psi.KtCallExpression
import org.jetbrains.kotlin.psi.KtExpression
import org.jetbrains.kotlin.psi.KtFile
import org.jetbrains.kotlin.psi.KtNameReferenceExpression
import org.jetbrains.kotlin.psi.KtParenthesizedExpression
import org.jetbrains.kotlin.psi.KtQualifiedExpression
import org.jetbrains.kotlin.psi.psiUtil.collectDescendantsOfType
import java.net.URI

/**
 * 机器怎么称呼（`hostname`）：一台机器的名字只照 `machineNames`（拨号核心算的，同桌面那张表），不在出口自己拿 `label` 兜机器 id。认两形：
 * - `….label ?: <机器 id>`
 * - `….label.ifBlank { <机器 id> }`（`ifEmpty` 同）
 *
 * 「`….label`」：左边（或 `ifBlank` 的接收方）的值就是一个 label（见 [readsLabel]：先存进变量的 `label`、后面接着调的、包在 lambda 里交出来的都算）。
 * 「机器 id」：名字是 `hostId` 或 `id`（光名字或 `x.hostId` · `x.id`）。
 *
 * 买不到：只看语法、不解类型 —— 叫别的名字的机器 id 认不出；同一形兜的不是机器（身份的 `id.label.ifBlank { id.id }`）也报，那一处标
 * `@Suppress("MachineNameFallback")` 并写明兜的是什么；直接读 `host.label` 当名字、不兜底的认不出。
 */
object HostNameScan {
    private val ID_NAMES = setOf("hostId", "id")
    private val BLANK_CALLS = setOf("ifBlank", "ifEmpty")

    fun scan(file: KtFile): List<KtExpression> {
        val elvis = file.collectDescendantsOfType<KtBinaryExpression> { it.operationReference.text == "?:" && readsLabel(it.left) && isMachineId(it.right) }
        val blank =
            file.collectDescendantsOfType<KtQualifiedExpression> { q ->
                val call = q.selectorExpression as? KtCallExpression
                call?.calleeExpression?.text in BLANK_CALLS && readsLabel(q.receiverExpression) && isMachineId(lambdaResult(call))
            }
        return elvis + blank
    }

    private fun unwrap(e: KtExpression?): KtExpression? {
        var x = e
        while (x is KtParenthesizedExpression) x = x.expression
        return x
    }

    private fun lastName(e: KtExpression?): String? =
        when (val x = unwrap(e)) {
            is KtNameReferenceExpression -> x.getReferencedName()
            is KtQualifiedExpression -> (x.selectorExpression as? KtNameReferenceExpression)?.getReferencedName()
            else -> null
        }

    /**
     * 这一段的值就是一个 `label`：`label` · `x.label` · `x?.label`，或在它后面接着调（`….getOrNull()?.ifBlank { null }`），
     * 或包在 lambda 里交出来（`runCatching { h?.label }`）。只在条件里读的（`firstOrNull { it.label == q }?.id`）不算。
     */
    private fun readsLabel(e: KtExpression?): Boolean =
        when (val x = unwrap(e)) {
            is KtNameReferenceExpression -> x.getReferencedName() == "label"
            is KtQualifiedExpression ->
                when (val s = x.selectorExpression) {
                    is KtNameReferenceExpression -> s.getReferencedName() == "label"
                    is KtCallExpression -> readsLabel(x.receiverExpression)
                    else -> false
                }
            is KtCallExpression -> readsLabel(lambdaResult(x))
            else -> false
        }

    private fun lambdaResult(call: KtCallExpression?): KtExpression? {
        val lambda = call?.lambdaArguments?.singleOrNull() ?: return null
        val body = lambda.getLambdaExpression()?.bodyExpression ?: return null
        return body.statements.lastOrNull()
    }

    private fun isMachineId(e: KtExpression?): Boolean = lastName(e) in ID_NAMES
}

class HostNameRuleSetProvider : RuleSetProvider {
    override val ruleSetId = RuleSetId("hostname")

    override fun instance(): RuleSet = RuleSet(ruleSetId, mapOf(RuleName("MachineNameFallback") to { c: Config -> HostNameFallback(c) }))
}

class HostNameFallback(
    config: Config,
) : Rule(config, "机器的名字只照 machineNames，不拿 label 兜机器 id", URI("https://github.com/")) {
    override fun visit(root: KtFile) {
        HostNameScan.scan(root).forEach { report(Finding(Entity.from(it), "机器的名字照 machineNames 取：「${it.text}」")) }
    }
}
