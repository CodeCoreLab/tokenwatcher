# TokenWatcher（Windows x64）

**不需要装 Node、不需要 npm、不需要联网。**

> 在线产品页：<https://xmtooldev.gf.com.cn/tokenwatcher/>
> 版本记录（变更日志）：[`CHANGELOG.md`](CHANGELOG.md) · 发行版下载：<https://github.com/CodeCoreLab/tokenwatcher/releases>

## 两种装法

| 方式 | 适合 | 差别 |
|---|---|---|
| **安装版** `TokenWatcher-1.5.1-win-x64-setup.exe` | 给同事分发 | 装到 `%LOCALAPPDATA%\Programs\TokenWatcher`，**免 UAC**；带开始菜单/桌面快捷方式与控制面板卸载项；**自启项跟着安装路径走，不会因移动目录失效** |
| **便携版** `TokenWatcher-1.5.1-win-x64.zip` | 临时试用、U 盘携带 | 解压到任意目录即用，不写安装记录；但自启项绑定解压路径，移动目录后需重设一次 |

两种方式用的是同一套可执行文件，功能完全一致。

| 文件 | 作用 | 日常用哪个 |
|---|---|---|
| `TokenWatcherTray.exe` | 托盘宿主：**无窗口**后台拉起服务、托盘图标、开机自启开关 | ✅ 双击这个 |
| `TokenWatcher.exe` | 主程序（13 个数据源的采集引擎 + Web 面板服务） | 由托盘拉起；也可当命令行工具 |
| `TokenWatcher.ini` | 配置文件：端口 / 是否自动开面板 / 是否开机自启 | 想改行为时编辑它 |

- 引擎：`token-watcher` v1.5.1（MIT，上游 <https://github.com/luwill/token-watcher>）
- 打包方式：Node 22 SEA 单可执行文件（内嵌 Web 面板、ECharts、zstd）+ Rust 托盘宿主
- 面板地址：<http://127.0.0.1:8788/>

## 配置文件 `TokenWatcher.ini`

放在 exe 同目录，UTF-8 编码。**改完需要重启托盘生效**（右键图标 → 退出 → 重新双击 exe）。

```ini
[server]
port = 8788

[behavior]
open_panel = true      ; 手动双击启动时自动打开面板（开机自启拉起时不弹）
autostart = true       ; 开机自动启动（写入 HKCU Run，免管理员）
```

| 配置项 | 取值 | 默认 | 说明 |
|---|---|---|---|
| `port` | 1–65535 | 8788 | 面板端口。被占用就改成 8789 之类 |
| `open_panel` | true / false | **true** | **手动双击**启动时是否自动打开浏览器。**开机自启拉起时一律不弹窗**，与本项无关 |
| `autostart` | true / false | **true** | true = 启动时确保写入开机自启；false = 启动时移除该项 |

三个要点：

1. **文件不存在时会自动生成默认值**，删掉也能恢复。
2. **`autostart` 会回写**：右键菜单勾选/取消，或命令行 `--set-autostart`，都会同步改写 ini 这一行，所以 ini 始终反映真实状态。
3. 文件里可写 `;` 开头的注释，解析时忽略；写错的值（如 `port = abc`）会退回默认，不影响启动。

常用写法也可以只写一行，section 头可省（解析时不强依赖）：

```ini
port = 8790
open_panel = false
autostart = false
```

## 首次使用

1. 双击 `TokenWatcherTray.exe`。
2. **不会弹任何黑窗口**，但会在右下角托盘区出现一个图标（绿色圆环）。
   - 如果没看到，点任务栏的 **`^`** 展开隐藏图标区 —— Windows 11 默认把新图标收在那里，可拖到任务栏固定。
   - 首次运行会弹一次气泡提示，右下角消息中心也能找到。
3. 图标出现的几秒内服务已在后台启动，**双击图标**即可打开面板。

## 托盘菜单（右键图标）

