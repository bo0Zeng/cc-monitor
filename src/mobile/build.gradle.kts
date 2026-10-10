// Top-level build file — plugins declared here with `apply false`, applied per-module.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.jvm) apply false // 纯 JVM 模块用；在根解析版本，子模块 apply 时不冲突
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.ksp) apply false
    // 根声明、subprojects 统一应用，全模块共享一份 .editorconfig 风格基线。
    alias(libs.plugins.ktlint) apply false
    alias(libs.plugins.detekt) apply false
}

// 构建输出不落在源码树里：cc-monitor 有一族判据走磁盘收 `src/` 下的 .css / .js / .ts（Gradle 的测试报告里就有），
// 产物落 `src/mobile/**/build/` 会把那些人群淹掉。照 `src/backend` 的先例，统一落仓根 `.build/mobile/<模块>`（已被忽略）。
// `.gradle/` 由 gradle.properties 的 org.gradle.projectcachedir 挪走；`.kotlin/`（编译守护的会话标记）留在原处，只有 .salive 与日志。
val buildRoot = rootDir.parentFile.parentFile.resolve(".build/mobile")
allprojects {
    layout.buildDirectory.set(buildRoot.resolve(if (this == rootProject) "_root" else name))
}

// 所有 Kotlin 子模块统一接 ktlint + detekt。
// ktlint 引擎版本从 version catalog 取；在 subprojects{} 外读，子工程闭包里的 libs 访问器解析不到。
val ktlintToolVersion = libs.versions.ktlintTool.get()

subprojects {
    apply(plugin = "org.jlleitschuh.gradle.ktlint")
    apply(plugin = "dev.detekt") // detekt 2.0（dev.detekt 命名空间，K2 类型解析）

    extensions.configure<org.jlleitschuh.gradle.ktlint.KtlintExtension> {
        version.set(ktlintToolVersion)
        // 风格规则集中在根 .editorconfig；这里只配行为。
        filter {
            // 不检 build 产物（KSP/Room 生成的 Kotlin）。
            exclude { it.file.path.contains("${layout.buildDirectory.get().asFile.path}") }
        }
    }

    extensions.configure<dev.detekt.gradle.extensions.DetektExtension> {
        // detekt 2.0 的扩展属性是惰性的（Property / RegularFileProperty），用 .set()。
        buildUponDefaultConfig.set(true)
        // 每模块一份 baseline（共享单文件会被各模块 detektBaseline 互相覆盖）；统一放 config/detekt/。
        // 这个 baseline 是模板：plain `detekt`（无类型解析）直接用它；带类型解析的变体任务自动派生
        // `baseline-<module>-<variant>.xml`。CI 跑带类型解析的变体：Android 模块 `detektDebug`，纯 JVM 模块 `detektMain`。
        // 注意：`-<variant>` 文件只在该模块有问题时才存在，detekt 不写空 baseline；缺了就当空的，别手动补。
        // 重生：Android 模块 `:<m>:detektBaselineDebug`，JVM 模块 `:<m>:detektBaselineMain`，模板 `:<m>:detektBaseline`。
        baseline.set(rootProject.file("config/detekt/baseline-${project.name}.xml"))
        config.setFrom(rootProject.file("config/detekt/detekt.yml"))
    }

    // compileSdk / minSdk / Java 版本 / jvmTarget 只在这里写一份；模块只留自己的
    // namespace、buildFeatures、testInstrumentationRunner、consumerProguard 与 app 的 targetSdk。
    // 用具体的 Library / ApplicationExtension，不用泛型 CommonExtension，对 AGP 版本不敏感。
    plugins.withId("com.android.library") {
        extensions.configure<com.android.build.api.dsl.LibraryExtension> {
            compileSdk = 36
            defaultConfig { minSdk = 26 }
            compileOptions {
                sourceCompatibility = JavaVersion.VERSION_11
                targetCompatibility = JavaVersion.VERSION_11
            }
        }
    }
    plugins.withId("com.android.application") {
        extensions.configure<com.android.build.api.dsl.ApplicationExtension> {
            compileSdk = 36
            defaultConfig { minSdk = 26 } // targetSdk 在 app 模块里
            compileOptions {
                sourceCompatibility = JavaVersion.VERSION_11
                targetCompatibility = JavaVersion.VERSION_11
            }
        }
    }
    // 所有 Kotlin 编译（android 与 jvm 都是 KotlinCompile）统一 jvmTarget = JVM_11。
    tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
        compilerOptions {
            jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11)
        }
    }
}
