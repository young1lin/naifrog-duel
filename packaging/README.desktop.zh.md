# 奶蛙对打 {{VERSION}} · Windows 客户端

[English](README.en.md)

完整解压后启动 naifrog-duel.exe；不要只复制 EXE，旁边的 assets 文件夹是运行所需的原始 2D 动画、字体和背景。无需安装 Rust、Node.js 或重新生成素材。

Windows x64 需要 Microsoft Visual C++ v14 运行库。若提示缺少 VCRUNTIME140.dll，请安装 [微软官方 x64 运行库](https://aka.ms/vc14/vc_redist.x64.exe)后重试；[版本与系统要求](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/)。

- 单机：你对电脑；键盘两套键位和任意手柄都控制你。
- 本地双人：P1 用 A/D/W/S + F/G/H/T 或第一个手柄；P2 用方向键 + J/K/L/I 或第二个手柄。
- 手柄：左摇杆/十字键移动，A 跳，X 轻拳，Y 重拳，B 冲刺，RB 奶泡弹，LB/LT/RT 防御。Start 重开，Select 暂停/继续。
- 私人联机：启动时指定服务器地址，在菜单输入服务口令。Windows 和浏览器客户端可加入同一房间。

    .\naifrog-duel.exe --server http://server.example.com:18080
    .\naifrog-duel.exe --lang en
    .\naifrog-duel.exe --version

本版桌面联机仅支持 HTTP；HTTPS 服务器使用浏览器客户端。公网 HTTP 不加密口令和对局数据。浏览器版的 HTTPS 手柄限制不适用于原生 EXE。

release-manifest.json 记录版本、源提交和文件 SHA256；压缩包旁的 .sha256 用来校验下载。MIT 与字体等第三方许可保存在包内。
