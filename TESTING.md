# 奶蛙对打 · 验收记录

只记录本机实际执行过的结果。浏览器模拟触摸、CDP 注入事件都不等于实机；没有实机的项目列在文末清单里。

## 2026-10-26 开源准备基线

当前维护命令以本节和 CI 为准。下方旧章节是历次开发时的历史记录，测试数量、包体积和浏览器环境不代表当前基线；涉及 `artifacts/` 的历史脚本或截图也不随公开源码分发。

- `cargo test --locked --no-default-features --features "server client" --lib`：135 项通过（本地 Rust 1.98.1）。
- `node --test scripts/server.test.mjs scripts/network.test.mjs`：7 项通过；HTTP 测试使用独立临时素材，不依赖角色美术。
- `node scripts/check-assets.mjs`：完整素材包通过；12 套动作、字体及仅运行资源目录的约束均已检查。维护者说明 AI 生成来源并选择 MIT 后，新的公开候选包含相同的 2D 图集和重建所需的 12 张原图。
- 前一份独立源码候选已通过 135 项 Rust 测试、7 项 Node 测试、Windows `cargo check --all-targets` 与 WASM 游戏编译检查；新版保留相同的 Rust 源码和依赖输入，并加入已登记来源的素材。Rust 格式及服务器/客户端 clippy 通过。含素材候选的额外校验见 `docs/OPEN_SOURCE.md`；尚未在 GitHub runner、Docker 镜像或生产主机执行本轮验收。
- 含素材候选：系统临时目录全新安装 Node 依赖后，完整素材校验、重建、来源与运输摘要检查通过；Git 索引纳入 12 个原图和 21 个运行资源。该候选的桌面 `--smoke` 编译/运行通过，截图在 `docs/screenshots/game.png`；本地原图和既有运行资源的 41 个 SHA256 均未变。

## 历史环境与验收记录

Windows 11 / Rust 1.98.1 / Bevy 0.19.0（锁定依赖含 0.19.1）/ Node.js 24 / wasm-bindgen 0.2.129。桌面版 Bevy 使用 NVIDIA GeForce RTX 4070 Ti（Vulkan）；Chrome 153（headless，CDP），WebGL 渲染器 ANGLE Direct3D11（AMD Radeon 核显）。

## 本地模式（单机打人机 / 本地同屏双人 / 菜单）

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 47 项通过（战斗、输入、CPU、模式、摇杆与按钮布局、结果显示时长、状态行长度） |
| `cargo test --locked --no-default-features --features server --lib` | 71 项通过（另含协议与服务器）；此前修复关闭竞态后曾连续 55 次全部通过 |
| `cargo test --bin naifrog-duel` | 1 项通过（命令行模式解析） |
| `cargo clippy --all-targets --all-features -- -D warnings` | 通过；`cargo clippy --target wasm32-unknown-unknown --bin naifrog-duel` 也通过 |
| `.\scripts\build-web.ps1 -Release` | 通过，`naifrog_duel_bg.wasm` 39,687,240 字节（37.8 MiB，低于 64 MiB） |
| `cargo fmt --all --check` | 通过 |
| `node scripts/check-assets.mjs` | 10 套图集、透明通道、尺寸、留白、脚底基线、原图 SHA256 通过 |
| `node --test scripts/server.test.mjs` | 3 项通过（离线静态服务器路由，含 `/network.js`） |
| Windows 桌面实际启动 | 菜单、单机、本地、演示、单机换边五种截图均为本次运行生成 |
| 浏览器 1280×720、DPR 1 | `browser-smoke.mjs` 通过 |
| 浏览器 1180×820、DPR 2（平板尺寸） | `browser-smoke.mjs` 通过，画布 2360×1640，61 fps |

桌面截图（`artifacts/phase2/`，不纳入版本管理）：菜单、`P1 YOU` / `P2 CPU`、本地 `P1` / `P2`、演示、`--swap-sides` 换边后金色 `P1 YOU` 在右、蓝色 `P2 CPU` 在左——标识跟身份走而不是跟位置走。

浏览器探针每次都重新导航页面，并且只在本次导航之后的控制台里等待贴图就绪，不复用旧日志或旧截图。它依次验证：

1. 新页面停在模式菜单，Rust 侧为 `menu` 且暂停，菜单后面不显示触屏控件；离线服务器上“私人联机”按钮不可用。
2. 触屏点“本地同屏双人”：出现两个摇杆和 8 个圆形按钮（每人 4 个）。P1 摇杆右拖、P2 摇杆左拖同时走近；接着 P2 摇杆下拉防御、P1 点 LIGHT（三指同时按住），前方防御无削血；键盘 F 命中，按住不连发，R 重开回满血。
3. Esc 暂停（菜单出现），再按 Esc 继续。
4. 菜单中按数字 1 进入单机：不操作时 CPU 主动进攻，只有金色血条下降；暂停 2 秒内血量不变；继续后只有一个摇杆和 4 个按钮，全部属于你（P1）：摇杆右拖走近，再点右侧圆形 LIGHT 打中 CPU。
5. 模拟标签页隐藏（派发 `visibilitychange`）会暂停单机并打开菜单。

蓝色血条像素（DPR 1 / DPR 2）：本地初始 448 / 826，防御 448 / 826，命中 412 / 760，按住 412 / 760，重开 448 / 826；单机中 CPU 主动攻击使金色血条下降（本次 DPR 1 为 448 → 376；CPU 带随机性，每次数值不同），触屏轻拳使蓝色血条降到 412 / 760（扣 8 HP）。

高 DPI 注意：CDP 的 `deviceScaleFactor` 仿真会报告 `devicePixelRatio = 2`，但 ResizeObserver 的设备像素尺寸仍是 CSS 尺寸，导致 WASM 窗口被当成一半大（字体放大、触屏坐标错位）。真实高 DPI 设备上两者一致。因此 DPR 2 验收使用 `--force-device-scale-factor=2` 启动的独立 Chrome，并传 `native`；探针会检查画布像素 = CSS 尺寸 × DPR，不一致直接失败。CPU 软件光栅（SwiftShader）在 2360×1640 只有约 1 fps，探针要求 ≥ 30 fps，因此改用 GPU（ANGLE D3D11）。

CPU 单元测试覆盖：只输出合法按键且不改动战斗状态；从出生点接近并只在有效距离出招；会用所有招式但不一直防御或连按；不能对比感知延迟更新的事件做出反应；同种子可复现、不同种子不同；两个 CPU 能打完一局且双方都受伤。另用一次性探针（未保留在仓库）跑了 400 局随机对打，两名角色从未互相穿越；`--swap-sides` 用于换边截图。

## 重点回归

- 轻拳启动/有效/收招边界与动画年龄一致，首/末有效帧命中不会显示成错误阶段。
- 单次挥击只命中一次，前方防御无削血且能后退；背后命中不被格挡。
- 同帧互换、双 KO、固定回放、墙边推挤与防穿越。
- 倒地动画时钟独立于落地计时；KO 后继续落地并完成动画；99 秒准确结算。
- 短按、同帧松开再按、多指分工、重叠同键取消、失焦基线、重开抑制、固定帧补偿；同一玩家两只手柄拔掉一只不重复触发。
- 触屏布局（宽高比 1.25–2.4，覆盖平板到手机横屏）：圆形按钮互不接触、不压摇杆区和 REMATCH、都在本方半屏内，每个按钮中心命中自己；摇杆区放得下整个摇杆，静止提示完整显示在屏幕内；单机左摇杆右按钮，本地左右各半。摇杆左右走、上推跳、下拉防，斜推不会同时跳和防，拖出半径时中心跟随手指。画面绘制、命中测试和页面上报（`window.__naifrogTouch`，浏览器脚本据此瞄准）使用同一份布局数据。

## 复现

在项目根目录执行：

```powershell
cargo test --locked --no-default-features --lib
cargo test --locked --no-default-features --features server --lib
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
node --test scripts/server.test.mjs
node scripts/check-assets.mjs
cargo run --locked --bin naifrog-duel -- --smoke artifacts/phase2/native-cpu.png --mode cpu
cargo run --locked --bin naifrog-duel -- --smoke artifacts/phase2/native-cpu-swapped.png --mode cpu --swap-sides
.\scripts\build-web.ps1 -Release
node scripts/serve-web.mjs --host 127.0.0.1 --port 18080
```

浏览器自动化需要自己单独启动可调试的 Chromium（独立用户目录），再执行：

```powershell
# DPR 1：chrome --headless=new --remote-debugging-port=9223 --user-data-dir=<临时目录>
node scripts/browser-smoke.mjs 9223 http://127.0.0.1:18080
# DPR 2 平板尺寸：chrome --headless=new --use-angle=d3d11 --force-device-scale-factor=2 --window-size=1180,820 --remote-debugging-port=9224 --user-data-dir=<另一个临时目录>
node scripts/browser-smoke.mjs 9224 http://127.0.0.1:18080 1180 820 native
```

联机与安全验收需要先构建 release 服务器，并用环境变量启动自己的实例（口令用 32 字节随机数，不要写进命令行历史或日志）：

```powershell
cargo build --locked --release --no-default-features --features server --bin naifrog-server
# 实例 1（联机）：NAIFROG_BIND=127.0.0.1:18090  NAIFROG_ORIGIN=http://127.0.0.1:18090  NAIFROG_ASSET_ROOT=<项目根>  NAIFROG_PROXY_IPS=127.0.0.1  NAIFROG_ACCESS_KEY=<口令>
node scripts/online-smoke.mjs 9223 9224 http://127.0.0.1:18090 10.20.0.2 10.20.0.3
# 实例 2（安全，一次性）：同上换一个全新端口（18091/18092/18093 的预算都已烧完），另加 NAIFROG_MAX_ROOMS=2  NAIFROG_EGRESS_MB_PER_HOUR=64；最后一步故意耗尽预算，跑完即停止
node scripts/security-smoke.mjs http://127.0.0.1:<新端口>
# 手柄（模拟标准手柄，复用实例 1 或另起一个）：
node scripts/gamepad-smoke.mjs 9223 http://127.0.0.1:18090
```

三个脚本都从 `NAIFROG_ACCESS_KEY` 环境变量读取口令。WASM 每 IP 每分钟只允许 6 次下载，一分钟内重跑联机脚本时换一组 IP（手柄脚本每次自动换）。每个新浏览器上下文完整载入约 13.9 MB（WASM 以 gzip 传输约 6.0 MB，贴图为无损 WebP 约 7.9 MB），默认 512 MiB/小时预算约够 38 次；同一上下文再次打开时游戏文件走浏览器缓存；连续重跑会耗尽预算，贴图返回 503、页面停在载入画面——测试实例可设 `NAIFROG_EGRESS_MB_PER_HOUR=4096`。

[浏览器探针](<scripts/browser-smoke.mjs>)通过 CDP 注入真实网页触摸/键盘事件并检查截图血条，不是只测试输入函数。原始图片由 [素材检查脚本](<scripts/check-assets.mjs>)比对 [清单](<assets/processed/game-assets.json>)中的 SHA256。

## 私人联机（本机回环：双浏览器 + 安全）

release 服务器（`cargo build --locked --release --no-default-features --features server --bin naifrog-server`）绑定 127.0.0.1，`NAIFROG_ORIGIN=http://127.0.0.1:<端口>`，`NAIFROG_PROXY_IPS=127.0.0.1`：脚本借这个受信代理地址给每个客户端发送不同的 X-Real-IP，模拟不同设备的来源地址。访问口令是 32 字节随机数，只经环境变量传给服务器和脚本，没有出现在命令行、网址、页面存储或日志里。以下全部在本机回环上完成，**没有真实网络延迟、HTTPS/WSS、Caddy 或第二台物理设备**。

### 双客户端（[联机验收脚本](<scripts/online-smoke.mjs>)，全部通过，约 40 秒）

两个独立的 Chrome（CDP，ANGLE D3D11），每个客户端都在全新的隔离浏览器上下文里（cookie、sessionStorage 互不相通）：PC 1280×720 用键盘；平板尺寸 1180×820、真实 DPR 2、CDP 触摸仿真（不是真平板）。每一步读取本次页面的快照（`window.__naifrogState`，只读、不含凭据），不复用旧日志或旧截图。

| 步骤 | 实测 |
|---|---|
| 登录前 | `/pkg/naifrog_duel.js`、`.wasm`、`/assets/processed/game-assets.json` 均 401，游戏未启动；错误口令显示“口令不正确或登录已过期” |
| 登录后 | 页面脚本读不到 cookie（HttpOnly），口令框已清空，localStorage / sessionStorage 为空 |
| 建房 / 等待 | PC 是 P1（金色），8 位匹配码；对方加入前服务器不模拟（tick 停在 0），显示“等待对方” |
| 加入 | 平板输入 `XXXX-XXXX` 匹配码，成为 P2（蓝色）；两端标签 `YOU` / `RIVAL` 各自正确；平板显示一个蓝色摇杆和 4 个按钮，PC（无触摸）不显示触屏控件 |
| 移动 | PC 的 D 只移动 P1（-235 → -100），平板摇杆左拖只移动 P2（235 → 105）；双方互看一致 |
| 攻击 / 防御 | PC 轻拳：两端 P2 都是 100 → 92；平板摇杆下拉防御时再挨轻拳仍是 92（前方防御无削血） |
| 跳跃 / 冲刺 | 平板圆形 JUMP 按钮：PC 端看到 P2 离地；PC 冲刺（H）：平板端看到 `Attack:Dash` |
| 一致性 | 停稳后两端 HP 与坐标完全相同（HP 100 / 78） |
| 输入超时 | PC 按住 A 时停止上传：约 250 ms 内服务器把 P1 按键清零、P1 停住；恢复上传后继续移动 |
| 断线暂停 / 恢复 | PC 离开页面：平板显示“等待对方 / 断线暂停”，tick 冻结，平板按键无效；PC 回到页面后自动以原席位恢复（P1、同一房间、HP 不变），模拟继续 |
| 旧连接失效 | 同一登录的第二个标签页用恢复令牌接管席位：原页面显示“这个席位已在另一个窗口或标签页打开，本页已断开；刷新本页可取回”且不自动抢回；原页面刷新后取回席位，第二个标签页收到 `connection replaced` 并断开 |
| 成对重开 | PC 按 R：平板看到投票，回合不变；平板点 REMATCH 后回合 0 → 1、双方满血 |
| 结果 | PC 重拳打到 KO：两端都是 `Winner(0)`，PC 显示 “YOU WIN!”，平板显示 “P1 RIVAL WINS!” |
| 离开 | 平板在菜单选“单机打人机”：立即进入 CPU 模式；PC 显示“房间已关闭（对方离开或超时）” |
| 换房 | 平板建新房（P1），PC 用新匹配码加入成为蓝色 P2；换房期间 PC 端只出现过“新房间 / P2”这一种状态 |
| 控制台 | 两端都没有错误 |

