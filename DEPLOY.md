# 奶蛙生产部署（单实例；HTTPS 域名，或没有域名的 http://IP:端口）

本文只提供部署模板与操作步骤。本机（Windows）已执行 release WASM 与 release 服务器构建，并在本机回环上用 release 服务器完成了双浏览器联机验收和安全验收（见 [测试记录](<TESTING.md>)）；**没有部署或测试真实云主机、域名、证书、Caddy、Docker 镜像（本机未安装 Docker）、真实公网延迟、iPad 或游戏手柄**；没有域名的明文 HTTP 方案只在本机用非回环 Origin 模拟验证过（见第 6 节），同样没有上过真实主机。下面的部署命令由管理员在目标 Linux 主机执行，不用于替换当前本地开发服务器。

## 1. 两种服务不能混用

- **生产：** `naifrog-server` 是无渲染器的权威服务器，同源提供网页、受认证保护的 WASM/处理后贴图、认证 API 与 WebSocket。有域名时 HTTPS 由 Caddy 终止，所有路径仍转发给服务器；没有域名时服务器可以直接以 `http://公网IP:端口` 对外，但不加密（第 6 节）。
- **离线调试：** [Node 静态服务器](<scripts/serve-web.mjs>) 只供离线/本机浏览器调试，**没有服务认证、没有在线权威模拟，绝不能公开作为生产服务**。页面能打开不等于安全多人服务已启动。
- 匹配码是随机 **8 字符**私房间定位码，不是服务认证。先用共享服务口令登录，才能下载受保护资源、建房或加入；知道口令的人拥有同等服务访问权限，没有实名账户或好友身份验证。**口令泄漏即授权攻击者访问**；不要把匹配码当作补救措施。
- 只运行 **1 个服务器副本**。房间、认证会话、恢复凭证和配额/预算计数均在内存，重启撤销全部会话/房间并清空计数；不支持多副本、负载均衡分房或无损滚动升级。

## 2. 交付文件与发布内容

| 文件 | 用途 |
| --- | --- |
| [Dockerfile](<Dockerfile>) | Rust 1.98.1 Bookworm 多阶段构建：release WASM + headless Linux 服务；非 root slim 运行镜像 |
| [.dockerignore](<.dockerignore>) | 默认拒绝构建上下文，只放行源码、Cargo 锁文件、四个网页入口及处理后资源 |
| [Compose 配置](<deploy/compose.yml>) | 仅 Caddy 对外发布 80/443；固定小网段与精确信任的代理 IP |
| [Caddy 配置](<deploy/Caddyfile>) | 自动 HTTPS、gzip、整站同源反代、真实 peer IP 覆盖 |
| [Compose 环境示例](<deploy/env.example>) / [私密配置忽略规则](<deploy/.gitignore>) | 域名、证书联系邮箱、空口令占位及可调上限；真实环境勿提交 |
| [systemd 服务示例](<deploy/systemd/naifrog-server.service>) / [服务环境示例](<deploy/systemd/naifrog-server.env.example>) | 裸机 loopback 服务，独立低权限账户 |
| [Caddy 环境示例](<deploy/systemd/caddy.env.example>) / [Caddy drop-in](<deploy/systemd/caddy.service.d/naifrog.conf>) | 裸机 Caddy 使用相同域名及 `127.0.0.1:18080` 后端 |
| [无域名 Compose](<deploy/compose.ip-http.yml>) / [无域名环境示例](<deploy/env.ip-http.example>) | 没有域名：只有服务器一个容器，明文 HTTP 直接发布一个端口，不用 Caddy（第 6 节） |

原始画稿目录 [original](<assets/original/>) 必须原封不动，**不进入 Docker 上下文，不复制到任何运行镜像/发布目录，也不配置公开静态路由**。已有 [processed](<assets/processed/>) 资源是发布输入；缺失时应在可信开发环境按现有资源流程准备，不能在运行容器内临时暴露原稿。发布资源根需包含 `web/` 和 `assets/processed/`；不是仅指向某个贴图目录。

Docker 从源码重新生成 release Web bindings，**不会使用本地已有的 debug 包**。默认 `game` feature 编译 `wasm32-unknown-unknown`，`wasm-bindgen-cli` 必须精确为 **0.2.129**，与 [Cargo 配置](<Cargo.toml>) / [锁文件](<Cargo.lock>) 一致。release 必须带 `--remove-name-section --remove-producers-section`，避免约 49 MB 的调试名称；本机 Windows `-Release`（`web-release` 配置：精简 Bevy 功能、整程序 LTO）构建实测 22,558,130 字节（约 21.5 MiB，低于服务器 64 MiB 单文件上限，含解码贴图用的 WebP 解码器），gzip 副本 5,976,547 字节；Docker 内的 Linux 构建未测量，应当相近。Windows 对应 [Web 构建脚本](<scripts/build-web.ps1>) 的 `-Release` 模式。

