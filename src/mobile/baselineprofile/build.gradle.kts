// Baseline Profile 的生成器与冷启量测（Macrobenchmark）：另一个包里驱动 `:app`，要一台模拟器或真机，不进门禁。
// 入口 `scripts/baseline-profile.sh`（起私有模拟器 · 录 · 量 · 收）。两条路写在 `Journeys.kt` 一处，生成器与量测共用。
// 插件按 app 的构建类型派生两族变体：`nonMinified<类型>`（录 profile，不混淆）· `benchmark<类型>`（量，照正式包混淆、调试签名）。
import groovy.json.JsonSlurper

plugins {
    alias(libs.plugins.android.test)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.androidx.baselineprofile)
}

// 两条路上要点的两处字（抽屉按钮的无障碍名 · 抽屉里「会话」）从文案表取：与 app 上屏的是同一句，这里不另写一份。
val copyTable: File = rootProject.file("../shared/copy/table.json")

@Suppress("UNCHECKED_CAST")
fun copyZh(key: String): String {
    val entries = (JsonSlurper().parse(copyTable) as Map<String, Any?>)["entries"] as Map<String, Map<String, Any?>>
    val zh = entries[key]?.get("zh") as? String ?: throw GradleException("文案表里没有 $key")
    return "\"" + zh.replace("\\", "\\\\").replace("\"", "\\\"") + "\""
}

android {
    namespace = "com.ccmonitor.mobile.baselineprofile"
    targetProjectPath = ":app"
    defaultConfig {
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "DRAWER_OPEN", copyZh("mobDrawer.open.aria"))
        buildConfigField("String", "DRAWER_SESSIONS", copyZh("mobDrawer.item.sessions"))
    }
    buildFeatures { buildConfig = true }
    // 对上 app 的 `minified`（x86_64，本机模拟器装得上）：插件照它派生 `nonMinifiedMinified` · `benchmarkMinified` 两支。
    buildTypes { create("minified") }
}

// 用 adb 连着的那一台（脚本起的私有模拟器，或接上的真机），不用 Gradle 托管设备。
baselineProfile {
    useConnectedDevices = true
}

dependencies {
    implementation(libs.androidx.test.ext.junit)
    implementation(libs.androidx.benchmark.macro.junit4)
}