截图在 `artifacts/online-*.png`（不纳入版本管理）：`pc-lobby`、`pc-joined` / `tablet-joined`、`tablet-rival-disconnected`、`pc-replaced`、`pc-ko` / `tablet-ko`、`pc-switched` / `tablet-switched`。

### 安全（[安全验收脚本](<scripts/security-smoke.mjs>)，全部通过，约 10 秒）

每次用一个一次性的服务器实例（`NAIFROG_MAX_ROOMS=2`、`NAIFROG_EGRESS_MB_PER_HOUR=64`），因为最后一步会故意用完整小时的出站预算。脚本直接发原始 HTTP 请求和原始 WebSocket 帧，每个用例使用自己的来源地址。

| 类别 | 实测 |
|---|---|
| 来源地址 | 受信代理不带 X-Real-IP、带两个 X-Real-IP、格式错误：401；只有一个合法值才接受 |
| 文件 | 只有 `/`、`index.html`、`main.js`、`network.js`、`style.css` 公开；未登录取 `/pkg/*`、`/assets/processed/*`、`/api/ws` 均 401；原始素材、`Cargo.toml`、`.env`、`DEPLOY.md`、服务器程序等 404；登录后 5 种路径穿越写法均 404；`POST /` 405；响应带 CSP（`frame-ancestors 'none'`、`connect-src 'self'`）、nosniff、`X-Frame-Options: DENY`、`no-referrer`、`Cache-Control: private, no-store` |
| 登录 | 缺 Origin、外站 Origin、相似域名 Origin、`text/plain`、多余字段、2 KB 请求体、错误口令：全部 401；正确登录得到 `naifrog_session`（HttpOnly、SameSite=Strict、Path=/、有 Max-Age；本机 HTTP 下无 Secure，HTTPS 下改为 `__Host-` 前缀 + Secure，见单元测试） |
| 暴力尝试 | 同一 IP 第 11 次错误口令 429，之后正确口令也 429 |
| 会话 | 把 cookie 拿到另一个 IP 使用：未登录、文件 401、WebSocket 401；伪造 cookie：401 |
| WebSocket 握手 | 无 cookie、无 Origin、外站 Origin、相似域名 Origin：401 |
| 首条消息 | 不是建房 / 加入、坏 JSON、自选席位、多余字段、二进制帧：`request denied`；旧协议版本：`client outdated, reload the page`；不存在的房间：`room unavailable`；超过 1024 字节：直接断开，不给席位；5 秒不发消息：断开 |
| 伪造输入 | 未知按键位、对“只能按住”的键发按下边沿、附带 `hp` / `player` / `x` 字段、未来回合、负序号、重放序号、房间内再建房 / 再加入、伪造 `winner` 消息、带多余字段的 leave、二进制帧、一次发 400 条：都以 `request denied` 断开；1.5 KB 帧属于协议错误，直接断开。每次之后原玩家都能用恢复令牌回到原席位，双方 HP 与坐标保持出生状态不变 |
| 跨会话 | 另一个登录拿 P2 的恢复令牌加入、或不带令牌加入满房：`room unavailable`；真 P2 不受影响 |
| 同席位新连接 | 旧 socket 收到 `connection replaced` 并断开 |
| 并发与频率 | 每个登录最多 2 个 WebSocket、每个 IP 最多 4 个（第 3 / 第 5 个 429）；每 IP 每分钟第 31 次握手 429；每 IP 每分钟第 121 次文件请求 429、第 7 次 WASM 请求 429 |
| 房间上限 | 上限 2 时第 3 个房间 `server full`；有房间离开后可以再建 |
| 出站预算 | 未登录流量在约 4 MiB（64 MiB 的 1/16）处被拒（503），登录后同一文件仍可下载；再下完整 WASM 后第二次 WASM 503；用完后：进行中的对局收到 `server transfer budget exhausted` 并断开，页面和游戏文件 503，新建房间被拒并说明原因 |
| 日志 | 服务器只在启动时打印一行（监听地址、Origin、上限），所有实例的 stdout / stderr 都不含口令、cookie 或 64 位十六进制令牌 |

修复：服务器以前在发出拒绝原因和 Close 帧后立即丢弃连接；如果客户端还有未读完的数据（例如洪水消息），操作系统会发 TCP 重置，客户端可能收不到拒绝原因。`raw_ws_rejects_wrong_version_oversized_frames_and_message_flood` 因此在完整测试中约 1/15 失败（本次修复前 16 次中 2 次）。现在 Close 后最多再读 1 秒 / 64 KiB 并丢弃（tungstenite 在 Close 之后不再回复 Ping），修复后完整服务器测试连续 55 次全部通过，安全脚本中洪水用例也能收到 `request denied`。

单元测试另外覆盖但没有在本机端到端重放的：全局 32 个 WebSocket、256 个会话、房间 12 小时寿命、5 分钟断线宽限过期、HTTPS 下的 `__Host-` cookie。

## 手柄（网页，模拟标准手柄）

问题：网页的模式菜单、房间面板和帮助面板只响应鼠标、键盘、触屏；面板获得焦点后画布失焦，winit 报告窗口失焦，游戏忽略全部手柄输入。所以只用手柄无法选模式、无法建房，按 Select 暂停后手柄也无法继续。

修复：[页面脚本](<web/main.js>)每帧轮询 `navigator.getGamepads()`：面板打开时十字键 / 左摇杆移动焦点，A / Start 确认，B / Select 返回；联机对局中 Select 打开房间面板，焦点默认在“返回画面”而不是“离开房间”。按钮在松开时生效，且只认在同一面板上按下的键，画布重新获得焦点后不会把这次按键漏进游戏。

[手柄验收脚本](<scripts/gamepad-smoke.mjs>)用 CDP 把一个标准映射的模拟手柄注入 `navigator.getGamepads()`（页面面板与 WASM 中的 gilrs 都读它），PC 一方全程只用手柄（登录口令仍用键盘输入），另一个隔离上下文用键盘做对手。修复前第一步即失败（十字键下移焦点不动）；修复后全部通过：

| 项目 | 结果 |
|---|---|
| 模式菜单 | 十字键上下移动焦点，A 进入私人联机；在房间面板移到“创建房间”，A 建房，画布重新获得焦点 |
| 联机对战 | 手柄只移动自己的 P1（对手屏幕上 X 增加 >50 单位），P2 不动 |
| 房间面板 | 对局中 Select 打开，B 关闭并回到画面，之后手柄仍能移动 P1 |
| 切回单机 | Select → “模式菜单” → 单机打人机，全程手柄；对方看到“房间已关闭” |
| 暂停 / 继续 | Select 暂停（游戏内）并打开菜单，再按 Select 继续；Select 暂停后按 B 也继续；400 ms 后仍在运行，没有被同一次按键重新打开 |
| 打 CPU | X 轻拳命中 CPU，蓝色血条 448 → 412 像素 |

问题二（“手柄长时间不操作后十字键 / XYAB 全部无效”）：实际是回合已经结束（画面显示 WINS / DRAW），结果出现后战斗不再接受输入，只有 R、Start 和画面上的 REMATCH 能开下一局，手柄 A 没有反应。

修复：结果显示满 60 个固定帧（约 1 秒）后，新按下的手柄 A 在松开时重开（联机时算作本方的重开确认）。约 1 秒内按下的 A、以及从结果出现前一直按住的 A 都不算，免得连按跳跃把结果跳过；在松开时才生效，是因为按下时就重开会把这次 A 作为跳跃带进新回合。“已显示多久”由 `Arena::ticks_since_outcome` 从冻结的倒计时推算，联机快照上同样成立。结果横幅改为 `R / START / PAD A: REMATCH`（联机 `BOTH: R / START / PAD A`）；联机的长版本在 1180 px 宽截图里被右上角页面按钮挡住，所以缩短了，并加单元测试限制状态行不超过 41 个字符。

| 项目 | 结果 |
|---|---|
| 联机：对手 KO 手柄方 | 结果出现后第 13 帧松开的 A 不投票；结果新出现时按下、按过 70 帧才松开的 A 不投票；对手先按 R 后，新按的 A 作为第二票开下一局（回合 0 → 1），新回合前 500 ms 里 P1 高度一直为 0（没有漏进跳跃） |
| 单机：CPU KO 手柄方 | 手柄 A 从 KO 前一直按到 KO 后 1.5 秒再松开，结果保持（金色 0 / 蓝色 412 像素不变）；再按一次 A，两条血条回满（448 / 448），开始下一局 |

截图只能看到血条，无法在单机里精确计时“结果出现后 1 秒内”的短按，所以短按只在联机里按快照帧数验证；两种模式走同一段代码。

同一修改后 `browser-smoke.mjs`（键盘 / 触屏）与 `online-smoke.mjs`（双客户端）重新运行均通过。模拟手柄只验证页面与游戏的代码路径，**不等于真实手柄或驱动**。

## 触屏：虚拟摇杆 + 圆形按钮（网页，CDP 触摸仿真）

问题：平板上显示的是和电脑一样的一排长方形按键（左、右、跳、防 / 轻拳、重拳、冲刺），压在画面底部一块深色面板上，不像触屏游戏。

修改：触屏设备上改为半透明的浮动摇杆 + 圆形按钮，深色底板去掉，画面完整可见。手指在左半屏（状态栏以下）按下的位置就是摇杆中心；水平偏移超过半径 30% 移动，向上 / 向下超过 55%（且比水平方向更明显）跳跃 / 防御；拖出半径时中心跟着手指走。四个圆形按钮按手柄排成菱形（下 JUMP、左 LIGHT、上 HEAVY、右 DASH），大小按屏幕高度计算，任何宽高比下都是圆的。颜色跟座位：P1 金色、P2 蓝色（联机加入者是蓝色）。按下的按钮与正在拖动的摇杆变亮，其余保持淡色。是否显示由页面判断（`navigator.maxTouchPoints > 0` 或粗指针）；电脑浏览器和桌面版不显示，触屏电脑在第一次触摸后显示。顶部提示在触屏上改为摇杆 / 按钮说明，不再列键盘键位。

| 项目 | 结果 |
|---|---|
| `browser-smoke.mjs` 1280×720 DPR 1、1180×820 真实 DPR 2 | 通过（见上文步骤 1、2、4）；DPR 2 下连续两次通过 |
| `online-smoke.mjs` | 通过：平板用摇杆左拖移动、下拉防御，圆形 JUMP 跳跃，REMATCH 确认；PC 不显示触屏控件 |
| `gamepad-smoke.mjs` | 通过（PC 不显示触屏控件，手柄流程不受影响） |
| Windows 桌面 `--smoke` | 画面底部不再有深色面板，也没有触屏控件 |

调试过程：DPR 2 下第一次运行时“单机触屏轻拳命中 CPU”超时。探针按住 LIGHT 截图，确认按钮点亮、P1 出拳、攻击框伸出，命中测试正确；失败原因是 CPU 当时跳开或不在距离内，而 DPR 2 截图较慢，单次点按的间隔里 CPU 已把 P1 打倒。脚本改为每轮先用摇杆走近、再连点三次，之后 DPR 2 连续两次、DPR 1 一次全部通过。

## 没有域名：http://IP:端口（本机用非回环地址模拟）

需求：服务器没有域名，私人联机不能只支持 HTTPS。

修改：
- 服务器：设置 `NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP=1` 后，接受非本机地址上的 `http://` Origin（例如 `http://203.0.113.10:18080`）。这个开关以前只算开发豁免。不设置时仍拒绝启动并说明原因；设置后启动日志那一行末尾带 `UNENCRYPTED public HTTP` 警告。`:80`、路径和末尾斜杠仍被拒绝。
- 页面：不是安全来源时（`isSecureContext` 为假，也就是非本机的 http:// 页面），登录框下显示明文警告。帮助文字、连接失败提示和离线提示不再只说 HTTPS。部分浏览器在 http:// 页面拒绝 `navigator.getGamepads()`，所以页面轮询改为捕获这个异常；WASM 里的 gilrs 本身已经容错。
- 部署：新增 [无域名 Compose](<deploy/compose.ip-http.yml>) 和 [环境示例](<deploy/env.ip-http.example>)，步骤见 DEPLOY.md 第 6 节。