原生服务器命令固定为：

```sh
cargo build --locked --release --no-default-features --features server --bin naifrog-server
```

运行镜像仅包含该二进制、网页与处理后资源，使用 UID/GID 10001，**无 Bevy 渲染/音频/窗口/GPU 原生依赖**。构建阶段仍需要编译工具与联网下载 crates；首次构建较重。模板固定版本标签但未验证远端镜像可拉取；正式使用前核实并按自己的供应链策略锁定镜像 digest、维护安全更新。容器 CPU/内存/PID 限额只是保守示例，不是性能或硬件验收结论。

## 3. 先准备域名、口令和防火墙

第 3–5 节是有域名的 HTTPS 方案。没有域名直接看第 6 节，但口令生成和环境文件权限的要求相同。

1. 准备自己控制的域名，把 A 记录指向目标主机的公网 IPv4；**只有主机 IPv6 和防火墙确实可用才添加 AAAA**。不要使用示例域名实际申请证书。
2. 允许入站 **TCP 80、TCP 443**，可选 UDP 443（模板发布它用于 HTTP/3）；允许 Caddy 出站 DNS/HTTPS 以取得并续期证书。保留管理员自己的 SSH 访问规则；不要盲目替换整套防火墙。
3. **不要放行/发布 18080**，也不要对公网发布 Caddy 管理 API。Docker 模式只发布 Caddy，裸机服务必须绑定 `127.0.0.1:18080`；检查云安全组和主机实际规则。Docker 端口映射可能绕过部分主机防火墙规则，因此“不写 backend ports”比只依赖防火墙重要。
4. 共享口令至少 **32 字节**；建议用系统 CSPRNG 生成 **32 随机字节的 64 字符十六进制文本**，而不是人类密码、房间码或重复字符。示例口令故意留空，不提供任何可用示例密钥。
5. 真实环境文件应为 **0600**，目录只允许部署管理员访问（systemd 环境由 root 管理）。只经可信私密渠道分享口令，勿放在 URL、仓库、构建参数、截图、公开日志或 shell 历史中。Docker 管理员可以查看容器环境；环境变量不是抵御本机管理员的秘密保险箱。

以下 Bash 命令均从项目根执行，需要 Docker Engine、Compose v2 和 Python 3。创建独占新文件并直接写入随机口令，不打印密钥；若目标已存在会失败，避免覆盖已用配置：

```sh
umask 077
python3 - <<'PY'
from pathlib import Path
import secrets
source = Path("deploy/env.example").read_text(encoding="utf-8")
assert source.count("NAIFROG_ACCESS_KEY=\n") == 1
with Path("deploy/.env").open("x", encoding="utf-8") as out:
    out.write(source.replace("NAIFROG_ACCESS_KEY=\n",
                             "NAIFROG_ACCESS_KEY=" + secrets.token_hex(32) + "\n"))
Path("deploy/.env").chmod(0o600)
PY

# 下一步：用可信编辑器填写自己控制的域名和真实邮箱
```

用可信编辑器把新环境文件里的 `NAIFROG_DOMAIN` 改成**小写裸域名**（不含协议、端口、路径），`ACME_EMAIL` 改成真实证书联系邮箱；保持口令不为空。例子 `game.example.com` / `you@example.com` 均不是本次实际部署目标。

### 精确 Origin 与信任链

- Compose 模板是标准 HTTPS 443，服务 Origin 由域名生成：`https://你的域名`。它必须与浏览器地址栏的 **scheme + host + 非默认 port** 完全一致，**没有路径、查询或末尾斜杠**；域名小写。默认 HTTPS 443/HTTP 80 要省略，`https://host:443` 会被 canonical 校验拒绝。自定义端口需同时修改公开监听、Caddy 站点和服务 Origin；模板不自动处理它。
- [Caddy 配置](<deploy/Caddyfile>) 对每个请求用 `header_up X-Real-IP {remote_host}` **覆盖**客户端头，并移除传入 X-Forwarded-For/Forwarded；不要改成 append，不设宽泛 `trusted_proxies`，不用客户端 XFF 取 IP。不要修改 `Origin`、Cookie 或 WebSocket 路由。
- [Compose 配置](<deploy/compose.yml>) 使用 `172.28.0.0/29`，Caddy 固定 `172.28.0.2`，服务固定 `172.28.0.3`；服务只信任精确 `172.28.0.2`。这样每位朋友按真实 IP 计额，而不是都被当成一个代理 IP。先检查网段与现有 Docker/VPN 路由不冲突；如更换网段，**同时更换 Caddy 地址、服务地址、IPAM 及服务受信 IP**。
- 裸机只信任 `127.0.0.1`，并强制 Caddy 使用该 IPv4 上游。不要把整个私网/CIDR/所有地址列为可信代理。其它本地进程或 Docker 管理权限同样属于可信主机边界。
- 套在 CDN、额外反代、Docker Desktop/NAT 或私有隧道后，Caddy 的 remote peer 可能不再是朋友 IP。**先验证真实来源传递**；不要为修复配额把任意 XFF 改为可信。此模板不支持这些多跳拓扑的自动配置。

