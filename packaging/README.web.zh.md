# 奶蛙对打 {{VERSION}} · 浏览器客户端

[English](README.en.md)

本包已经包含编译好的 WASM、网页和原始 2D 运行素材，无需 Rust 或重新构建。

离线体验需要 Node.js 22+；在解压后的包目录执行（不需要 npm install）：

    node scripts/serve-web.mjs --host 127.0.0.1 --port 18080

打开 http://127.0.0.1:18080。必须通过 HTTP 服务访问，不能直接双击 HTML 文件。此静态服务器只支持单机/本地双人，不提供私人联机。

要联机，请下载同版本服务端包；服务端包已经内置本浏览器客户端。键盘与触屏可用，手柄 API 是否可用由浏览器决定，通常需要 HTTPS 或 localhost。

release-manifest.json 和压缩包旁的 .sha256 用于核对版本、来源提交与文件摘要。MIT、字体许可及素材来源随包附带。