本机验证，不是公网主机：
- release 服务器绑定 127.0.0.1:18103，`NAIFROG_ORIGIN=http://naifrog.test:18103`，开关为 1，`NAIFROG_PROXY_IPS=127.0.0.1`。
- 两个 Chrome 加 `--host-resolver-rules="MAP naifrog.test 127.0.0.1"`，把这个名字解析到本机。页面因此是非回环的 http:// 地址，不是安全来源，和朋友打开 `http://公网IP:端口` 时的浏览器条件相同。
- 本机有系统 HTTP 代理，Chrome 会把 naifrog.test 发给代理并得到 502，所以测试用的 Chrome 另加 `--no-proxy-server`。

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --features server --lib` | 71 项通过。新增的无域名配置测试覆盖：IPv4 / IPv6 地址都需要开关；端口 80 写成 `http://IP`，写 `:80` 或带末尾斜杠被拒绝；绑定 0.0.0.0 时回环 Origin 也算公网；`https://IP` 不需要开关，cookie 为 `__Host-` 前缀加 Secure |
| 不设开关启动 | 退出码 1：`plain HTTP on a public address needs NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP=1` |
| 设开关启动 | 只有一行日志，末尾带 UNENCRYPTED 警告，不含口令 |
| `online-smoke.mjs`（`http://naifrog.test:18103`） | 全部通过：`isSecureContext` 为假，明文警告可见；登录前受保护文件 401；cookie 为 HttpOnly；WebSocket 走 `ws://`；建房、加入、移动、攻防、断线恢复、旧连接失效、重开、KO、离开、换房全部通过 |
| 登录的 Set-Cookie | `naifrog_session=…; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200`，没有 Secure（浏览器在 http 页面上不会保存 Secure cookie） |
| 警告只在需要时出现 | 同一服务器：`http://127.0.0.1:18103` 是安全来源，警告隐藏；`http://naifrog.test:18103` 显示警告 |
| `gamepad-smoke.mjs`（同一 http 地址） | 连续两次通过。模拟手柄替换了 `getGamepads()`，所以只能说明页面和游戏的手柄路径在 http 页面上正常，不能说明真实浏览器在 http 页面上提供手柄 API |
| `browser-smoke.mjs`（离线静态服务器） | 通过 |
| clippy（本机 / server / wasm32）、fmt | 通过 |

调试过程：第一次运行 `gamepad-smoke.mjs` 时，“手柄轻拳命中 CPU”超时。截图显示 CPU 在 15 秒内先把原地不动的 P1 打倒了，和触屏 DPR 2 的情况相同；重跑一次就通过，说明是时序问题。脚本改为每轮先按十字键向右走近，再连按 X 三次，超时放宽到 20 秒，之后连续两次通过。

没有验证：真实公网主机、Docker、真实浏览器在 http 页面上是否提供手柄 API、IP 证书方案。

## 桌面版联机（`--server http://IP:端口`）

需求：电脑上用手柄玩的桌面 GUI 也要能联机，和平板浏览器连同一个服务器、进同一个房间。

修改：
- 新的 `client` 功能（`game` 默认包含，wasm32 不编译）：一个后台线程负责 HTTP 登录和 WebSocket，协议与 `web/network.js` 相同。登录时 `Origin` 头写服务器的地址，cookie 只在内存里。掉线后用恢复令牌重连（最多 80 次，间隔 1–4 秒，约 5 分钟，与服务器保留断线席位的时间一致）。只支持 `http://`，`https://` 地址会被拒绝并提示用浏览器。
- 桌面菜单：带 `--server`（或 `NAIFROG_SERVER`）时多出“3 PRIVATE ONLINE”；进入后依次是 ACCESS KEY（键盘输入，显示为 `*`）→ CREATE ROOM / JOIN ROOM → 入座后菜单自动关闭，顶部状态行显示匹配码和对方是否在线。在文字行上，字母、数字和空格只输入文字，不触发 W/S 移动或数字快捷键；方向键、手柄十字键和 Enter / A 照常可用。
- 联机时 Esc / Select 打开菜单但不暂停；状态行在菜单打开时不显示 PAUSED。出错或房间关闭时菜单自动打开，显示原因。

| 项目 | 结果 |
|---|---|
| `cargo test --no-default-features --features "server client" --lib` | 78 项通过。新增测试让两个桌面客户端连本机真实服务器：错误口令被拒；未登录建房被拒；登录、建房、用小写带横线的匹配码加入；双方入座、发中立输入后，P1 按右在服务器上前进；重开投票对方可见；离开后对方收到“房间已关闭” |
| 先登录再进入联机 | 修复前“进入”会把登录结果（如 WRONG KEY）覆盖成“请输入口令”，看不到失败原因；测试先登录再进入，等 100 ms 后仍是登录结果 |
| 菜单单元测试 | 每种模式和联机状态的菜单最多 5 行，行在面板内、互不重叠、不压页脚，点按命中对应行；联机行按登录 / 入座状态变化，单机、本地、退出始终可选 |
| 对已部署的服务器（`http://IP:端口`，同一台电脑上两个客户端） | 登录、建房、加入（第二个客户端为 P2）、P1 在服务器上前进、离开，对方收到“房间已关闭”，全部通过 |
| 桌面 `--smoke --mode menu --server …` 截图 | 菜单显示 1 VS CPU、2 LOCAL 2 PLAYERS、3 PRIVATE ONLINE、QUIT，页脚显示服务器地址 |
| clippy（本机 / wasm32）、fmt | 通过 |

没有验证：桌面菜单里的键盘输入、手柄选择和联机对局都没有人工操作过（没有自动驱动桌面窗口）；没有真手柄；没有电脑与平板跨设备对局。

## 房间保留靠心跳；顶部 HUD 重做

需求：“房间留存时间太短，我一直在等人，应该有 WebSocket ping 的事件才行”；顶部 UI 太丑。

原因：服务器本来就每 10 秒发一次 WebSocket ping，10 秒内没有 pong 就断开连接；但房间过期用的是固定时钟，与心跳无关：没开打的房间 5 分钟关闭，断线席位只留 30 秒，房间最长 1 小时。服务器日志不记录房间事件，用户遇到的是哪一条无法确认；截图（打过一局后提示“对方离开或超时”）更像 30 秒断线宽限，例如平板切出去或重新载入页面。

修改：
- 房间只要有一方的连接还在回应心跳就一直保留，最长 12 小时（与登录会话相同）；断线的席位保留 5 分钟。去掉“未开打 5 分钟关闭”。
- 网页和桌面客户端的自动恢复延长到约 5 分钟（80 次，间隔 1–4 秒）。
- HUD：去掉顶部深色横条，改成渐变阴影；血条加边框、圆角和红色掉血残影，掉血一侧靠近倒计时；名字在外侧、血量数字在内侧；倒计时放进带框的方块；状态行改成居中的胶囊；REMATCH 下移；键盘 / 手柄提示移到屏幕底部（触屏时仍在状态行下方，底部留给摇杆和按钮）。
- 背景图按窗口比例放大铺满，宽屏左右、4:3 平板上下都不再露黑边；放大以地面线为基准，角色脚下位置不变。
- 浏览器探针和手柄探针里血条的采样区域跟着新位置改。
- 桌面版加 `--size 宽x高` 参数，用来截不同比例的窗口。

| 项目 | 结果 |
|---|---|
| `cargo test --no-default-features --lib` | 49 项通过。新增背景测试：4:3 到 21:9 都盖住整个视野和震屏余量，地面线不动 |
| `--features server --lib` | 76 项通过。房间测试改为：只建房 3 小时仍在；创建者断线后 5 分钟减 1 秒仍在、到 5 分钟清理；双方在线的房间 12 小时减 1 秒仍在、到 12 小时清理 |
| `--features "server client" --lib` | 79 项通过 |
| 桌面截图 1280×720、1920×912（宽屏）、1024×768（4:3） | 新 HUD 正常；宽屏和 4:3 没有黑边。用探针的采样代码量截图：P1 84 HP 时金色 546 像素，满血 651 像素 |
| clippy（本机 / 全功能 / wasm32）、fmt | 通过 |
| 已部署的服务器（`http://IP:端口`），两个桌面客户端 | A 建房后空等 400 秒（旧版 5 分钟就会关房），期间一直是“等待对方”；之后 B 加入成为 P2，P1 在服务器上前进，A 离开后 B 收到“房间已关闭” |
| 掉血残影方向 | 临时让残影不消退，截 CPU 对 CPU 的图：P1 92 HP、P2 76 HP 时红色都紧贴倒计时一侧，长度对应扣掉的血；截完已改回 |

没有验证：新 HUD 的浏览器探针没有重跑（需要单独启动可调试的 Chromium）；触屏布局下提示行的位置没有截图；真平板、真手柄、平板切后台再回来的恢复都需要人工按下面的清单确认。

## 跳跃躲奶泡调参（2026-10-03）

问题：用户反馈“是不是滞空的时间太短了？我这个不好躲啊”。实测滞空不是瓶颈（42 帧），瓶颈是奶泡判定框（离地 70–140）与跳跃弧线配合太紧：原地跳只有 7 帧窗口，联机还要再扣网络延迟和快照插值。

修改（分两步、每步只改一个参数并用测试实测）：

1. 奶泡判定框下移：`SHOT_BOTTOM`/`SHOT_TOP` 70/140 → 50/110。原地跳窗口 7 → 11 帧（[37,47]），仍差 1 帧不到 12 帧目标。
2. 起跳速度 `vy` 18000 → 19000（重力 850 不变）。原地跳窗口 [34,47] = **14 帧**，向前跳 [11,45] = **35 帧**，滞空 42 → 43 帧（上限 48，新增 `a_jump_stays_airborne_at_most_48_ticks` 保护），顶点约 203 单位。

奶泡贴图位置由 `main.rs` 按 `(SHOT_BOTTOM + SHOT_TOP) / 2` 自动摆放，无需改渲染。仍保证：站着不动挨奶泡扣 10、防御削 3、第 0 tick 就起跳仍被打中、窗口是一段连续区间。

| 项目 | 结果 |
|---|---|
| `cargo test --no-default-features --lib` | 62 通过（原地跳 ≥12、向前跳 ≥24 写成断言；CPU 跳过 / 防御奶泡的测试照常通过） |
| 改参数前先确认 | 两个窗口断言在旧参数上失败（原地跳 [39,45]=7 帧），随后调参转绿 |

没有验证：真平板 / 真手柄上“向前跳躲奶泡”是否顺手；真实网络延迟下 14 帧窗口的手感（联机时还要扣约 50 ms 插值）。

## 开场倒数与结算横幅（2026-10-03）

问题：开局即打，没有 ROUND/FIGHT 提示；KO 只有状态行小字，没有格斗游戏常见的居中播报。

修改：

- `combat` 加 90 帧开场倒数（`Arena::intro`，重开后重新倒数）：期间输入无效、99 秒计时器不走，倒数结束的那一帧才开始计时。`ticks_since_outcome` 相应扣掉开场帧数（否则“结果显示了多久”会多算 90 帧，手柄 A 的重开门槛会被推迟）。
- 快照多了 `intro` 字段，协议版本 2 → 3（旧页面加入会提示刷新）；`web/network.js`、`security-smoke`、`online-smoke` 的裸 WebSocket 同步改为 3。
- 画布新增居中横幅（`ui::banner` 纯函数，只在画布上画、不参与判定）：开场先 `ROUND 1`（约 1 秒）再 `FIGHT!`（约 0.5 秒），结算显示 `K.O.`（击倒）/ `TIME OVER`（时间到）/ `DRAW`（平局），获胜方满血显示 `PERFECT!`。位置在屏幕 27%–40% 高度，不挡 REMATCH 按钮与状态行；菜单模式隐藏。单机、本地、联机同一份快照驱动。
- 服务器单元测试与 raw-ws 测试改为先度过开场再断言移动 / 命中：单元测试用恒定时钟快进（不触发 250 ms 输入超时），raw-ws 测试改成每读一帧就补发一次按键（真实客户端 60 Hz 上传的节奏），直到对局真正开始且 P1 实际移动。
- 浏览器脚本适配开场：`browser-smoke` 在进入本地模式和每次重开后各等 2 秒；`online-smoke`、`gamepad-smoke` 等快照 `arena.intro === 0` 后才做移动断言；`gamepad-smoke` 的结果显示帧数公式减去 90。

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 64 通过（开场输入无效 / 计时不走 / 下一帧开始计时；横幅 ROUND 1→FIGHT!→K.O./TIME OVER/DRAW/PERFECT! 映射） |
| `cargo test --locked --no-default-features --features server --lib` | 91 通过 |
| `cargo test --locked --no-default-features --features "server client" --lib` | 94 通过（`/api/config` 期望 protocol 3） |
| clippy（全功能 / wasm32）、fmt、`check-assets` | 通过 |
| `.\scripts\build-web.ps1 -Release` | 通过，`naifrog_duel_bg.wasm` 39,766,259 字节 |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器，61 fps） | 通过：防御削血 521→511、键盘轻拳 521→469、按住不连发、重开回满 521、键盘奶泡 521→469、触屏奶泡 521→469、CPU 模式触屏轻拳命中；`local-ready` 截图（开场倒数中）在横幅区有 3555 个白色文字像素（改动前该区域无文字） |
| `online-smoke.mjs`（release 服务器 18090，PC + 平板尺寸触屏） | 连续两次通过：移动、攻防、奶泡、断线恢复、旧连接失效、成对重开（回合 0→1）、KO（`Winner(0)`，86 秒剩时）、离开、换房 |
| `gamepad-smoke.mjs` | 通过：修掉开场竞态后，早期 A 不投票（结果显示第 13 帧松开）、按住跨过 1 秒门槛不投票、新按 A 作为第二票开下一局且新回合无跳跃；CPU 局手柄轻拳 521→479 |
| `security-smoke.mjs`（一次性实例，MAX_ROOMS=2 / 64 MiB 每小时） | PASS：协议 3 裸帧、伪造输入、跨会话、限流、出站预算全部符合预期 |

没有验证：真平板 / 真手柄上的开场倒数节奏；联机真实延迟下 1.5 秒开场是否足够（服务器权威，两边一致）。

## 三局两胜整场比赛（2026-10-03）

问题：一局定胜负，没有局间节奏；重开重置的到底是“一局”还是“一场”也没有说清。

修改：