## 4. Docker Compose 上线步骤

先确认环境已填写、DNS 可解析到目标、80/443 空闲、小网段不冲突。**不要输出完整 `docker compose config`，它会展开口令；只做 quiet 校验。**

```sh
docker compose --env-file deploy/.env -f deploy/compose.yml config --quiet
docker compose --env-file deploy/.env -f deploy/compose.yml up -d --build
# 容器启动后，验证其实际 Caddy 配置（此命令不启动替代服务）。
docker compose --env-file deploy/.env -f deploy/compose.yml exec caddy \
  caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
docker compose --env-file deploy/.env -f deploy/compose.yml ps
```

Caddy 会自动取得/续期 HTTPS 证书，并用 gzip 压缩可压缩响应。浏览器打开自己的 HTTPS 域名，输入共享口令再建房，把 8 字符匹配码私发给已拿到口令的朋友。网页、API 与 WS 同源；不需单独 WS 域名、CORS、手工 Upgrade 头或公网 backend URL。

不要加 Caddy `file_server` 或把 WASM/贴图放入另一个公开静态服务，否则绕过认证和流量预算。**不要启用 CDN/反代共享缓存**，也不要在 Caddy 里改写 `Cache-Control`：缓存头由 naifrog-server 统一给出——API、拒绝响应和页面本身（`/`）`private, no-store`——页面若可存，离开时会被留在浏览器的往返缓存（bfcache）里、联机连接冻结而不断开，对方就看不到断线暂停；页面脚本与样式 `no-cache`（每次带 ETag 校验，未变时 304 无正文）；游戏文件只走浏览器私有缓存（`private`）。做法与成熟网页游戏引擎的“文件名带内容哈希 + 资源清单”相同：登录后页面先取 `/api/manifest`（每个游戏文件的内容哈希，需登录、计入文件请求频率），再用 `/v/<该文件哈希>/pkg/…`、`/v/<该文件哈希>/assets/processed/…` 加载；哈希与文件当前内容一致时回 `max-age=31536000, immutable`，之后的访问直接从浏览器缓存载入，不再向服务器请求。更新部署后只有内容变了的文件换哈希、换地址，浏览器只重新下载这些文件；旧地址只按 `private, no-cache` 回源校验。这是普通 HTTP 缓存，`http://IP:端口` 部署同样有效；无痕窗口、清除网站数据或浏览器自行清理缓存后会重新完整下载。离线调试服务器没有清单，页面退回普通地址。证书数据在两个 Caddy 持久卷中，里面有 TLS 私钥；只让管理员访问，重启/升级不要随手 `down -v` 删除，避免重复签发和 ACME 限额。

WASM 的 `.wasm` 与 `.wasm.gz` 应一起发布。服务器启动时预热压缩副本校验；首次读取或任一文件的长度 / 修改时间变化时，流式解压并比较完整 SHA256，同时检查 gzip CRC。校验失败会退回未压缩的当前 `.wasm`，即使旧副本与新版解压长度相同也不会使用。校验结果缓存在内存里，正常请求不重复解压。

只部署一个 server。两服务默认各限制 512 MiB 内存、1 个 CPU 配额、64 PID，日志轮转；server 文件系统只读、无 capabilities、禁提权；Caddy 仅保留低端口监听 capability，证书卷可写。这些容器资源限额与应用出站预算是**不同**概念，可按真实监测调节，不保证任意机器达到目标帧率。服务器镜像的 `EXPOSE 18080` 仅是元数据，不表示发布；`docker compose ps` 的 server 不应出现 host 18080 映射。

## 5. 裸机 systemd 替代方案（不用 Compose 与其并行）

