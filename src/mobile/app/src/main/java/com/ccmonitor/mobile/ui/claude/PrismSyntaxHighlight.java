package com.ccmonitor.mobile.ui.claude;

import androidx.annotation.ColorInt;

import io.noties.markwon.syntax.Prism4jTheme;
import io.noties.markwon.syntax.Prism4jThemeDarkula;
import io.noties.markwon.syntax.Prism4jThemeDefault;
import io.noties.markwon.syntax.SyntaxHighlightPlugin;
import io.noties.prism4j.Prism4j;
import io.noties.prism4j.annotations.PrismBundle;

/**
 * V11 代码块语法高亮：prism4j-bundler 入口 + 高亮插件工厂。
 *
 * <p>为什么是 Java（而非 Kotlin/kapt）：{@code prism4j-bundler} 是 <b>Java 注解处理器</b>，
 * {@link PrismBundle} 在 javac+APT 阶段生成 {@code GrammarLocatorDef}（同包）。本工程 KSP-only、刻意不引 kapt。
 * 把对生成类 {@code GrammarLocatorDef} 的全部引用关在 <b>Java 内</b>，只向 Kotlin 暴露 {@link #create} 工厂
 * —— Kotlin 编译期只解析签名（不碰方法体），故无需 kapt 也不触发"Kotlin 看不到 APT 生成类"的编译顺序问题。
 *
 * <p>语言子集（控包体，覆盖 Android/Claude 常见块）。<b>shell/bash 不在 prism4j 支持内</b>，
 * 这类块自动回退纯文本（不崩）。
 */
@PrismBundle(
        include = {
                "clike", "c", "cpp", "java", "kotlin", "javascript",
                "json", "python", "go", "yaml", "sql", "css",
                "markup", "groovy", "git", "makefile"
        },
        grammarLocatorClassName = ".GrammarLocatorDef"
)
public final class PrismSyntaxHighlight {

    private PrismSyntaxHighlight() {
    }

    /**
     * 构建语法高亮插件。{@code dark} 由 Kotlin 侧按主题判定；{@code codeBg} 传我们主题的代码块背景
     * 色，使高亮主题背景与卡片一致（避免暗/亮冲突）。
     */
    public static SyntaxHighlightPlugin create(@ColorInt int codeBg, boolean dark) {
        final Prism4j prism4j = new Prism4j(new GrammarLocatorDef());
        final Prism4jTheme theme =
                dark ? Prism4jThemeDarkula.create(codeBg) : Prism4jThemeDefault.create(codeBg);
        return SyntaxHighlightPlugin.create(prism4j, theme);
    }
}