- `combat` 加整场比赛状态：`Arena::game`（0 起第几局）、`wins`（每边胜局数）、`match_outcome`（整场结果）。一局结算的那一帧点亮胜局标记；先赢两局整场获胜。平局（双 KO 或时间到同血量）不点亮标记。结果展示 120 帧（约 2 秒）后自动进入下一局：`next_round()` 重置血量、位置、奶泡与计时，只保留局数与胜局；下一局照常走 90 帧开场倒数（横幅 `ROUND 2`，能分出整场胜负的一局显示 `FINAL ROUND`）。打满 5 局仍无人两胜时多胜者获胜，否则整场平局；整场结果一直停留，只有重开能过去。`restart()` 重置整场。
- 快照多了 `game` / `wins` / `match_outcome` 字段（直接在 `Arena` 上，随快照走），协议版本 3 → 4；`web/network.js`、`security-smoke`、`online-smoke` 裸 WebSocket、服务器 `/api/config` 测试同步改为 4。客户端按 `Arena::game` 变化识别“新的一局”，重置渲染插值基线；R / Start / REMATCH 与联机双票重开走房间回合计数 + `arena.restart()`，重置的都是整场比赛。
- 血条下方每边两颗胜局标记，赢一局点亮一颗（本地渲染，无新协议）。CPU 演示测试改为打完整场（逐局限时，最终必有两胜或整场平局）。
- `gamepad-smoke` 的 CPU 局步骤按新流程改写：局内结果只停留约 2 秒，陈旧 A 松开不再以“横幅还在”判读，改为在 2 秒窗口内截图断言不重开，再断言下一局自动开始、第二局 CPU 夺冠后整场结果停留、新按 A 重置整场回满血。
- 过程中发现并修复：胜局标记查询与摇杆查询同为 `BackgroundColor` 写入且过滤器无法证明不相交，Bevy 以 B0001 拒绝启动（库测试不构建 UI 调度所以没拦住；桌面截图冒烟与 browser-smoke 各红一次后修复，029d54c）。

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 68 通过（两胜成场 / 平局不点亮 / 5 局多数胜否则整场平局 / 横幅 ROUND 3 与 FINAL ROUND 判定；重开 == 默认整场） |
| `cargo test --locked --no-default-features --features server --lib` | 95 通过 |
| `cargo test --locked --no-default-features --features "server client" --lib` | 98 通过（`/api/config` 期望 protocol 4） |
| `cargo test --locked --bin naifrog-duel`；`cargo run --bin naifrog-duel -- --smoke artifacts/smoke.png --mode cpu` | 2 通过；截图冒烟先红（B0001，与 browser-smoke 同时复现）后绿 |
| clippy（全功能 / wasm32）、fmt、`check-assets` | 通过 |
| `.\scripts\build-web.ps1 -Release` | 通过，`naifrog_duel_bg.wasm` 39,778,932 字节 |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器，61 fps） | 通过：防御削血 521→511、键盘轻拳 521→469、按住不连发、重开回满 521、键盘奶泡 521→469、CPU 模式触屏轻拳命中（血条像素与上一节一致，胜局标记不影响测量） |
| `online-smoke.mjs`（release 服务器 18090，协议 4，PC + 平板尺寸触屏） | 通过：移动、攻防、奶泡、断线恢复、旧连接失效、成对重开（回合 0→1，整场重置）、KO（`Winner(0)`，86 秒剩时）、离开、换房 |
| `gamepad-smoke.mjs`（协议 4） | 通过：早期 A 不投票（第 14 帧松开）、按住跨过门槛不投票、新按 A 作为第二票开新局且无跳跃；CPU 局轻拳 521→438、KO 后陈旧 A 在 2 秒窗口内松开不重开（0/438）、下一局自动开始回满 521/521、第二局 CPU 满血夺冠整场停留（0/521）、新按 A 重置整场回满 521/521 |
| `security-smoke.mjs`（一次性实例 18092，MAX_ROOMS=2 / 64 MiB 每小时） | PASS：协议 4 裸帧、伪造输入、跨会话、限流、出站预算全部符合预期 |

没有验证：真平板 / 真手柄上的局间节奏与胜局标记观感；联机真实延迟下 2 秒结果展示是否足够。

## 玩家名字与开场对阵画面（2026-10-03）

问题：联机双方只能靠金 / 蓝色分辨身份；第一局上来就打，没有“对阵”的节奏感；桌面版带不上名字。

修改：

- `combat`：第一局（含整场重开后）在 90 帧倒数前多 120 帧（2 秒）开场对阵画面（`VS_TICKS`），期间输入无效、99 秒计时器不走；`Arena::ticks_since_outcome` 的基线按“是否第一局”计入这 120 帧，手柄重开判定与自动进入下一局的计时不再提前 2 秒。横幅在对阵阶段不显示（只有对阵画面本身）。
- `net`：`Create` / `Join` 可带 `name`，`net::valid_name` 校验（trim 后 1–10 字节，仅 ASCII 字母 / 数字 / 空格 / `_` / `-`，非法即无名字）；`Snapshot` 增加 `names: [String; 2]`（只走快照，不走 Welcome：P1 反正要从快照里知道 P2 的名字）；协议版本 4 → 5，`web/network.js`、`security-smoke`、`online-smoke` 裸 WebSocket、服务器 `/api/config` 测试同步改为 5。
- 服务器：席位记住名字（`clean_name`，非法 / 缺省回退 `P1` / `P2`），随快照下发；带 `resume` 重进不带合法名字时沿用席位原名，带新合法名字则改名；名字不用于鉴权、不写日志。快照缓冲上限 60 → 400 帧：对阵 + 倒数共 210 帧 ≈ 3.5 秒，期间 20 Hz 下发的快照不再被当作迟到丢弃。
- 客户端：网页房间面板加“你的名字”输入框（故意不放进 `<form>`——单文本框的表单按回车会隐式提交并刷新页面），存 `localStorage`（键 `naifrog-name-v1`，那里永不存口令），创建 / 加入 / 断线重连都带上；桌面版加 `--name` 参数（非法则回退 P1 / P2 并提示）。HUD 血条上方常驻显示双方名字 + 标签（`YOU` / `RIVAL` / `CPU`）；对阵画面居中显示金 / 蓝两名字与 `VS`。
- 验收脚本跟着改：`browser-smoke` 两处开场等待 2 秒 → 4 秒；`gamepad-smoke` 的 `sinceOutcome` 计入第一局 120 帧；`online-smoke` 加名字步骤（双方填名、两端快照都看到两个名字、`localStorage` 记住名字、对阵画面截图）；`security-smoke` 加第 8b 节（敌意名字被清洗为座位默认值且绝不出现在快照里）。

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 70 通过（对阵只领先第一局 / 每局 `ticks_since_outcome` 都从结算起算 / 210 帧内输入无效 / 名字校验 trim、长度、字符集 / 名字随消息与快照往返） |
| `cargo test --locked --no-default-features --features server --lib` | 100 通过（含名字清洗、断线沿用、改名、快照下发；`/api/config` 期望 protocol 5） |
| `cargo test --locked --no-default-features --features "server client" --lib` | 103 通过 |
| `cargo test --locked --bin naifrog-duel`；桌面截图冒烟 | 2 通过；`artifacts/smoke-b3.png` 截图成功（HUD 血条上方名字 + 标签渲染正常） |
| clippy（全功能 / wasm32）、fmt、`check-assets`、`node --test scripts/server.test.mjs` | 通过 |
| `.\scripts\build-web.ps1 -Release` | 通过，`naifrog_duel_bg.wasm` 39,788,165 字节 |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器，61 fps，新 WASM） | 通过：防御削血 521→511、键盘轻拳 521→469、按住不连发、重开回满 521、键盘奶泡 521→469、触屏奶泡 521→469、CPU 模式触屏轻拳 521→469（4 秒开场等待下全部成立） |
| `online-smoke.mjs`（release 服务器 18090，协议 5，PC + 平板尺寸触屏） | 通过：双方填名（`PC-One` / `Pad_Two`）后两端快照都看到 `names` 一致、`localStorage.naifrog-name-v1` 记住 `PC-One`、对战开始时对阵画面仍在（`intro > 90`）并截图（`online-pc-versus.png`：金色 `PC-One` vs 蓝色 `Pad_Two` + `VS`，血条上方 `PC-One YOU` / `RIVAL Pad_Two`）；其余移动、攻防、奶泡、断线恢复、旧连接失效、成对重开、KO、离开、换房全部与上节一致 |
| `gamepad-smoke.mjs`（协议 5） | 通过：早期 A 不投票（KO 后第 14 帧松开，VS 修正后的计时）、按住跨过门槛不投票、新按 A 第二票开新局无跳跃；CPU 局轻拳 521→427、KO 后陈旧 A 松开不重开（0/427）、下一局自动回满 521/521、第二局 CPU 夺冠（0/521）、新按 A 重置整场 521/521 |
| `security-smoke.mjs`（一次性实例 18093，MAX_ROOMS=2 / 64 MiB 每小时） | PASS：协议 5、伪造输入、跨会话、限流、出站预算全部符合预期；新增：敌意名字（`<script>` 标签、非 ASCII + 控制字符 + 超长）创建 / 加入都被清洗为 `["P1","P2"]`，且原字符串绝不出现在任何快照里 |

没有验证：真平板 / 真手柄上名字与对阵画面的观感；名字输入框的真键盘中文输入法行为（模拟输入已覆盖提交路径）。

## 打击顿帧、连击计数与 HUD 头像 / 警示色（2026-10-03）

问题：命中没有分量感，一串命中也只是血条数字在掉；HUD 只有名字与血条，低血量与最后十秒没有警示；玩家没有头像。

修改：

- `combat` 加顿帧（hit-stop）：未被防御的命中让全场定格 4 帧（`HIT_STOP_TICKS`）——双方、飞行中的奶泡与 99 秒计时器一起暂停；被防御（削血）不触发；连续命中时定格取最长、不叠加；顿帧期间到达的攻击 / 跳跃边沿直接丢弃，不会在定格结束后补发。服务器跑同一套模拟，顿帧对联机同样生效。
- `combat` 加连击计数 `Fighter::combo_taken`：打进同一段受击硬直的连续命中累计，受击方回到可行动状态即清零；防御削血永不计数。协议版本 5 → 6，`web/network.js`、`security-smoke`、`online-smoke` 裸 WebSocket、服务器 `/api/config` 测试同步改为 6。
- CPU 节奏测试适配顿帧：守备 / 出招 / 起跳计数只在非顿帧（`hitstop == 0`）的活 tick 上计量；“攻击明显多于起跳”的倍率从 4× 放宽到 3×（顿帧削的是攻击节奏，奶泡触发的起跳不受影响）；反应时间测试窗加 `HIT_STOP_TICKS`。
- HUD：血条外侧加双方共用的头像（素材脚本从待机第一帧裁切生成的 128×128 `portrait.png`，资产清单与 `check-assets` 校验透明角落、来源哈希与待机图一致），框色随座位金 / 蓝；两次起连击数显示 `N HITS`（攻击方一侧）；最后 10 秒计时器变红。
- 低血量警示：血量低于 25 时血条**边框**红色脉动。第一版做成填充色脉动，`gamepad-smoke` 立刻红了：血条像素探测（防御削血读数、KO 判定 `gold === 0`）把脉动谷底当成空血，误报提前 KO。改回边框脉动、填充保持座位纯色后探测恢复稳定（3a26f16）。
- `gamepad-smoke` 第 8 步跟着修：KO 轮询截图比真实 KO 慢约 1 秒，而局间结果窗口只有 2 秒，陈旧 A 松开后的截图可能落在结果期、也可能下一局已自动开场（两种都是满血条）。断言从“血条等于 KO 时”改为读**胜局标记**：进行中的比赛 CPU 的胜局点亮着，重开会整场重置、标记全灭——重开与否与时序无关（e2fc908）。像素探针先在已标注截图上校准（0-0 / 0-1 / 0-2 / 新场四种状态读数全部正确）再上跑。

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 73 通过（顿帧只冻结不被防御、取最长不叠加、计时器停走、边沿丢弃；连击同硬直链累计、受击方回待机清零、防御不计；CPU 节奏与反应窗适配） |
| `cargo test --locked --no-default-features --features server --lib` | 103 通过 |
| `cargo test --locked --no-default-features --features "server client" --lib` | 106 通过（`/api/config` 期望 protocol 6） |
| `cargo test --locked --bin naifrog-duel`；桌面截图冒烟 | 2 通过；`artifacts/smoke-b4.png`（头像渲染核对：双方头像 + 金 / 蓝边框无异常）、边框脉动改版后 `artifacts/smoke-b4b.png` 复跑通过 |
| clippy（全功能 / wasm32 `--bin`）、fmt、`node --test scripts/server.test.mjs` | 通过（node 3/3） |
| `check-assets.mjs` | PASS：12 套图集、基线 / 边距、原始哈希、奶泡图集、**HUD 头像**（方形、透明角落、非空、来源与待机图同哈希） |
| `.\scripts\build-web.ps1 -Release` | 通过，`naifrog_duel_bg.wasm` 39,800,385 字节 |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器，61 fps，新 WASM） | 通过：防御削血 521→511、键盘轻拳 521→469、按住不连发、重开回满 521、键盘奶泡 521→469、触屏奶泡 [521,469]、CPU 模式触屏轻拳 521→438（顿帧不影响血条像素读数） |
| `online-smoke.mjs`（release 服务器 18090，协议 6，PC + 平板尺寸触屏，X-Real-IP 10.88.7.4 / 10.88.7.5） | 通过：名字 / 对阵（intro 131）、移动、攻防（含快照 `combo` 字段 = 1）、奶泡、跳跃（82 单位）、断线恢复（791 tick）、旧连接失效、成对重开（回合 0→1）、KO（`Winner(0)`，剩 86 秒）、离开、换房 |
| `gamepad-smoke.mjs`（协议 6） | 通过：早期 A 不投票（第 41 帧）、按住跨门槛不投票、新按 A 第二票开新局无跳跃、RB 奶泡 100→90 两端一致；CPU 局轻拳 521→479、KO 后陈旧 A 松开不重开（胜局标记亮着 63 px）、下一局自动开始、第二局 CPU 夺冠、新按 A 重置整场（标记全灭 0/0、血量回满 521/521） |
| `security-smoke.mjs`（一次性实例 18094，MAX_ROOMS=2 / 64 MiB 每小时） | PASS：协议 6 裸帧、伪造输入、跨会话、限流、出站预算全部符合预期 |