适用于现代 Linux/systemd，提前安装标准 Caddy 服务和 Rust 1.98.1 构建工具链。**不要同时启动 Docker 和裸机两个副本**。也可从已经构建的 Docker 镜像仅导出 `/app` 发布内容，或在 Windows 开发机交叉编译（见本节末），都不必在运行主机安装 Rust。若自行构建，在 Linux 项目目录执行以下命令，保持默认 `game` feature 用于 WASM，而服务器明确禁用它：

```sh
cargo build --locked --release --no-default-features --features server --bin naifrog-server
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
test "$(wasm-bindgen --version)" = "wasm-bindgen 0.2.129"
# web-release：整程序 LTO、单代码生成单元，构建更慢但下载更小（见 Cargo.toml）。
cargo build --locked --profile web-release --target wasm32-unknown-unknown --bin naifrog-duel
wasm-bindgen --target web --no-typescript --out-name naifrog_duel --out-dir web/pkg \
  --remove-name-section --remove-producers-section \
  target/wasm32-unknown-unknown/web-release/naifrog-duel.wasm
# gzip 副本：浏览器接受 gzip 时服务器发它（约为原大小的四分之一）。每次构建后都要重新生成。
gzip -9 -k -n -f web/pkg/naifrog_duel_bg.wasm

# 新主机创建独立账户；已存在时先核对，勿重复创建或复用高权限账户。
sudo useradd --system --user-group --home-dir /nonexistent --no-create-home \
  --shell /usr/sbin/nologin naifrog
sudo install -d -m 0755 /opt/naifrog/web/pkg /opt/naifrog/assets/processed
sudo install -m 0555 target/release/naifrog-server /opt/naifrog/naifrog-server
sudo install -m 0644 web/index.html web/main.js web/network.js web/style.css /opt/naifrog/web/
sudo cp -a web/pkg/. /opt/naifrog/web/pkg/
sudo cp -a assets/processed/. /opt/naifrog/assets/processed/
sudo chown -R root:root /opt/naifrog
sudo chmod -R a=rX /opt/naifrog
sudo install -d -m 0700 /etc/naifrog
```

只复制上面指定的发布内容，不能复制整个项目或 [原稿目录](<assets/original/>)。不要把 Windows 版（`x86_64-pc-windows-*`）二进制装到 Linux；本节末交叉编译出的 `x86_64-unknown-linux-musl` 产物是 Linux 程序，可以安装。发布目录由 root 拥有，服务账户只有读取/执行权限。

生成 root 所有的 **0600** 服务环境，再安装 Caddy 专用环境（其中没有共享口令）：

```sh
sudo python3 - <<'PY'
from pathlib import Path
import os, secrets
os.umask(0o077)
source = Path("deploy/systemd/naifrog-server.env.example").read_text(encoding="utf-8")
assert source.count("NAIFROG_ACCESS_KEY=\n") == 1
with Path("/etc/naifrog/naifrog.env").open("x", encoding="utf-8") as out:
    out.write(source.replace("NAIFROG_ACCESS_KEY=\n",
                             "NAIFROG_ACCESS_KEY=" + secrets.token_hex(32) + "\n"))
Path("/etc/naifrog/naifrog.env").chmod(0o600)
PY
sudo install -m 0600 -o root -g root deploy/systemd/caddy.env.example /etc/naifrog/caddy.env
sudoedit /etc/naifrog/naifrog.env /etc/naifrog/caddy.env
# 填写相同真实域名：naifrog.env 为 https://host，caddy.env 为 host。
# 后端仍 127.0.0.1:18080，受信代理仍 127.0.0.1，禁止公网明文开关。
sudo install -m 0644 deploy/systemd/naifrog-server.service /etc/systemd/system/naifrog-server.service
sudo install -d -m 0755 /etc/systemd/system/caddy.service.d
sudo install -m 0644 deploy/systemd/caddy.service.d/naifrog.conf /etc/systemd/system/caddy.service.d/naifrog.conf
sudo install -m 0644 deploy/Caddyfile /etc/caddy/Caddyfile
sudo systemd-analyze verify /etc/systemd/system/naifrog-server.service
sudo systemctl daemon-reload
sudo systemctl enable --now naifrog-server
# 必须 restart 以加载新的 Caddy EnvironmentFile；仅 reload 不更新进程环境。
sudo systemctl enable caddy
sudo systemctl restart caddy
sudo systemctl status naifrog-server caddy
ss -lntp
```

