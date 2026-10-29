# 奶蛙对打 {{VERSION}} · 权威服务端

[English](README.en.md)

本包包含服务端程序、同一提交构建的浏览器客户端及原始 2D 运行素材。无需再下载浏览器包。Linux 包使用 x86_64 musl 目标，Windows 包使用 x86_64 MSVC 目标。

启动脚本默认仅监听本机 127.0.0.1:18080。先设置至少 32 字节的私密访问口令，脚本不生成或保存固定口令。

Windows PowerShell：

    $key = Read-Host '服务口令（至少 32 字节）' -AsSecureString
    $env:NAIFROG_ACCESS_KEY = [Net.NetworkCredential]::new('', $key).Password
    .\start-server.ps1

Linux Bash：

    read -rs -p 'Service key (at least 32 bytes): ' NAIFROG_ACCESS_KEY
    export NAIFROG_ACCESS_KEY
    ./start-server.sh

然后在浏览器打开 http://127.0.0.1:18080，用刚才的口令登录。局域网/公网部署必须另设 NAIFROG_BIND、NAIFROG_ORIGIN 及代理配置，详见 DEPLOY.md。运行单个服务端实例。

Windows EXE 客户端可通过 HTTP 连接；HTTPS 服务请用浏览器。HTTP 不加密口令和流量。不要把口令放在 URL、截图或公开日志中。

程序支持 --version；release-manifest.json 和压缩包旁的 .sha256 用于核对版本与完整性。项目与第三方许可证均包含在包内。