没有验证：连击 `N HITS` 文案、计时器变红与低血量边框脉动的**观感**（三个冒烟都停在满血开局，只有代码走查与“填充色不变所以像素探测稳定”的间接证据）；真平板 / 真手柄上的顿帧手感。

## 平板与触屏界面（2026-10-03）

问题：手指按下的页面按钮没有视觉反馈（触屏没有 hover 态）；刘海 / 圆角 / 手势条会压住摇杆和按钮；旋转成竖屏后控件可能出屏；新加的横幅、名字与连击文案可能与触屏控件重叠。

修改：

- 页面按钮加 `:active` 反馈（背景与边框变亮、轻微缩放）。`browser-smoke` 用 CDP `CSS.forcePseudoState` 强制 `:active` 断言计算样式变化——无头浏览器脚本不出真实手指的 `:active`，强伪类触发的是同一条规则。
- 安全区三段管线：`viewport-fit=cover`（已有）→ `style.css` 把 `env(safe-area-inset-*)` 落到 `--safe-l/r/t/b` 自定义属性（JS 读不到 `env()`，自定义属性链让测试可以覆盖）→ `main.js` 暴露 `__naifrogSafeArea()` 返回 CSS 像素的 `左,右,下,视口高` → wasm 每帧轮询（与 `__naifrogNetPoll` 同一模式），除以视口高换成高度分数（DPR 约掉），`ui::Insets::new` 把边 12% / 底 10% 以外的荒谬值夹回可触达范围。
- 让位规则：锚在屏幕右缘的按钮簇吃 `right` 内缩、锚在左缘的摇杆区吃 `left` 内缩，全部贴底控件抬 `bottom`；锚在中线的簇（双人左半场）不动。`button_at` / `stick_at` 命中测试与绘制用同一套内缩坐标，划入内缩带的触摸命中不了任何控件。HUD 文本（横幅 / 名字 / 连击 / 血条）都在上半屏，触屏控件都在下半屏——`browser-smoke` 在每个视口断言每个控件的 `(y-r)/h > 0.5`。
- 竖屏旋转：`browser-smoke` 用 `Emulation.setDeviceMetricsOverride` 在运行中切到 820×1180，断言画布跟随、四个按钮与摇杆全在屏内且在 HUD 带之下，再切回横屏。

| 项目 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 75 通过（新增：安全区把边缘控件内推的确切位移、划入内缩带无命中、荒谬内缩被夹住后控件仍在屏内；主布局测试扫 [无内缩, 刘海样例] × 两种布局 × 8 种宽高比） |
| `cargo test --locked --no-default-features --features server --lib` | 105 通过 |
| `cargo test --locked --no-default-features --features "server client" --lib` | 106 通过 |
| `cargo test --locked --bin naifrog-duel`；桌面截图冒烟 | 2 通过；`artifacts/smoke-b5.png` |
| clippy（全功能 / wasm32 `--bin`）、fmt | 通过 |
| `.\scripts\build-web.ps1 -Release` | 通过，`naifrog_duel_bg.wasm` 39,812,524 字节 |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器，61 fps，新 WASM） | 通过：防御削血 521→511、键盘轻拳 521→469、按住不连发、重开回满 521、键盘奶泡 521→469、触屏奶泡 [521,469]、CPU 模式触屏轻拳 521→438；`:active` 反馈、安全区内推（按钮左移 / 摇杆内上移 / 页面面板跟随）、竖屏旋转三项新检查全过 |
| `browser-smoke.mjs` 1000×600 DPR 2（画布 2000×1200，61 fps） | 通过：削血 812→796、轻拳 812→731、重开 812、奶泡 812→731 / [812,731]、CPU 触屏后 812→682；三项新检查全过（触控坐标按 CSS 像素断言，DPR 2 不再误判出屏） |
| `browser-smoke.mjs` 1180×820 DPR 2（画布 2360×1640，61 fps） | 通过：削血 959→940、轻拳 959→863、重开 959、奶泡 959→863 / [959,863]、CPU 触屏后 959→863；三项新检查全过 |
| `online-smoke.mjs`（重开的 18090，协议 6，X-Real-IP 10.88.7.10 / 10.88.7.11） | 通过：登录门 401×3、对阵画面 intro 134、攻防 100→92（combo 1）、防御 92→90、奶泡 100→90、跳跃峰值 96 单位、断线恢复冻结在 790 tick（90/76）、成对重开回合 0→1、KO 100→0 剩 86 秒、换房换席位——页面改动不影响联机路径 |
| `gamepad-smoke.mjs`（同 18090） | 通过：RB 奶泡 100→90 两端一致；第 45 帧早按 A 不投票、按住跨门槛不投票（胜局标记 63 px 亮着）、新按 A 投票；CPU 局触屏轻拳 521→479、Select 暂停 / 恢复、KO 0/479 |
| `security-smoke.mjs`（一次性实例 18095，MAX_ROOMS=2 / 64 MiB 每小时） | PASS：44 项全过（协议 6 裸帧、伪造输入、跨会话、限流、出站预算耗尽 503） |

调试中发现并处理：

- 第一轮复跑 `security-smoke` / `gamepad-smoke` 失败，排查是我误杀了 18090 的服务器进程：拿着压缩摘要里的 PID 对照表去 `taskkill`，而摘要把 18090 / 18094 的 PID 写反了，也没先用 `netstat -ano` 核对端口归属。`gamepad-smoke` 卡在登录表单的 ERR_CONNECTION_REFUSED 正是服务器已死的症状；`netstat` 核对后重启 18090，online / gamepad 全绿。教训已写入执行记录：杀进程前先核对端口与 PID。
- 18094 旧实例的 GET / 503 是小时流量额度耗尽（该实例已服务过本节与上一节的多次冒烟），不是回归；`security-smoke` 第 13 节本来就在验证额度耗尽行为，换 18095 新实例复跑通过。

没有验证：真平板（红米 Pad）上 `env(safe-area-inset-*)` 的实际数值、真机旋转与真机按下的观感——`browser-smoke` 的安全区检查靠测试覆盖 `--safe-*`（与 `env()` 同一条自定义属性链），真机项见下方人工验收清单新增条目。

## 断线重连补强（2026-10-03）

问题：网络切换（Wi-Fi↔流量）后页面只认旧的登录地址，五个小时内盲目重连 80 次也不提示；等的一方不知道对方席位还保留多久；断线一方看不到重试进度、也不能主动重连；对局中途恢复的回合从冻结帧直接开打，回来的人还没看清就被打了。

修改：

- 登录失效检测（C1）：从第 3 次被拒的重连起，每次先探 `GET /api/config`；若 `authenticated:false`（会话已不认这个地址），停止重连，显示“登录已失效（可能切换了网络），请重新输入口令。”并弹出口令表单。探测本身失败（还离线）则照旧退避重试。重新登录后如实提示原座位已随旧登录失效。
- 即时重连（C3）：`pageshow`、`visibilitychange`（变可见）与浏览器 `online` 事件都立刻重连并清零退避（健康或正在拨号的连接不重复拨）。地址恢复 + `online` 事件时同一座位免重新登录直接恢复。
- 席位保留倒计时（C2）：快照新增 `grace_left`（每席位秒数，连着为 null；serde 带默认值，旧服务器不发也能解析），协议版本 7。等的一方画面显示 `RIVAL DISCONNECTED 4:59` / “对方已断线（席位保留 4:59），对局暂停”；`__naifrogState` 增加 `graceLeft`。断线一方的顶部状态条显示重试次数与“立即重试”按钮（`#net-status` 改为 span + 按钮结构，点击走同一条即时重连路径）。
- 恢复回合 3-2-1（C4）：战斗核心新增 `Arena::resume` 与 `RESUME_TICKS=180`——中断恢复后的 3 秒与开场倒数同规则（输入无效、计时冻结），`banner` 显示 3 / 2 / 1；服务器 `interrupted` 标记在双方席位都恢复当前的第一个 tick 挂起倒数（该 tick 本身就是倒计时第一拍）；开局与等人不播，中断期间已出结果的回合直接续显结果。协议版本 8，`web/network.js`、`online-smoke`、`security-smoke` 同步。
- 快照帧改 `Box<Snapshot>` 装箱（clippy 大变体门禁触发；房间广播器本就持箱，每广播少一次拆装分配），线上 JSON 不变。

| 项目 | 结果 |
| --- | --- |
| `cargo test --no-default-features --lib` | 79 通过（新增恢复倒数冻结 / 重开清空、banner 3-2-1） |
| `cargo test --no-default-features --features server --lib` | 111 通过（新增中断回合恢复倒数、每席位保留倒计时） |
| `node --test scripts/server.test.mjs` | 3 通过 |
| `cargo fmt --check`；clippy（native bin / wasm32 bin / server lib，`-D warnings`） | 全部通过 |
| `online-smoke`（协议 8，双浏览器 + X-Real-IP 换地址） | 通过：断线 25 秒（超服务器心跳超时）后换地址回来，第 3 次被拒即探配置、弹“登录已失效”且 4 秒内无再重试；恢复地址 + `online` 事件后同一座位免登录恢复；PC 离开期间平板 `graceLeft[0]` 从 ≤300 走秒、状态条含“席位保留 m:ss”；PC 回来后平板在 177/180 拍捕捉到 3-2-1，倒数走完计时器才重新走字 |
| `security-smoke`（18095 一次性实例，协议 8） | 44 项通过 |
| `browser-smoke` / `gamepad-smoke` | 通过 / 通过 |
| wasm（`build-web.ps1 -Release`） | 39,815,173 字节 |

调试中发现并处理：

- CDP `Network.emulateNetworkConditions` 的 offline 掐不断已建立的 WebSocket（离线期间什么也收不到，30 秒等不到 `onclose`）。改为：先设好切换后的 `X-Real-IP`，离线保持 25 秒让服务器心跳超时关掉自己一侧，再恢复网络——页面 60 Hz 的输入包发往已死 TCP 连接收到 RST，`onclose` 才触发。这才是 Wi-Fi↔流量切换的真实形态，冒烟因此多花 25 秒。
- 快照加 `grace_left` 后 `ServerMessage::Snapshot` 触发 clippy 大变体门禁（恰好过了尺寸阈值）；按建议装箱帧而不是加 allow。

没有验证：真机 Wi-Fi↔流量切换、锁屏解锁、切 App 回来（`pageshow` / `online` 在真机上的时序）——见下方人工验收清单新增条目。断线接管（拿恢复令牌顶掉掉线席位）与放宽会话 IP 绑定**刻意未做**，需要书面威胁模型评审后另行授权。

## 整分支评审修复（2026-10-03，协议 9）

问题（评审发现三处）：换传输后名字每次无快照的轮询都被清回 P1/P2（`apply_view` 把清空放在了传输赋值之后，成了无条件执行）；`ticks_since_outcome` 把本回合已积累的冻结拍（顿帧 + 恢复倒数）当成结果时间——连击 KO 后结果只显示 `120−顿帧合计` 拍，中断期间已出结果的回合恢复后可能直接跳进下一局，手柄 A 的结果期防误触门也提前打开；`banner` 测试触发了 clippy `--all-targets` 门禁。

修改：清名字只随传输变化分支走；`Arena::dead` 计本回合冻结拍（恢复倒数与顿帧各 +1），`ticks_since_outcome` 一并扣除——结果展示与防误触门从结果出现那拍起算满 120 拍。`dead` 随快照上线，协议版本 8→9（`web/network.js`、`online-smoke`、`security-smoke`、服务器配置测试同步）。AI 不变量测试改为**全程**采样 CPU 是否命中过（KO 后会滚进血量回满的新回合，终点读数分不清"没打过"和"刚开新局"）。

| 项目 | 结果 |
| --- | --- |
| `cargo test --no-default-features --lib` | 80 通过（新增"恢复后的回合照样先显示结果"） |
| `cargo test --no-default-features --features server --lib` | 112 通过 |
| `cargo test --bin naifrog-duel` | 3 通过（新增"两次快照之间名字不闪"） |
| `node --test scripts/server.test.mjs`；`check-assets.mjs` | 3 通过 / 通过 |
| `cargo fmt --check`；clippy `--all-targets --all-features` 与 wasm32 `--bin`，`-D warnings` | 全部通过 |
| `online-smoke`（协议 9） | 通过：平板 175/180 拍捕捉到 3-2-1、倒数走完计时器才走字；换网提示与免登录恢复不变 |
| `security-smoke`（一次性实例，协议 9） | 44 项通过 |
| `browser-smoke` / `gamepad-smoke` | 通过 / 通过（gamepad 连续三轮全绿；此前五轮中有一轮 `pass:false`，未存报告、连续重跑未复现，符合该冒烟已记录的时钟窗敏感性，已留档） |
| wasm（`build-web.ps1 -Release`） | 39,815,325 字节 |

调试中发现并处理：