[服务 unit](<deploy/systemd/naifrog-server.service>) 默认使用 `SIGINT` 配合当前 Ctrl-C 优雅退出；systemd 从 root-only 环境文件读取变量后才降权，服务不必拥有环境文件读取权限。没有配置热重载，更新二进制/域名/口令后要重启。确认后端**只有 loopback**，不要改为公网 `0.0.0.0:18080`，也不要给服务账户写发布目录的权限。Caddy drop-in 假设发行版标准服务读取 `/etc/caddy/Caddyfile`；如发行版布局不同，先核对实际 unit。首次在目标机验证 Caddy 配置、证书签发与安全指令兼容性，不要仅凭本文视为通过。

### 在 Windows 开发机交叉编译（运行主机不装 Rust）

运行主机内存小（如 2 GB）时不要在上面编译 Bevy。可以在 Windows 开发机上编好，只把发布内容传过去：

```powershell
rustup target add x86_64-unknown-linux-musl
# 默认会用 MinGW gcc 链接 Linux 程序而失败；musl 目标自带 C 运行库，改用 Rust 自带的 rust-lld。
$env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = 'rust-lld'
cargo build --locked --release --no-default-features --features server --bin naifrog-server --target x86_64-unknown-linux-musl
.\scripts\build-web.ps1 -Release   # WASM 与平台无关；同时生成 gzip 副本
```

服务器程序在 `target/x86_64-unknown-linux-musl/release/naifrog-server`，是静态链接的 Linux x86-64 程序（`file` 显示 `static-pie linked`），不依赖主机的 glibc 版本。同一份源码重复构建得到相同的字节，传到主机后可以用 `sha256sum` 核对。只适用于 x86-64 主机；ARM 主机要换成对应的 musl 目标，本仓库没有测过。

发布内容与上面相同：这个程序；`web/` 下的四个文件；`web/pkg/` 里的 `naifrog_duel.js`、`naifrog_duel_bg.wasm`、`naifrog_duel_bg.wasm.gz`；`assets/processed/` 下的 `*.webp`、`game-assets.json`、`name_font.json`、`name_font.otf`（名字与界面中文字体）和字体许可证 `name_font-OFL.txt`。预览 GIF 与历史 PNG/JSON 交付件不发布；在开发机先运行 `node scripts/check-assets.mjs`，确认发布目录没有多余文件。在本机按这个结构组好目录，用 Windows 自带的 `tar -czf` 打包，`scp` 传到主机解压，再用上面的 `install` / `cp` 命令安装。Windows 打包会丢掉可执行位，`install -m 0555` 会重新设置。

更新已经在运行的实例：

- 先备份发布目录（如 `sudo cp -a /opt/naifrog /opt/naifrog.prev-日期`），回滚时把目录换回来再重启。
- `cp -a` 不会删除新版已经不用的文件（例如贴图改成 WebP 后留下的旧 PNG），服务器仍会把它们列进清单、允许登录后下载。安装前删掉发布内容里没有的文件，或整个目录替换。
- 只换网页、WASM 或贴图时不用重启：服务器每次请求都从磁盘读取，清单按文件长度和修改时间重新计算哈希。换了服务器程序要 `sudo systemctl restart naifrog-server`，内存里的房间、会话与恢复 token 全部失效，玩家要重新登录、建房。`PROTOCOL_VERSION` 变了时，程序和 WASM 必须一起更新。

## 6. 没有域名：http://IP:端口

没有域名也能私人联机：服务器自己监听一个公网端口，朋友在浏览器打开 `http://服务器公网IP:端口`。浏览器、Cookie 和 WebSocket（`ws://`）都按这个地址工作，不需要 Caddy 和证书。代价必须先想清楚：

- **不加密。** 服务口令（登录时提交一次）、登录 Cookie、匹配码和对局数据都以明文经过网络；同一 Wi-Fi、运营商或中间网络上的人可能看到或篡改。拿到口令或 Cookie 的人就能用你的服务器。只把口令给信任的人，不要和其它服务共用；怀疑泄漏就按第 8 节轮换。
- **手柄可能不可用。** `http://` 不是浏览器的“安全来源”，部分浏览器在这种页面上不提供手柄 API。触屏和键盘不受影响。页面里的“复制匹配码”按钮在这种页面上改用旧式复制（选中隐藏文本框再复制）；浏览器连这也拒绝时，会选中匹配码并提示长按或 Ctrl+C 复制。
- 登录页在 `http://` 地址上会显示一条明文警告；服务器启动日志那一行末尾也会带 `UNENCRYPTED public HTTP`。这是预期的提示，不是故障。
- 服务器必须显式设置 `NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP=1` 才会在非本机地址上接受 `http://` Origin，否则拒绝启动。不信任任何代理（`NAIFROG_PROXY_IPS` 为空），每位朋友按自己连进来的 IP 计额。

### Docker Compose

