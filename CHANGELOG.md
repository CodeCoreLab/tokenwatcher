# 变更日志 — Token 用量看板（tokenwatcher）

本文件记录本项目的变更。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

## [1.5.2] - 2026-11-06
### 修复

- **`copy_to_buf` 长度恰好相等时不再丢最后一个字符**
- `trace()` 写入格式为纯文本，无时间戳与级别，排障时无法判断先后与严重度 →
- `tray.log` / `serve.log` 单文件无限增长、无轮转 → 按日切分
- 时间戳实现守住「零第三方依赖」承诺：直接调用 kernel32 `GetLocalTime`，
- 启动后端服务失败只弹 MessageBox、不落日志，事后无迹可查 →

### 安全

- **构建脚本不再经 shell 调用 npm**：原用 `shell: true` 拼接命令，且资源路径硬编码 Windows 反斜杠；改为参数数组传参 + `path.join` 拼接
## [1.5.1] - 2026-10-26

### 新增

- 安装版与便携版双分发：安装版免 UAC、自启项跟随安装路径；
  便携版解压即用，解压到 U 盘可携带
- 托盘宿主 `TokenWatcherTray.exe` 与主程序 `TokenWatcher.exe` 分离：
  托盘无窗口静默驻留，主程序负责 13 个数据源采集与 Web 面板服务
- `TokenWatcher.ini` 配置：端口 / 是否自动开面板 / 是否开机自启，
  首次运行自动生成默认值，写回后始终反映真实状态

### 变更

- 打包方式：Node 22 SEA 单可执行文件（内嵌 Web 面板、ECharts、zstd）
  + Rust 托盘宿主，零 Node / npm 依赖、零联网
- 面板地址固定为 `http://127.0.0.1:8788/`，数据落盘于本机 `.tokenmeter`

### 修复

- 托盘自启项原先绑定解压路径，移动目录后失效；改为自启项跟随安装路径

## [1.5.0] - 2026-10-26

### 新增

- 引擎升级到上游 `token-watcher` v1.5.0（MIT）

### 变更

- 13 个 AI 编码工具的数据源目录汇聚：Claude Code、Codex、Cursor、
  WorkBuddy 等会话记录统一采集