- AI 不变量测试最初因 `dead` 改动翻红。定位：CPU 在第 1049 拍 KO（8 次命中累计 32 拍顿帧），旧公式把顿帧预记为结果时间、新公式记满 120 拍，第二局起点整体后移 32 拍，固定的 1320 步终点恰好落在 CPU 尚未首次命中的新回合里。是测试在混沌轨迹终点采样的问题，不是模拟回归。
- `naifrog-server` 的传输预算环境变量是 `NAIFROG_EGRESS_MB_PER_HOUR`；误设 `NAIFROG_EGRESS_LIMIT` 会被静默忽略、落到默认 512 MiB/h。一次性安全实例配置错时，"匿名下载 4 MiB 断流"检查出现一次无法解释的 401（正确的 64 MiB 配置下 44 项全过，服务器代码未改）。留档提醒：启动脚本要抄 DEPLOY.md 的变量名。

## 削血、奶泡弹、肚皮压

问题：前方防御永远不扣血；跳起来没有优势，也没有能跳过的远程攻击；平板只有四个按钮。

修改：前方防御削掉原伤害的 1/4（向上取整），削血也能 KO。新增奶泡弹（T / I / 手柄 RB / 触屏 SHOT），空中缩腿，算准时机能跳过奶泡。新增空中肚皮压（每次跳跃一次）。输入多了 SHOT 位，所以协议版本升到 2，旧页面要刷新。奶泡弹、肚皮压的角色动作是**占位图**（复制轻拳 / 冲刺的图集），奶泡特效由素材脚本程序生成。

| 项目 | 结果 |
|---|---|
| `cargo test --no-default-features --lib` | 61 通过。覆盖：削血与削血 KO；奶泡第 14 帧生成、每帧 8、相撞抵消、可防御、出界消失、穿过倒地无敌；从开场距离原地跳有 7 帧、向前跳有 24 帧能躲过奶泡；肚皮压每跳一次、落地可行动、可防御；CPU 会跳过或防御奶泡 |
| `cargo test --no-default-features --features "server client" --lib` | 91 通过 |
| `cargo test --bin naifrog-duel` | 2 通过 |
| clippy（全功能 / wasm32）、fmt | 通过 |
| `check-assets.mjs` | 通过：12 套图集（占位 shot←light、air←dash），奶泡图集不贴边 |
| `browser-smoke.mjs`：1280×720 DPR 1；1000×600 真实 DPR 2（画布 2000×1200） | 通过。本地双人每人 5 个按钮；前方防御轻拳削 2 血（1280 宽时蓝条 521→511 像素）；键盘 T、P2 触屏 SHOT 的奶泡各打掉站着不动的对手 10 血 |
| `online-smoke.mjs`：本机服务器，PC 1280×720 + 平板尺寸 1180×820 DPR 2 触摸仿真 | 通过。平板防御 PC 轻拳 92→90，两边一致；平板 SHOT 奶泡打 PC 100→90，两边一致；重开、KO、断线恢复、离开等原有步骤照常 |
| `gamepad-smoke.mjs`（模拟标准手柄） | 通过：RB 奶泡打掉键盘对手 10 血，两边一致 |
| `security-smoke.mjs` | 42 项通过；服务器日志只有一行 |
| 桌面截图（临时脚本场景，截完已删除） | 奶泡在胸口高度飞行；肚皮压占位动作显示；1180×820 双人、1200×720 单人的触屏布局中 5 个按钮互不重叠；自己的奶泡在场时 SHOT 变灰 |

调试中发现并修复：

- `web/network.js` 发送前按旧的位数截断输入（`held&127`、`pressed&92`），网页联机时 SHOT 位被丢掉，平板按 SHOT 没有反应。现改为 255 / 220，与 `net::INPUT_MASK`、`controls::TRIG` 一致。online-smoke 新增的平板 SHOT 步骤修复前失败，修复后通过。桌面联机客户端直接用 Rust 常量，不受影响。
- 测试脚本里的旧期望：
  - 三个浏览器脚本等的是 “All 13 game textures ready”，现在是 16 张。
  - online-smoke 点 REMATCH 的位置仍是 HUD 重做前的 y=0.168，现在按钮在 0.172–0.216，改为点中心 0.194。
  - security-smoke 仍期望 protocol 1、WASM 每分钟 3 次（服务器早已改为 6 次）。
  - browser-smoke 在 600 高的屏幕上去点菜单里滚出视口的“返回对局”，现在先滚动到按钮再点。

没有验证：真平板、真手柄；奶泡和肚皮压在真实网络延迟下的手感；正式原画（还没有）。

## 平板去掉 JUMP 按钮（四个按钮，摇杆上推跳）

修改：触屏按钮只保留四个，排成菱形：左 LIGHT、上 HEAVY、右 DASH、下 SHOT（原来 JUMP 的位置）。跳跃只靠摇杆：上推跳，斜上推向前 / 向后跳；判定阈值没有改（上推超过半径 55% 且比水平方向明显）。键盘、手柄不变。

| 项目 | 结果 |
|---|---|
| `cargo test --no-default-features --lib`、clippy（全功能）、fmt | 61 通过；无警告 |
| `browser-smoke.mjs` 1280×720 DPR 1；1000×600 真实 DPR 2 | 通过：单人 4 个、双人每人 4 个按钮，没有 JUMP；防御削血、键盘与触屏奶泡同上一节。DPR 2 下第一次运行时 CPU 在测试打中它之前先 KO 了 P1（截图慢，CPU 现在会丢奶泡、会压），脚本改为 P1 被 KO 时按 R 重开再试，之后通过 |
| `online-smoke.mjs` | 通过：平板没有 JUMP 按钮；摇杆上推跳，PC 端看到 P2 离地（最高看到 102 单位）；防御削 2、奶泡 10、重开、KO 等照常 |

没有验证：真平板上摇杆上推跳、斜推向前跳是否顺手，左右走时会不会误跳。

## 评审第二轮修复（2026-10-03，6 项，协议仍为 9）

六个确认问题各自独立小提交（7240fa1、f57eb0f、98c7ad9、fc33ef3、d70508c、ee7a57f），全部先写失败测试、看它失败、再修。快照与消息字段没有任何变化，`PROTOCOL_VERSION` 保持 9。

修改：

- **手柄 A 重开只认整场结束**（7240fa1）：新增 `ui::pad_rematch_ready`——要求 `match_outcome` 已定且结果显示满 `REMATCH_DELAY`。打满整场之前的局间结果显示里 A 完全无效（局间自动进下一局）；R / Start / 屏上 REMATCH 仍是任何时候重开整场。A 在抬起沿触发（A 同时是跳，按住不松会把跳带进新回合）。README 与页面帮助同步改写。
- **五局打满按胜局多数定整场**（f57eb0f）：先到两胜不变；打满五局后按 `wins` 多数决定整场（胜局相同则整场平），最后一局的赢家不必然拿走整场。`ai.rs` 的收敛断言放松为“赢家胜局更多”。
- **等待 / 开场期间掉线不再叠 3-2-1**（98c7ad9）：`Arena::resume_now` 只在 `outcome.is_none() && intro == 0` 时布防——对战卡与回合开场本来就不吃输入、时钟冻结，回来时原地继续；服务器布防处改为直接信任 `resume_now`，删掉重复的 outcome 判断。
- **重新登录提示在唤醒后不消失**（fc33ef3）：新增 `awaitingRelogin` 状态，`pageshow` / `online` / `visibilitychange` 在此状态下最多做一次静默拨号——地址回来了席位悄悄恢复（welcome 清除等待并收起表单），没回来则拒绝无声通过，口令框、已输入的文字与状态原样保留。
- **自动重连尊重服务器叫停**（d70508c）：三个唤醒事件改传 `auto=true`，`intentional` 置位时直接返回——此前被顶掉的标签页会在唤醒时拿恢复令牌把席位抢回来；错误 / 服务器关闭处理器同时隐藏“立即重试”，只有人点按钮才再试。
- **DEPLOY.md WASM 每 IP 限速 3 → 6**（ee7a57f）：与 `server.rs` 的 `WASM_PER_IP_PER_MINUTE` 及本文档既有记载对齐。

| 项目 | 结果（全部本次重跑） |
|---|---|
| `cargo fmt --check` | 干净 |
| `cargo test --no-default-features --lib` | 87 通过 |
| `cargo test --no-default-features --features "server client" --lib` | 123 通过（含新增 `a_drop_while_waiting_for_the_rival_plays_no_countdown`、`the_resume_countdown_skips_versus_and_introductions` 与手柄 A 三项） |
| `cargo test --bin naifrog-duel` | 3 通过 |
| clippy 宿主 `--all-targets --all-features -D warnings` 与 wasm32 `--bin` | 均无警告 |
| `scripts\build-web.ps1 -Release` | WASM 39,815,426 字节（gz 8,294,927） |
| 桌面 `--smoke artifacts/smoke.png` | 17 张贴图就绪，截图写出 |
| `browser-smoke.mjs` 1280×720 DPR 1 | 通过：开局双方 521、CPU 模式触屏轻拳 469 / 479，全部像素断言通过（退出码 0） |
| `gamepad-smoke.mjs` | 通过（退出码 0）：局间结果显示里 A 被忽略（midMatchAIgnored）、局间自动进下一局且胜局保留（autoAdvanceKeepsWins）、整场结束（`match_outcome` Winner）后 A + 对方 R 完成重开、重开后双方 521、胜局点 0/0、无自跳 |
| `online-smoke.mjs` | `pass: true`：17 步全过 + 两处新检查——唤醒（pageshow / online / visibilitychange）不擦掉重新登录提示（表单、已输入文字、状态原样，立即重试不出现），被顶掉的页面唤醒后保持沉默（不抢席位）、另一标签保住席位、错误 / 关闭后立即重试保持隐藏 |
| `security-smoke.mjs`（一次性实例 18091，`NAIFROG_MAX_ROOMS=2`、`NAIFROG_EGRESS_MB_PER_HOUR=64`） | PASS：含预算耗尽路径（第二个 WASM 503、活动对局收到 `server transfer budget exhausted`） |

调试中发现（环境，非产品代码）：

- 本轮联机冒烟只用**一个**可调试 Chromium（9223）：两个客户端是同一实例里各自隔离的浏览器上下文，登录互不干扰。两个实例双开时 online-smoke 第 11 步的 `Runtime.evaluate` 曾在导航中丢失、整个脚本悬挂（Node 以未决顶层 await 退出）；单实例下未再出现。
- 本机显示器缩放 125%：CDP 模拟 `deviceScaleFactor=1` 只改报告值，画布后备存储仍是 CSS×1.25，browser-smoke 的画布一致性检查失败。Chromium 要加 `--force-device-scale-factor=1` 启动。
- browser-smoke 取 `/json/list` 的第一个页面标签：上次运行残留的旧标签会让它连错目标，等待贴图就绪超时。冒烟前用干净 user-data-dir 重启浏览器即可。

没有验证：真平板、真手柄上的新行为——局间结果显示里手柄 A 不重开（见下方人工验收清单新增条目）、切网后重新登录提示在真机唤醒（切后台回来 / 锁屏解锁）时是否原样保留。模拟触控与模拟手柄都不是硬件。

## 浏览器缓存与 WASM 瘦身（2026-10-03，协议仍为 9）

快照与 WebSocket 消息没有变化，`/api/config` 的字段也没变，`PROTOCOL_VERSION` 保持 9。

修改：

- **WASM 瘦身**：Bevy 依赖由 `2d` / `ui` 整包改为只列出用到的功能；新增 `web-release` 构建配置（继承 release，整程序 LTO、单代码生成单元），`build-web.ps1 -Release` 与 Dockerfile 用它，桌面版与服务器仍用 release。试过 wasm-opt：gzip 后只再小约 0.7%，不用。
- **按文件内容哈希的浏览器缓存**（与 Unity“文件名带哈希 + Addressables 目录”、Cocos md5Cache 同一思路）：服务器给每个游戏文件算 SHA-256（取前 80 位），登录后 `GET /api/manifest` 返回清单（需登录、计入文件请求频率、`no-store`）。页面用 `/v/<哈希>/pkg/…`、`/v/<哈希>/assets/processed/…` 加载胶水脚本、WASM 与贴图；贴图由 main.js 的 fetch 包装改写地址，Rust 代码未改。哈希与文件当前内容一致时回 `private, max-age=31536000, immutable`；普通地址或哈希不符时 `private, no-cache`。页面脚本与样式 `no-cache` + ETag，未变时 304 无正文、不计流量；WASM 的 gzip 副本有自己的 ETag。离线静态服务器没有清单，页面退回普通地址。
- **页面本身仍是 `private, no-store`**：第一次联机冒烟在第 10 步失败（PC 离开页面后，平板 5 秒内看不到 P1 断线），重跑同样失败。探针证实：页面改为 `no-cache` 后，Chrome 会把它放进往返缓存（返回时 `BackForwardCacheRestore`），联机连接被冻结而不断开。`/` 恢复 `no-store` 后 Chrome 报告 `MainResourceHasCacheControlNoStore`，不再放入，冒烟通过。
- **Caddy 不再覆盖 Cache-Control**：原 `header` 块强制 `private, no-store`，会让以上缓存全部失效；已删除并加注释。
- **安全冒烟随 WASM 变小调整**：预算一步不再假设“第一份 WASM 装得下、第二份装不下”，改为一直下载到被拒（本次装下 2 份，第 3 份 503）。预算耗尽后新建房间，改为检查该连接关闭前收到的原因：剩下的几个字节可能还够发 welcome，随后立即以预算原因关闭——两次运行中两种顺序各出现一次。另加清单检查（未登录 401、登录后条目为 20 位十六进制、不含 .gz）与缓存头检查。
- 登录页的载入提示改为“之后浏览器会缓存，游戏更新时只重新下载变化的部分”。