[无域名 Compose](<deploy/compose.ip-http.yml>) 只有服务器一个容器，把主机端口（默认 18080）直接映射到容器，**不要与 [Compose 配置](<deploy/compose.yml>) 同时运行**（两者项目名相同）。从[无域名环境示例](<deploy/env.ip-http.example>) 生成环境文件，同样不打印口令：

```sh
umask 077
python3 - <<'PY'
from pathlib import Path
import secrets
source = Path("deploy/env.ip-http.example").read_text(encoding="utf-8")
assert source.count("NAIFROG_ACCESS_KEY=\n") == 1
with Path("deploy/.env").open("x", encoding="utf-8") as out:
    out.write(source.replace("NAIFROG_ACCESS_KEY=\n",
                             "NAIFROG_ACCESS_KEY=" + secrets.token_hex(32) + "\n"))
Path("deploy/.env").chmod(0o600)
PY
# 下一步：用可信编辑器把 NAIFROG_PUBLIC_ORIGIN 改成自己的 http://公网IPv4:端口
docker compose --env-file deploy/.env -f deploy/compose.ip-http.yml config --quiet
docker compose --env-file deploy/.env -f deploy/compose.ip-http.yml up -d --build
docker compose --env-file deploy/.env -f deploy/compose.ip-http.yml logs server
```

- `NAIFROG_PUBLIC_ORIGIN` 必须与朋友地址栏里的完全一致：`http://` + 公网 IPv4 + 端口，小写，无路径、无末尾斜杠。用 80 端口时写 `http://203.0.113.10`，**不能**写 `:80`；其它端口必须写出。`NAIFROG_PUBLIC_PORT` 要与 Origin 里的端口相同。示例 IP `203.0.113.10` 是文档保留地址，不是实际目标。
- 云安全组 / 主机防火墙只放行这一个 TCP 端口和管理员自己的 SSH。Docker 发布端口可能绕过 ufw 等主机规则，以云安全组为准。
- 用 IPv4 地址。Docker 默认网桥对 IPv4 发布端口会保留朋友的真实来源 IP；rootless Docker、经 userland-proxy 的 IPv6 等情况下，服务器可能把所有人看成同一个来源，大家共享每 IP 配额（每 IP 4 个 WS、每分钟 10 次登录等）。人少时影响不大；要按人计额就用下面的裸机方案。
- 主机公网 IP 变了（例如重新分配），要同时更新 Origin 并 `up -d --force-recreate server`；旧登录全部失效。

### 裸机 systemd

按第 5 节构建、安装发布目录和[服务 unit](<deploy/systemd/naifrog-server.service>)，但**不安装 Caddy 与它的 drop-in**。生成 `/etc/naifrog/naifrog.env` 后把这几项改成：

```sh
NAIFROG_BIND=0.0.0.0:18080
NAIFROG_ORIGIN=http://203.0.113.10:18080
NAIFROG_PROXY_IPS=
NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP=1
```

服务账户没有低端口权限，请用 1024 以上的端口（如 18080），Origin 里的端口与 `NAIFROG_BIND` 一致。防火墙只放行这个端口。

### 没有域名也想要 HTTPS

Let's Encrypt 从 2026 年 1 月起正式签发 **IP 地址证书**，但只有约 6 天有效的 `shortlived` 证书，必须自动续期，并且要能从公网访问 80/443 完成验证。Caddy 对 IP 证书的支持仍有问题（例如 [caddy#7399](https://github.com/caddyserver/caddy/issues/7399) 报告申请被拒），本仓库**没有提供也没有测试**这套配置。想试的话，先在目标主机上单独验证证书能签发和续期，再把服务按第 3–5 节放在 HTTPS 后面，Origin 写成 `https://公网IP`。在验证通过前，请把本节的明文 HTTP 方案当作实际可用的选项。

## 7. 环境与默认上限

以 [服务端配置](<src/server.rs>) 为实现依据。环境只在进程启动时读取：