| 菜单项 | 作用 |
|---|---|
| 打开面板 | 浏览器打开 `http://127.0.0.1:8788/`（服务没起会先静默拉起） |
| 立即扫描一次 | 强制重新扫描所有数据源，不等 60 秒轮询 |
| **开机自动启动** | 勾选即写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`（**免管理员**），再点取消 |
| 打开日志 | 打开 `%LOCALAPPDATA%\TokenWatcher\logs\serve.log` |
| 打开数据目录 | 打开 `%USERPROFILE%\.tokenmeter`（统计数据库） |
| 编辑配置 | 用记事本打开 `TokenWatcher.ini` |
| 退出（停止服务） | 同时停掉后台服务进程，不留残余 |

**双击图标** = 打开面板；**再双击一次 `TokenWatcherTray.exe`** 也只会打开面板（不会起第二个实例）。

## 开机自启

**`TokenWatcher.ini` 里的 `autostart` 是唯一真源**，注册表跟着它走。设置方式任选：

- 右键托盘图标 → 勾选/取消「开机自动启动」（会同步回写 ini）
- 直接改 ini 的 `autostart = true / false`（重启托盘生效）
- 命令行（会同步回写 ini）：
  ```bat
  TokenWatcherTray.exe --set-autostart on
  TokenWatcherTray.exe --set-autostart off
  TokenWatcherTray.exe --query-autostart
  ```
  最后一条：已开启返回 0，未开启返回 1。

只写当前用户注册表（`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`），不写系统级、不申请管理员权限。
写入的命令行带 `--autostart` 标记，托盘靠它识别「我是被开机拉起的」。

**关键行为：开机自启拉起时静默驻留托盘，不弹浏览器。** 想看面板就双击托盘图标，
或再双击一次 `TokenWatcherTray.exe`（会直接打开面板）。

## 命令行用法（`TokenWatcher.exe`）

需要在终端里敲（双击它默认是起服务，会留黑窗口，日常别用这个入口）：

```bat
TokenWatcher.exe today --light            :: 今日用量速览
TokenWatcher.exe today --json             :: 机器可读
TokenWatcher.exe sessions --day 2026-10-26
TokenWatcher.exe sessions --from 2026-10-01 --to 2026-10-31 --csv --out s.csv
TokenWatcher.exe wrapped --year 2026      :: 年度报告
TokenWatcher.exe doctor                   :: 环境与 13 个数据源体检
TokenWatcher.exe scan                     :: 只扫描一次，不起服务
TokenWatcher.exe serve --port 8788 --no-open
```

## 数据与隐私

- 数据**只**来自本机：`~/.claude`、`~/.codex`、`~/.zcode`、`~/.dsh`、`~/.WorkBuddy`、`~/.grok`、
  `~/.pi`、`~/.local/share/opencode`、`~/.gemini/antigravity*`、`~/.kimi-code`、`~/.qoder*`、Cursor 导出。
- 统计数据库：`%USERPROFILE%\.tokenmeter\tokenmeter.db`，不上传任何地方。
- 面板只监听 `127.0.0.1` 并校验 Host（挡 DNS rebinding），同网段其他人访问不到。
- 出站请求都不带用量数据：汇率、LiteLLM 模型牌价、厂商余额（带本地 key）、Claude 官方配额、Cursor 账单。
  断网时走本地缓存，功能不受影响。

## 常见问题

**托盘图标找不到**
点任务栏 `^` 展开隐藏区。想长期显示就把它拖到任务栏上，或在
「设置 → 个性化 → 任务栏 → 其他系统托盘图标」里打开 TokenWatcher。

**端口 8788 被占用**
改 `TokenWatcher.ini` 的 `port = 8788` 为别的端口（如 8789），重启托盘即可。
托盘、服务、面板地址三处会一起用新端口，不用另外改。

**Windows 提示"已保护你的电脑"（SmartScreen）**
exe 未做代码签名，首次运行会被拦一次：点「更多信息」→「仍要运行」。
团队内网分发建议让 IT 把这两个 exe 加白名单。

**面板空白 / 图表不显示**
跑 `TokenWatcher.exe doctor`；若提示 web 资源缺失，删掉
`%LOCALAPPDATA%\TokenWatcher\runtime` 再重启托盘，会自动重新释放。

**想彻底退出**
右键托盘图标 → 退出（停止服务）。仅关浏览器标签不会停服务。

## 卸载

**安装版**：控制面板 →「程序和功能」→ TokenWatcher → 卸载（或开始菜单里的「卸载 TokenWatcher」）。
卸载会先停掉服务、清掉自启项，然后问一次「是否连统计数据一起删」——选否就保留，重装后可接着用。

**便携版**：

1. 右键托盘图标 → 退出（停止服务）。
2. 若开过自启：`TokenWatcherTray.exe --set-autostart off`。
3. 删除本目录。
4. 按需删除数据：`%USERPROFILE%\.tokenmeter`（统计库）、`%LOCALAPPDATA%\TokenWatcher`（运行时资源与日志）。

## 已知限制

- 仅 Windows x64。macOS / Linux 需在对应平台各自构建（Node SEA 不支持交叉编译，托盘代码也是 Win32）。
- 无自动更新：上游 token-watcher 升级后需重新构建（`node build.mjs`）。
- 命令别名只有 `TokenWatcher.exe`，不再有 `token-watcher` / `tokenmeter` 三个名字。
- 托盘图标是静态的，不随今日用量变化（做动态图标需要频繁重绘，不值得）。
