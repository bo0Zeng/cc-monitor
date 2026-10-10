import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// 发布签名：从 gitignored 的 keystore.properties 读凭据；没有这个文件就产出未签名的 release，照样能构建。
val keystorePropsFile = rootProject.file("keystore.properties")
val keystoreProps =
    Properties().apply {
        // 用 UTF-8 reader：InputStream 按 ISO-8859-1 解码，非 ASCII 路径会乱码，找不到密钥库。
        if (keystorePropsFile.exists()) keystorePropsFile.reader(Charsets.UTF_8).use { load(it) }
    }

android {
    namespace = "com.ccmonitor.mobile"

    // debug 变体直接读 `bridge/vectors/` 的金样，不另存一份副本（两份会各自漂移）。
    // 注意：不能把整个目录 srcDir 进去，那样 README 与原始录制也会进 APK，往那里放的任何东西都会自动进包。
    // 所以只同步 app 会读的 `*.frames.ndjson`。
    sourceSets.getByName("debug") {
        assets.srcDir(layout.buildDirectory.dir("generated/vectorAssets"))
    }

    // 共享测试源目录 `testing/src/test/kotlin`（源码词法扫描器 KotlinSourceScanner）接进 test 源集。
    // `:core-claude` 接的是同一个目录，全仓只有一份实现；目录里的自检在两个模块各跑一遍，哪边接线掉了哪边编译不过。
    sourceSets.getByName("test") {
        kotlin.srcDir(rootProject.file("testing/src/test/kotlin"))
    }
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置；targetSdk 在这里。
    signingConfigs {
        if (keystorePropsFile.exists()) {
            create("release") {
                storeFile = file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    defaultConfig {
        applicationId = "com.ccmonitor.mobile"
        targetSdk = 36
        versionCode = 11
        versionName = "0.6.1"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            // R8 full mode：开 minify，CI 借此验 BC / sshj / Tink 的反射在收缩与混淆下不破。
            // BC / sshj 的 keep 来自 core-ssh 的 consumer-rules.pro；Tink 的 keep 在 app 的 proguard-rules.pro。
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // 正式签名：keystore.properties 存在则用，否则不签。
            if (keystorePropsFile.exists()) signingConfig = signingConfigs.getByName("release")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true // BuildConfig.DEBUG 门控只在 debug 里做的播种
    }
    // :app:lintDebug 默认只扫 app 源，NewApi 一类问题多在 core-data / core-ssh，所以下钻依赖模块。
    lint {
        checkDependencies = true
    }
}

// prism4j 带进 org.jetbrains:annotations-java5，与主 annotations 重复，排除掉免 duplicate-class。
configurations.all {
    exclude(group = "org.jetbrains", module = "annotations-java5")
}

dependencies {
    implementation(project(":core-ui"))
    implementation(project(":core-data"))
    implementation(project(":core-ssh"))
    implementation(project(":core-terminal"))
    implementation(project(":core-claude"))
    implementation(project(":core-remote"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.activity.compose)

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.ui.tooling.preview)
    implementation(libs.androidx.material3)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.google.material)
    implementation(libs.kotlinx.coroutines.android)

    implementation(libs.koin.android)
    implementation(libs.koin.androidx.compose)

    // Markdown 渲染
    implementation(libs.markwon.core)
    implementation(libs.markwon.ext.tables)
    implementation(libs.markwon.ext.strikethrough)
    implementation(libs.markwon.linkify)
    implementation(libs.markwon.ext.latex) // LaTeX 数学（JLaTeXMath）

    // 代码块语法高亮：syntax-highlight + Prism4j。bundler 是 Java 注解处理器，用 annotationProcessor
    // 处理 src/main/java 下的 @PrismBundle 类（生成 GrammarLocatorDef），不必为一个注解引入 kapt。
    implementation(libs.markwon.syntax.highlight)
    implementation(libs.prism4j)
    annotationProcessor(libs.prism4j.bundler)

    debugImplementation(libs.androidx.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test) // ViewModel 测试：runTest + Dispatchers.setMain

    // Compose UI 测试。注意：androidTest 也要 platform(bom)，BOM 不跨 configuration 传递，漏了 ui-test-junit4 解析不到版本。
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules) // GrantPermissionRule
    // ui-test-manifest 提供测试宿主 Activity 的 manifest 条目，必须落 debug 变体（不是 androidTest）。
    debugImplementation(libs.androidx.ui.test.manifest)
}

/**
 * 把金样里 app 会读的那部分同步进 debug assets：只挑 `*.frames.ndjson`（帧流），不含原始录制、daemon 线样本与 README。
 */
val syncVectorAssets by tasks.registering(Sync::class) {
    from(rootProject.file("bridge/vectors")) { include("*.frames.ndjson") }
    into(layout.buildDirectory.dir("generated/vectorAssets"))
}

tasks.matching { it.name.startsWith("merge") && it.name.contains("DebugAssets") }.configureEach {
    dependsOn(syncVectorAssets)
}
tasks.matching { it.name == "generateDebugAssets" || it.name == "preDebugBuild" }.configureEach {
    dependsOn(syncVectorAssets)
}