| 变量 | 默认 / 本模板 | 说明 |
| --- | --- | --- |
| `NAIFROG_BIND` | 代码默认 `127.0.0.1:18080`；容器 `0.0.0.0:18080` | 有域名：容器仅 bridge 内部，裸机固定 loopback；无域名裸机为 `0.0.0.0:端口` |
| `NAIFROG_ORIGIN` | 模板为实际 HTTPS 域名；无域名为 `http://公网IP:端口` | 精确 canonical Origin，与地址栏一致；不要生产使用缺省本机 HTTP Origin |
| `NAIFROG_ACCESS_KEY` | 必填，至少 32 字节 | 推荐 64 字符随机 hex；示例留空 |
| `NAIFROG_ASSET_ROOT` | 容器 `/app`，裸机 `/opt/naifrog` | 下含网页与处理后资源；代码默认当前目录 |
| `NAIFROG_MAX_ROOMS` | 16；允许 1..256 | 提高房间数不会提高固定 32 WS 总上限 |
| `NAIFROG_PROXY_IPS` | 容器 `172.28.0.2`；裸机 `127.0.0.1`；无域名为空 | 精确 IP 的逗号列表，不是 CIDR；代码默认不信任代理 |
| `NAIFROG_EGRESS_MB_PER_HOUR` | 512；允许 64..8192 | **MiB**，HTTP 文件与 WS 出站合计的保守滚动小时预算，内存计数 |
| `NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP` | 0 / 不启用；无域名模板为 1 | `1` 允许在非本机地址上用明文 `http://` Origin（第 6 节），口令与 Cookie 不加密；有域名的 HTTPS 方案不得启用 |

应用资源边界（最终服务版本）：

| 范围 | 默认限制 |
| --- | --- |
| 私房间 / WS | 16 房间；总计 32 WS、每 IP 4、每认证会话 2；同一 NAT 下会共享 IP 配额 |
| 认证 | 每 IP 10 次/分钟、全局 120 次/分钟；HttpOnly + SameSite=Strict Cookie，HTTPS 加 Secure 与 `__Host-` 前缀 |
| 认证有效期 | 绑定登录时 IP，12 小时**绝对**有效期，不是活动滑动延期；网络/IP 切换可能需重新登录 |
| 静态资源 | 每 IP 120 次/分钟、全局 2048 次/分钟；WASM 额外每 IP 6、全局 12 次/分钟 |
| WS upgrade | 每 IP 30 次/分钟、全局 240 次/分钟，独立于同时在线 WS 上限 |
| WS 客户端消息 | 最大 1024 字节帧/消息；120 消息/秒，burst 180；不是无限请求队列 |
| HTTP 文件 + WS 出站 | 合计 512 MiB/保守滚动小时；最多 4 个并发 WASM 流式生产者，120 秒期限（其它文件同为 120 秒）；浏览器接受 gzip 且有匹配的 `.wasm.gz` 时 WASM 以 gzip 发送 |
| 房间寿命 | 有一方连着就保留（服务器每 10 秒 ping，10 秒无 pong 算断线），最长 12 小时；断线席位宽限 5 分钟 |
| 服务器权威模拟 | 60 Hz 模拟、20 Hz **完整快照**，客户端收到快照后用约 50 ms 平滑过去；没有客户端预测、没有 rollback，延迟直接体现在手感上，客户端不是最终裁判 |
| 输入与重开 | 连接/恢复后两人均提交当前轮次新输入才开始模拟；250 ms 无输入归零；双方当前轮次重开票须在 10 秒内凑齐 |

服务器只在启动时向 stderr 打印一行：监听地址、Origin、房间上限、每小时出站预算与受信代理地址数量；明文公网 HTTP 时末尾另有一句 `UNENCRYPTED public HTTP` 警告。它不记录请求、Cookie、恢复 token 或口令；Caddy 模板也没有开启访问日志。`journalctl -u naifrog-server` 或 `docker compose logs server` 里应只有这一行；浏览器提示连接失败时，先核对这里的 Origin 是否与地址栏完全一致。

恢复 token 绑定 Cookie 认证会话、房间与席位；为丢失 welcome 的重试保持稳定，旧 socket 被 epoch fencing 隔离，不可用其覆盖新连接状态。跨 Cookie 会话不能用旧恢复 token 夺席；过了 5 分钟宽限、认证绝对过期或服务器重启不能假定恢复成功。Cookie 的 HttpOnly 不等于防御所有浏览器恶意脚本；不要添加未审核的第三方页面脚本。

预算耗尽会拒绝/停止继续传输、使 WS 中断；普通朋友也可能耗尽预算，游戏文件只进各自浏览器的私有缓存、不走共享缓存：一个浏览器第一次完整载入约 15.9 MB（WASM gzip 副本约 6.0 MB + 17 张无损 WebP 贴图约 7.9 MB + 名字与界面中文字体约 1.8 MB + 页面与胶水脚本；没有 .gz 副本时 WASM 按约 22.6 MB 计），默认 512 MiB 约够每小时 33 次首次载入；同一浏览器之后再打开只取清单和页面文件的 304，几乎不计流量，更新部署后只计变化的文件；本地反复跑验收脚本就耗尽过，此时贴图返回 503，页面停在载入画面且没有错误提示。提高预算前估算实际并发与出站费。**这不是云账单硬封顶**：计的是应用文件/WS 字节，不含完整 TCP/TLS/代理协议开销、Caddy 生成的响应、证书流量等；计数在内存中，重启清零，不能拿每小时 512 MiB 推导整个主机所有网络费用。