| 项目 | 结果（全部本次重跑） |
|---|---|
| `cargo fmt --check` | 干净 |
| `cargo test --no-default-features --lib` | 87 通过 |
| `cargo test --no-default-features --features "server client" --lib` | 125 通过（新增 `manifest_lists_game_files_and_versioned_urls_are_kept`、`version_prefixes_are_lowercase_hex_segments`） |
| `cargo test --bin naifrog-duel` | 3 通过 |
| clippy 宿主 `--all-targets --all-features -D warnings` 与 wasm32 `--bin` | 均无警告 |
| `node --test scripts/server.test.mjs` / `node scripts/check-assets.mjs` | 3 通过 / 通过 |
| `scripts\build-web.ps1 -Release`（无 wasm-opt） | WASM 22,390,791 字节（gz 5,902,317）；上轮 39,815,426（gz 8,294,927） |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器：无清单，走普通地址） | 通过（退出码 0） |
| `online-smoke.mjs`（一个 Chromium，两个隔离上下文） | 修复前两次都在第 10 步失败；修复后 `pass: true` |
| `gamepad-smoke.mjs` | 通过（修复前、后各一次） |
| `security-smoke.mjs`（一次性实例，`NAIFROG_MAX_ROOMS=2`、`NAIFROG_EGRESS_MB_PER_HOUR=64`） | PASS |

缓存核对（临时 CDP 脚本，同一浏览器上下文连续打开三次，数字取自 Chrome 网络事件）：

| 打开 | 19 个游戏文件（胶水脚本、WASM、17 张贴图） | 游戏文件经网络的字节 |
|---|---|---|
| 第一次（未登录、空缓存） | 全部经网络，全部是 `/v/<哈希>/` 地址并带 `immutable` | 22,582,478 |
| 第二次 | 19 个全部来自浏览器缓存 | 0 |
| 只改动 `naifrog_duel.js` 之后 | 18 个来自缓存，只重新下载它（新哈希） | 107,184 |

第二、三次打开时 `/` 每次 8,166 字节（`no-store`），另外三个页面文件各 446 字节（只传响应头的校验）；清单每次都经网络（`no-store`）。改动的文件在核对后已原样恢复。

没有验证：手机 / 平板浏览器上的缓存（容量与清理策略、无痕模式）；经 Caddy + HTTPS 时的响应头；Docker 内 Linux 构建的 WASM 大小；真正的重新部署（本次用追加内容模拟“只改一个文件”）。真平板、真手柄同样没有验证。

## 无损 WebP 贴图（2026-10-03，协议仍为 9）

快照与 WebSocket 消息没有变化，`PROTOCOL_VERSION` 保持 9。

修改：

- **17 张游戏贴图由 PNG 改为无损 WebP**（12 张角色图集、命中特效、奶泡图集、背景、影子、HUD 头像），像素与原 PNG 完全相同，合计 16,563,292 → 7,868,298 字节（-52.5%）。试过有损 q90：更小，但放大后皮肤纹理被抹平（PSNR 约 38 dB），不用。
- **素材管线用 sharp 编解码 WebP**：新增 `package.json` / `package-lock.json`（sharp 0.35.5，内含 libwebp 1.6.0）与 `scripts/webp-codec.mjs`。参数为 `lossless`、`effort: 6`、`exact`（保留全透明像素的颜色，精灵边缘过滤时采样不变）；每张图写出前先解回来逐字节核对。`prepare-game-assets.mjs` 与 `check-assets.mjs` 运行前需要 `npm ci`，`run-game.ps1` / `build-web.ps1` 在缺素材时会自动执行。读取原图仍用 `sprite-codec.mjs`。Dockerfile 只复制已生成的 `assets/processed/`，镜像里不需要 Node 或 sharp。项目约定改为“Node 脚本可以引入 npm 依赖，在 package.json 里声明”。
- **游戏**：Bevy 增加 `webp` 功能（保留 `png`，`--smoke` 截图要用），WASM 因此多 120,156 字节（gzip 后约多 48 KB）；`main.rs` 中六处载入路径改为 `.webp`。
- **服务器与页面**：`naifrog-server` 的游戏文件类型加入 `.webp`，响应 `image/webp`；离线静态服务器补上同一类型；main.js 的贴图地址改写改为匹配 `.webp`；`.dockerignore` 放行 `assets/processed/*.webp`。服务器 Rust 测试、`server.test.mjs` 与安全冒烟里的贴图名一并改为 `.webp`。
- 按本次实测更新文档中的 WASM 大小与流量预算：DEPLOY.md 中首次完整载入约 13.9 MB，默认 512 MiB 约够每小时 38 次（上轮约 22.6 MB、23 次）。

| 项目 | 结果（全部本次重跑） |
|---|---|
| `cargo fmt --check` | 干净 |
| `cargo test --no-default-features --lib` | 87 通过 |
| `cargo test --no-default-features --features "server client" --lib` | 125 通过（贴图夹具改为 `fighter.webp`，新增 `image/webp` 断言） |
| `cargo test --bin naifrog-duel` | 3 通过 |
| clippy 宿主 `--all-targets --all-features -D warnings` 与 wasm32 `--bin` | 均无警告 |
| `npm ci` / `node --test scripts/server.test.mjs` / `node scripts/check-assets.mjs` | 安装完成、0 个漏洞 / 3 通过 / 通过 |
| 重跑 `prepare-game-assets.mjs` | 17 张 WebP 与 `game-assets.json` 和上次生成逐字节相同 |
| 像素核对（临时 Rust 程序，与游戏同版本的 image 0.25.10 + image-webp 0.2.4） | 17 张 WebP 与替换前的 PNG 尺寸相同、逐像素相同；check-assets 另核对背景与原图逐像素相同 |
| `scripts\build-web.ps1 -Release` | WASM 22,510,947 字节（gz 5,950,662）；上轮 22,390,791（gz 5,902,317） |
| `cargo run --bin naifrog-duel -- --smoke`（桌面版） | 退出码 0，日志“All 17 game textures ready”；截图中背景、两名角色、影子、头像显示正常 |
| `browser-smoke.mjs` 1280×720 DPR 1（离线服务器） | 通过（退出码 0） |
| `online-smoke.mjs`（一个 Chromium，两个隔离上下文） | `pass: true` |
| `gamepad-smoke.mjs` | 通过 |
| `security-smoke.mjs`（一次性实例，`NAIFROG_MAX_ROOMS=2`、`NAIFROG_EGRESS_MB_PER_HOUR=64`） | PASS（装下 2 份 WASM，第 3 份 503） |

缓存核对（与上一节同一个临时 CDP 脚本，同一浏览器上下文连续打开三次）：

| 打开 | 19 个游戏文件 | 游戏文件经网络的字节 |
|---|---|---|
| 第一次（未登录、空缓存） | 全部经网络，全部是 `/v/<哈希>/` 地址并带 `immutable` | 13,935,837（上轮 22,582,478，-38%） |
| 第二次 | 19 个全部来自浏览器缓存 | 0 |
| 只改动 `naifrog_duel.js` 之后 | 18 个来自缓存，只重新下载它 | 107,184 |

解码时间（桌面原生 release，image 0.25.10，每张取 5 次中最快）：17 张合计 PNG 88.2 ms，WebP 223.8 ms，多约 136 ms；单张角色图集约 5 ms → 13–14 ms，背景 13.9 → 30.7 ms。第一次载入少下载约 8.6 MB，按 20 Mbps 计约少 3.5 秒。

没有验证：浏览器（WASM）内与平板上的实际解码耗时、贴图载入期间是否卡顿；Docker 内 Linux 构建（Bevy 新增 `webp` 功能后的镜像构建）；在 Linux / macOS 上运行 `npm ci` 与素材管线（sharp 按平台下载预编译包）。真平板、真手柄同样没有验证。

## 复制匹配码、中文名字与中英文界面（2026-10-04，协议仍为 9）

快照与 WebSocket 消息字段没有变化，`PROTOCOL_VERSION` 保持 9（安全验收脚本按协议 9 通过）。名字字段原来就有，只是允许的字符变多。

修改：

- **复制匹配码**：先用 Clipboard API（只在 https 与 localhost 页面上有）；没有或被拒绝时，在视口内放一个透明的只读文本框，选中后 `execCommand('copy')`，在点按的同步阶段执行（iOS Safari 也接受）；仍然失败时在面板上选中匹配码，按钮提示“已选中，长按或 Ctrl+C 复制”。反馈写在按钮上，因为状态行每秒被快照改写 20 次。
- **中文名字**：可用《通用规范汉字表》的 8105 个汉字（[名字可用汉字表](<src/name_han.txt>)，由 [名字字体脚本](<scripts/prepare-name-font.mjs>) 生成），最多 10 个字母或 6 个汉字宽，可以混用；服务器、桌面版、网页用同一张表校验。画面用裁剪后的 Noto Sans SC Medium（`assets/processed/name_font.otf`，OFL）画汉字，Bevy 把它设为汉字与拉丁文字的后备字体，字体就绪（或载入失败）后才开始游戏。
- **中英文界面**：网页与画面始终同一种语言，规则见 README“中文 / English”一节。网页：`index.html` 以中文为默认内容，英文放在 `data-en*` 属性里；main.js 选语言、切换并记在 `localStorage`；network.js 的每条提示都是中英成对，切换时重写当前状态、名字提示和复制按钮。画面：文字都在 [模式与按钮布局](<src/ui.rs>)，[语言模块](<src/i18n.rs>) 解析语言标签并估算宽度（测试保证每条状态行不超过 24.6 em）。桌面版：`--lang` / `NAIFROG_LANG` / 系统语言 / F2，`run-game.ps1 -Lang`。名字字体加入 171 个中文标点给界面用。
- **顶部导航**：英文按钮更宽，窄窗口里会压到最长的画面状态行（英文联机等待那行：24.6 em × 18 px，加内边距约 476 px，居中）。按钮略收紧，窗口宽度 ≤1100 px 时改为竖排。
- **桌面 debug 构建的控制台**：icu_provider 在 debug 构建里每排版一次中文就 `eprintln!` 一行 “No segmentation model for complex script”（bevy_text 没带中日文词典，按字断行，对本游戏没有影响）。Cargo.toml 只对这个包关闭 dev 配置的 debug 断言：中文演示 30 秒内从 109 行降到 0 行；release 与 WASM 构建本来就不输出。
- 文档：README 新增“中文 / English”一节，画面文字写成中英对照；DEPLOY.md 更新 WASM 大小、首次完整载入约 15.9 MB（默认预算约够每小时 33 次）、打包清单加字体与许可证，以及 http 页面上的复制说明。

| 项目 | 结果（全部本次重跑） |
|---|---|
| `cargo fmt --check` | 干净 |
| `cargo test --no-default-features --lib` | 93 通过 |
| `cargo test --no-default-features --features "server client" --lib` | 133 通过 |
| `cargo test --bin naifrog-duel` | 3 通过 |
| clippy 宿主 `--all-targets --all-features -D warnings` 与 wasm32 `--bin` | 均无警告 |
| `node scripts/check-assets.mjs` / `node --test scripts/server.test.mjs` | 通过（名字字体：ASCII + 8105 汉字 + 171 标点，OFL）/ 3 通过 |
| `scripts\build-web.ps1 -Release` | WASM 22,558,130 字节（gz 5,976,547）；上轮 22,510,947（gz 5,950,662） |
| 桌面 `--smoke --mode cpu --lang zh` / `--lang en` | 退出码 0，“Name font ready” 与 “All 17 game textures ready” 两条日志；截图中状态行、头顶标签、“开打！”横幅、底部操作提示都是所选语言 |
| 桌面 `NAIFROG_LANG=zh_CN.UTF-8`、不带 `--lang` | 中文菜单，`--server https://…` 被拒的原因也是中文 |
| 桌面 `--lang fr` | 退出码 1，`unknown --lang "fr"; use zh or en` |
| `browser-smoke.mjs` 1280×720（离线服务器；本机浏览器语言 zh-CN） | 通过 |
| `online-smoke.mjs`（一个 Chromium 两个上下文，页面带 `?lang=zh`） | `pass: true` |
| `gamepad-smoke.mjs`（页面带 `?lang=zh`） | `pass: true` |
| `security-smoke.mjs`（一次性实例） | PASS |

界面核对（临时 CDP 脚本；英文浏览器用 `Emulation.setUserAgentOverride` 的 `acceptLanguage: en-US,en`，`setLocaleOverride` 改不了 `navigator.languages`）：

| 场景 | 结果 |
|---|---|
| 英文浏览器第一次打开（离线服务器） | 网页与画面都是英文；可见文字里只有切换按钮“中文”含汉字；标题 “Naifrog Duel · CPU / Local / Online” |
| 在“操作”面板点“中文” | 网页与画面变中文，`<html lang="zh-CN">`，记住 `zh`；中文界面里只剩“English”按钮是纯英文 |
| 对局暂停时在菜单里切换 | “当前：单机打人机（已暂停）” → “Now: Vs CPU (paused)”，返回对局后画面是英文 |
| 重新载入 / `?lang=en` | 用记住的中文 / 本次用英文，记住的值仍是 `zh` |
| 联机房间面板（英文，本地 naifrog-server） | 登录提示、房间提示、名字规则、“Copy code”都是英文（占位符 “e.g. Alice or 奶蛙” 有意保留汉字）；建房后状态 “Room ####-#### · you are P1 (gold) · Waiting for the other player / paused”；在面板里切到中文，状态、复制按钮、名字提示立即变中文；离开房间的提示中英都对 |
| 名字（中文界面，本地 naifrog-server） | 输入 “奶蛙😀”“奶蛙，”“奶蛙奶蛙奶蛙奶”“ABCDEFGHIJK” 时提示显示不了；“奶蛙Pro123”“Alice”“奶蛙 一号” 通过；建房后快照里的名字是 “奶蛙Pro123”，本机记住同一个值，血条上方与开场对阵画面都正确画出 |
| 复制（127.0.0.1，打开房间面板点按钮） | Clipboard API：“已复制 ✓”，剪贴板内容等于匹配码。去掉 Clipboard API（模拟 `http://IP:端口`）：`execCommand('copy')` 返回 true，执行时选中的正是匹配码，临时文本框已移除，“已复制 ✓”。`execCommand` 也失败：面板上选中的文字等于匹配码，提示“已选中，长按或 Ctrl+C 复制” |
| 导航按钮与最长状态行（宽 768–1280，16:9；中 / 英；有无“房间”按钮） | 导航左缘始终在状态行右缘之外：≥1101 px 横排，最小间距 16 px（英文带“房间”按钮，1101 px）；≤1100 px 竖排，最小 57 px（768 px）。1100 px 与 1180 px 的联机等待截图里互不遮挡 |
| 浏览器控制台（中文单机局，载入后 8 秒） | 只有 Bevy 启动日志与两条 ready，没有 ICU 警告 |

