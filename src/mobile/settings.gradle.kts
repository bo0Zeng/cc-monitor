// 本地优先用国内镜像提速；CI（GitHub Actions 总设 CI=true）访问镜像慢且易断，直接用官方源。
// 注意：`System.getenv("CI")` 要在每个 repositories 块里内联调用，pluginManagement 先于脚本主体求值，引用不到顶层 val。
pluginManagement {
    repositories {
        if (System.getenv("CI") == null) {
            maven { url = uri("https://maven.aliyun.com/repository/gradle-plugin") }
            maven { url = uri("https://maven.aliyun.com/repository/google") }
            maven { url = uri("https://maven.aliyun.com/repository/public") }
        }
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        // termlib 从仓内 maven 仓 libs/m2 解析（aar 4 个 ABI + pom + .module + sources），fresh clone 与 CI 直接可构建。
        // termlib 与 app 的 AGP 版本不同，不能 composite build。
        // 放最前，且只对 org.connectbot 开放，不抢其它依赖。
        // 升级 termlib：另外构建新版本，拷进 libs/m2 的新版本目录，再改 libs.versions.toml。
        maven {
            url = uri("$rootDir/libs/m2")
            content { includeGroup("org.connectbot") }
        }
        if (System.getenv("CI") == null) {
            maven { url = uri("https://maven.aliyun.com/repository/google") }
            maven { url = uri("https://maven.aliyun.com/repository/public") }
        }
        google()
        mavenCentral()
    }
}

rootProject.name = "android-terminal"
include(":app")
include(":core-ui")
include(":core-data")
include(":core-ssh")
include(":core-terminal")
include(":core-claude")
include(":core-remote")