## 8. 口令轮换、更新与验收

- 所有朋友共享同一个服务口令，只发给愿意信任的人。任何持有者可以发请求/占房，匹配码不能撤销他的服务权限，也没有按用户单独封禁/撤权接口。
- 泄漏时生成新的随机口令，用可信编辑器更新实际环境文件。**只修改文件或只 `docker compose restart` 不会更新已创建容器的环境**（无域名方案把下面的 `compose.yml` 换成 `compose.ip-http.yml`）。Compose 应使用下面的 recreate；systemd 用 restart。整个实例的旧 Cookie/恢复 token/房间将全部失效，朋友重新登录建房：

```sh
docker compose --env-file deploy/.env -f deploy/compose.yml up -d --no-deps --force-recreate server
# 裸机替代：
sudo systemctl restart naifrog-server
```

- 升级源码先重新构建 release 镜像/发布目录，再切换并重启。备份环境文件和 Caddy TLS 数据到仅管理员可读的地方；没有房间存档可恢复。变更域名时同时更新 Origin、Caddy 站点与 DNS，并重启相应服务；已有对局会断。
- 本机验收脚本：[双浏览器联机](<scripts/online-smoke.mjs>)与[安全验收](<scripts/security-smoke.mjs>)。它们依赖“信任本机代理地址”来伪造不同的 X-Real-IP，安全脚本还会故意用完整小时的出站预算，**只能对本机一次性实例运行，绝不要对生产服务运行**。生产环境按下面的清单人工验收。
- 本机只交付配置与本机回环验收。**在自己的目标环境另做以下验收，尚未完成，不能宣称已上线或真机验证**（无域名方案跳过证书相关项，第 1 项改为“公网只开放那一个端口”，第 3 项的 Cookie 名为 `naifrog_session` 且没有 Secure，第 4 项用 `ws://IP:端口/api/ws`）：
  1. HTTP 自动跳转 HTTPS、证书域名正确且可续期；公网只有预定的 80/443，不能连接 18080。
  2. 未登录 `GET /api/config` 返回 `multiplayer: true, authenticated: false`；未登录获取 `/pkg/naifrog_duel_bg.wasm` 或 `/assets/processed/fighter_idle.webp` 拒绝。原稿路径不能下载。
  3. `POST /api/auth` 要求正确 Origin、`Content-Type: application/json` 与包含字符串字段 `access_key` 的 JSON 对象；错误口令/Origin 拒绝。正确 HTTPS 登录得到带 `__Host-` 前缀、HttpOnly、Secure、SameSite=Strict、Path=/ 的 Cookie，无 Domain 属性。
  4. 两个不同公网 IP 的朋友实测能同源 `wss://host/api/ws` 建房/加入、正确计额；伪造 X-Real-IP/XFF 不能改变服务器认定的来源。不要把凭据写进 curl 命令行或测试日志。
  5. 断线暂停/5 分钟恢复、当前轮新输入屏障、双方 10 秒重开票、旧 socket fencing、轮换重启全量失效；超限/预算耗尽时安全拒绝，而非无限资源增长。
  6. 浏览器缓存：同一浏览器第二次打开，开发者工具 Network 里 WASM 与贴图显示来自磁盘 / 内存缓存，服务器只收到 `/api/manifest` 和页面文件的 304；更新部署后再打开，只有内容变了的文件重新下载。经 Caddy 时响应头 `Cache-Control` 与服务器给出的一致（没有被改成 `no-store`）。

## 9. 安全边界：不是防 DDoS，也不是完整反作弊

上述配额、流式期限和容器限额用于**包含应用资源成本**，不保证拦住 DDoS、分布式机器人、合法高频连接、宏、恶意持有人/抢房/消耗预算或其它 grief 行为。攻击者即使未认证，仍可占用网络/TLS/代理处理；拿到口令更能合法消耗共享容量。权威服务器不会使所有输入自动可信，也不等于检测真人、封禁宏或公平赛事系统。

对不愿承担公网风险的小圈子，优先考虑上游防护/接入限额、仅允许朋友网络、私有隧道/VPN，并在真实拓扑中重新审核来源 IP 与精确信任链。默认 Caddy 不提供云 WAF 或 DDoS 清洗；不要把示例部署当作无限规模公开游戏站。任何上游方案都不能给受认证资源加共享缓存、公开原稿或放宽 arbitrary forwarded headers 的信任。
