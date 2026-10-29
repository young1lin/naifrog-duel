# 版本与发布

[English](RELEASING.en.md)

## 三个独立角色

- Windows 客户端：完整游戏，默认 game 功能保留键盘、手柄、单机/同屏和桌面联机。打包 EXE 与原始 2D 运行素材。
- 浏览器客户端：真实编译 WASM、运行匹配版本的 wasm-bindgen，再打包网页和素材。静态调试服务器只能离线游玩。
- 权威服务端：分别编译 Linux x86_64 musl 和 Windows x86_64 MSVC，不启用 Bevy。安装包已经内置同一提交的浏览器客户端。

四个压缩包各带 SHA256；内部 release-manifest.json 记录基础版本、精确提交、目标平台及文件摘要。服务端拒绝拼入不同版本/提交的 Web 包。每个包都有中英文运行说明和必要许可证，不包含原图草稿、个人配置或构建目录。

## CI 与下载

每次分支 push、PR 和手动 CI 都先运行检查，再进行真正的 release 编译。Actions → CI → Artifacts 可下载开发包；文件名附带 -dev.<提交号>。程序 --version 显示 Cargo 基础版本，完整提交见包内清单。Artifacts 保留 7 天。

也可以手动运行 Build packages，选择 all、web、desktop 或 server；server 会自动先构建匹配的 Web 包。成功编译的缓存先于打包保存，打包失败不必再次冷编译。

版本标签会触发 Release 工作流，重跑检查并构建四个平台包，全部成功后创建草稿 Release。仓库可见性不改变，也不会自动点“Publish release”。

服务端与浏览器包启动后会自动验证页面、鉴权及真实 WASM 的 HTTP 交付；Windows EXE 用 --version 验证链接和版本。CI 没有实体手柄，不能代替真手柄或真实跨设备联机验收。

## 版本号如何演进

Cargo.toml 的 package.version 是唯一基础版本；Cargo.lock 中本项目条目必须一致，CHANGELOG.md 必须有对应条目。初始标签为 v0.1.0；其发布检查发现问题后，使用 v0.1.1 修复，不改写旧标签。

- patch：兼容的修复，例如 0.1.0 → 0.1.1。
- minor：新功能；1.0 前不兼容变更也提升 minor，例如 0.1.1 → 0.2.0。
- major：稳定版之后的不兼容版本，例如 1.x → 2.0.0。
- 多人协议 PROTOCOL_VERSION 独立管理；改线协议时同步客户端/服务端并更新它，不能把应用版本直接当协议号。

准备下一版（在干净的新仓库副本内）：

    node scripts/version.mjs bump patch

脚本同步 Cargo.toml、Cargo.lock，并把 Unreleased 内容移入新版本条目。补齐 CHANGELOG 后执行：

    node scripts/version.mjs check
    node --test scripts/*.test.mjs
    node scripts/public-source.mjs manifest
    git add .
    git commit -m "chore: prepare next release"
    $version = node scripts/version.mjs print
    git tag "v$version"
    git push origin main "v$version"

空白/TODO 变更记录、锁文件版本不符、标签与 Cargo 版本不符都会阻止发布。v 标签应指向已审查提交；修复已发布版本时创建新版本，不改写旧标签。

## 草稿验收

1. 下载目标包并核对 .sha256、包内版本和源提交。
2. Windows 完整解压、启动，验证键盘、真手柄和需要的模式。
3. 启动服务端，验证浏览器和 Windows 客户端同房、重连及原始 2D 动画。
4. 检查发布说明后，维护者再手动发布草稿。

只有草稿可由工作流重传；脚本拒绝覆盖已经发布的 Release。构建在 GitHub runner 上执行；本地默认仅 2 个编译任务，勿为发版在工作电脑同时跑多份 Bevy 构建。