已知、不是这次引入的问题：高度 400 px 以下的横屏窗口（如 844×390）里，画面状态行与“再来一局”按钮上下相互压住约 10 px（两者按画面百分比定位，与语言无关）。

没有验证：真平板 / 手机上的复制（iOS Safari、Android Chrome，尤其 `http://IP:端口` 页面）、系统输入法输入中文名字、桌面版对局中按 F2 切换（只验证了 `--lang` 与 `NAIFROG_LANG` 启动）、`run-game.ps1 -Lang` 实际启动（只做了语法检查）、Docker 内构建；这次没有部署到服务器。真平板、真手柄同样没有验证。

## 载入进度停在 0%（2026-10-04，协议仍为 9）

现象：中英文界面那一版部署后，线上第一次打开时“正在下载游戏程序（1/2）”一直显示 0%（文字“0% · 压缩传输共 5.7 MB”，进度条不动），下载完直接跳到“正在下载贴图（2/2）”。原因：那次把进度文字改成中英成对时，`fetchWasm` 的进度回调漏掉了 `got+=n`，已接收字节数一直是 0；贴图那一步单独计数，没有受影响。

修复：恢复计数，并把 WASM 的进度记在 `window.__naifrogLoad.wasm`（已接收 / 预计总字节）。`browser-smoke.mjs` 新增一条断言：载入后已接收字节大于 0，且知道总数时等于总数。

| 项目 | 结果（全部本次重跑） |
|---|---|
| 修复前的 main.js（本地 naifrog-server，gzip 传输，CDP 限速约 1.5 MB/s，中文） | 复现：第 1 步 4 秒里进度条始终 0、文字始终 0%，随后直接进入第 2 步 |
| 修复后，同样条件，中文 / 英文 | 第 1 步 27 次采样，进度条单调上升到 0.984（3% → 98%），结束时已接收 22,558,130 / 22,558,130 字节；第 2 步照常 0 → 17 张 |
| 修复后，离线服务器（不压缩，限速约 4 MB/s） | “0.2 MB / 21.5 MB（0%）”逐步到“21.1 MB / 21.5 MB（98%）” |
| `browser-smoke.mjs` 1280×720（离线服务器） | 通过，`wasmLoad` 22,558,130 / 22,558,130 |

没有验证：真实慢速网络（手机流量、平板 Wi-Fi）上的观感；新断言没有对修复前的页面单独跑过（修复前没有 `__naifrogLoad.wasm`，读取会直接失败）。

## 代码审查六项修复（2026-10-04，协议仍为 9）

- 战斗转向统一读取本帧开始时的位置，消除跳跃换边的席位偏差。新增镜像换席位回归：从默认开局走近、跳跃、换边防御再出肚皮压，每拍双方状态与镜像一致。修复前在第 66 拍失败，修复后通过。
- 服务端对 `.wasm.gz` 流式解压、检查 CRC，并将解压内容完整 SHA256 与当前 `.wasm` 比较；解压读取最多为原文件长度加 1，防止错误副本无限展开。校验结果按两个文件的长度 / 修改时间缓存，启动时预热；并发校验共用一把异步锁，最多一个解码任务。测试覆盖等长旧副本、坏 CRC、重新生成正确副本、等长更新原文件后的缓存失效，以及正常 gzip 传输。
- 浏览器重连探测在 `fetch` 和 JSON 读取完成后都先检查连接代次与联机状态，再修改界面。新增 `scripts/network.test.mjs`：旧 fetch 晚返回、旧 JSON 晚返回、退出联机后晚返回均不能弹出登录界面；当前连接的会话过期仍正常提示登录。修复前前三项失败，修复后四项通过。
- 桌面单机 / 本地同屏失焦时自动打开暂停菜单；回到窗口后手动继续。Bevy 集成测试运行真实输入与固定更新系统，验证失焦期间竞技场不变、恢复焦点不自动继续、一直按住的跳跃不泄漏、新按跳跃有效；联机不触发本地暂停菜单。
- 素材清单加入阴影，checker 解码命中特效与阴影，检查尺寸、非空、透明角和特效每帧边距。已重新运行素材生成；隔离读取替身验证缺失特效、缺失阴影、损坏特效、损坏阴影、空特效和错误阴影尺寸六种情况均拒绝，没有损坏真实素材。
- 桌面口令行接受内部空格，使用真实 `KeyboardInput` 消息验证 `review key` 不会变成 `reviewkey`。

| 验证 | 结果 |
|---|---|
| `cargo test --locked --no-default-features --lib` | 94 passed |
| `cargo test --locked --no-default-features --features "server client" --lib` | 135 passed，含真实服务器的桌面联机测试 |
| `cargo test --locked --bin naifrog-duel` | 5 passed，含失焦暂停与口令输入集成测试 |
| `node --test scripts/network.test.mjs scripts/server.test.mjs` | 7 passed |
| `node scripts/prepare-game-assets.mjs` + `node scripts/check-assets.mjs` | 通过，原图哈希未变；特效与阴影也纳入检查 |
| `node artifacts/code-review/verify-assets.mjs` | 六种异常素材状态均正确拒绝；该脚本为本次隔离验证产物 |
| `cargo fmt --all --check` | 通过 |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | 通过 |
| `cargo check --locked --target wasm32-unknown-unknown --bin naifrog-duel` | 通过 |

本节的桌面验证使用无渲染的 Bevy 测试应用；没有重跑真实窗口 Alt-Tab、物理手柄、真实浏览器慢网或 Docker 上线验收。协议消息和快照字段未改变，协议版本保持 9。

## 尚未完成的实机验收（需要人工）

没有物理手柄、平板或手机的实机验证。请在真实设备上逐项确认（中文界面里的按钮名：LIGHT = 轻拳、HEAVY = 重拳、DASH = 冲刺、SHOT = 奶泡、REMATCH = 再来一局、CREATE ROOM = 创建房间、LEAVE ROOM = 离开房间）：

- [ ] Android 平板（如红米 Pad，Chrome）与 iPad / Safari 横屏：菜单可点，三种模式文字无方块（tofu）；进入对局后出现半透明摇杆和圆形按钮，位置与手指一致；同一网址在电脑浏览器上不出现触屏控件。
- [ ] 平板单机：左手在左半屏任意处按下拖动摇杆（左右走、上推跳、下拉防），右手同时按 LIGHT / HEAVY / DASH / SHOT；多指不串、不卡键；拖出圆圈后摇杆跟随手指；按钮大小、位置、透明度是否顺手，是否挡住角色；切后台再回来停在菜单。
- [ ] 平板本地双人：两人各用半屏的摇杆和按钮，同时操作互不干扰。
- [ ] 平板奶泡与跳跃：菱形底部的 SHOT 是否顺手、会不会误触 LIGHT / DASH；自己的奶泡在场时 SHOT 变灰；没有跳跃按钮后，摇杆上推跳、斜上推向前跳是否好按，平时左右走会不会误跳；对方奶泡飞来时向前跳能躲过（原地跳窗口约 14 帧、向前跳约 35 帧，见“跳跃躲奶泡调参”一节）；跳起后按 LIGHT / HEAVY 出肚皮压，每跳一次。
- [ ] 真手柄：RB 出奶泡，LB / LT / RT 防御，跳起后 X / Y 出肚皮压；不同品牌手柄上 RB 的位置一致。
- [ ] PC 手柄（Xbox / PlayStation 各一）：单机时任意手柄控制 P1；本地时按连接顺序分配 1 号 / 2 号；对局中拔插手柄后按键不残留；Select 暂停 / 再按继续；Start 重开；整场结束的结果显示约 1 秒后按 A 重开整场，新回合开始时角色不会自己跳一下。
- [ ] 真手柄 · A 只在整场结束重开（2026-10-03 评审第二轮）：本地双人打完一局但未打满整场时，局间结果显示里按 A、按住 A 都不触发重开，局间自动进入下一局；一方拿到两胜（或打满五局）的结果显示约 1 秒后，新按的 A 才重开整场；R / 手柄 Start / 屏上 REMATCH 任何时候仍立即重开整场。
- [ ] 浏览器手柄（真手柄）：模式菜单 / 房间面板里十字键、左摇杆、A、B、Start、Select 是否与模拟手柄结果一致；在 `http://IP:端口` 页面（无域名部署或局域网）上是否可用（部分浏览器只在 HTTPS / localhost 提供手柄 API），以及是否需要先按一次按钮才被识别。
- [ ] 多键键盘的同时按键冲突（本地双人）。
- [ ] 复制匹配码（2026-10-04）：iPad Safari、Android 平板 Chrome、Windows / macOS 电脑浏览器，分别在 https 与 `http://IP:端口` 页面点“复制匹配码”，粘贴到聊天软件里与显示的匹配码一致；复制不了时匹配码被选中、长按或 Ctrl+C 可以复制。
- [ ] 中文名字（2026-10-04）：平板与电脑用系统输入法（拼音候选、组字中按回车）输入中文名字，建房 / 加入后双方画面上的名字正确；组字过程中的提示不误导。
- [ ] 中英文切换（2026-10-04）：真平板上三个面板右上角的“English / 中文”按钮好点，切换后网页与画面一致、刷新后保持；系统语言为英文的设备第一次打开是英文；桌面版对局中按 F2 切换，窗口标题跟着变；`run-game.ps1 -Lang en` 启动是英文。
- [ ] 无损 WebP 贴图（2026-10-03）：平板 / 手机第一次打开（空缓存）时，从下载完成到出现菜单没有明显变慢，角色、特效、背景显示正常、边缘没有杂色；开发者工具 Network 里贴图为 `image/webp`。
- [ ] 浏览器缓存（2026-10-03）：同一台平板 / 电脑第二次打开明显更快，开发者工具 Network 里 WASM 与贴图来自缓存；服务器更新后再打开只下载变化的文件；无痕窗口每次完整下载；经 Caddy / HTTPS 部署时响应头与本机一致（见 DEPLOY.md 验收第 6 项）。
- [ ] 桌面版联机：`run-game.ps1 -Server http://公网IP:端口` 启动，菜单选 3，键盘输入口令后按 Enter，手柄选 CREATE ROOM；平板浏览器用顶部状态行的匹配码加入；手柄只控制桌面一方；Select 打开菜单不暂停，B 回到对局；LEAVE ROOM 后平板看到“房间已关闭”。
- [ ] 私人联机（需要先按 [DEPLOY.md](<DEPLOY.md>) 部署：有域名用 HTTPS，没有域名用 `http://公网IP:端口`）：PC 用手柄建房、朋友用平板（红米 Pad / iPad）从另一个网络加入；手柄只控制 PC 一方，平板触屏只控制自己；两边血量与胜负一致；在真实往返延迟（例如 50–150 ms）下手感是否可以接受（没有预测 / 回滚，延迟会直接体现）。
- [ ] 无域名部署：用 `http://公网IP:端口` 打开时登录页显示明文警告；点“复制匹配码”后能粘贴出正确的匹配码，复制不了时匹配码被选中、长按可以复制；服务器日志只有一行，并带 UNENCRYPTED 提示。
- [ ] 平板切到后台或锁屏再回来：对方看到断线暂停，回来后自动恢复原席位（5 分钟内，包括重新载入页面）；只建房不加入时，等 10 分钟以上房间仍在。
- [ ] 手机 / 平板切换 Wi-Fi 与蜂窝网络后：会话绑定登录 IP，需要重新登录；确认提示可理解。
- [ ] 手机 / 平板切换 Wi-Fi 与蜂窝网络（断线重连补强一节的真机项）：切网后页面在几次重试内弹出“登录已失效（可能切换了网络），请重新输入口令。”而不是长时间盲试；重新输口令登录后提示原座位已失效；切回原网络并触发 `online` / 回到前台（`pageshow`）时同一座位直接恢复、不用重新登录；等待重新输口令期间切后台再回来，口令框与已输入的文字不被擦掉（2026-10-03 评审第二轮）；等待一方看到“对方已断线（席位保留 m:ss）”走秒；断线方顶部状态条出现“立即重试”按钮且点击立即重连。
- [ ] 手机 / 平板锁屏解锁、切 App 再回来（对局中）：回来后 `pageshow` / `visibilitychange` 立即重连；对局中途掉线的回合恢复时先播 3-2-1 再继续，期间输入无效、计时不动，两边看到的倒计时一致；开局与等人状态不播倒计时。
- [ ] 红米 Pad（Chrome，重点验收 2026-10-03 平板与触屏界面一节）：开着全面屏手势 / 圆角时摇杆与按钮让开底部与两侧的安全区（地址栏下拉也不遮按钮）；旋转到竖屏再转回横屏，控件位置正确、不出屏、不与横幅 / 名字重叠；按住菜单和房间面板的按钮能看到按下反馈；摇杆跟手、按住拖出圆圈后中心跟随。

本机 Vulkan 日志报告两个宿主覆盖层 JSON 缺失，但游戏正常选中 GPU、完成加载和截图，退出码 0。未擅自修复系统覆盖层。
