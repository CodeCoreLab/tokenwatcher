# 贡献指南

感谢你愿意为 TokenWatcher 出力。

本项目是 **Node.js 22 SEA 单文件 + Rust 托盘宿主** 的 Windows 工具，
工程要求只有一条：**能跑、测得住、说得清**。

---

## 快速判断：改哪个目录

| 你想改的东西 | 去哪 |
|---|---|
| 打包逻辑 / 补丁锚点 | `build.mjs` |
| 托盘宿主（Win32 交互） | `tray/src/main.rs` |
| 安装包脚本 | `tokenwatcher.nsi` |
| 依赖与许可声明 | `THIRD-PARTY-NOTICES.md` |
| 变更记录 | `CHANGELOG.md` |
| Issue / PR 模板 | `.github/` |

**采集引擎与 Web 面板不在本仓** —— 它们来自上游
[`token-watcher`](https://github.com/luwill/token-watcher)（MIT）。本仓负责
**打包、托盘封装、安装包与版本发布**。改引擎请向上游提 PR。

---

## 版本号只有一个权威源

`package.json` 的 `version` 是本仓的**唯一版本真相源**：

| 位置 | 值 | 说明 |
|---|---|---|
| `package.json` | `1.5.1` | **权威源**，改这里 |
| `package-lock.json` | `1.5.1` | 跟随 `package.json`，勿手工改 |
| `tokenwatcher.nsi` | `APP_VERSION "1.5.1"` | 跟随 `package.json`，勿手工改 |

`build.mjs` 里的 `VERSION` 读的是本仓 `package.json`；上游引擎版本单独以
`ENGINE_VERSION` 打印，**两者不可混用**。

发版时 `CHANGELOG.md` 必须新增对应的版本段，格式见该文件头部说明。

---

## 开发环境

| 必需 | 版本 | 用途 |
|---|---|---|
| Node.js | 22.x | SEA 单文件打包（SEA 目前只吃 CJS） |
| Rust | stable-x86_64-pc-windows-msvc | 编译托盘宿主 |
| NSIS | 3.10 | 生成安装包（可选，缺失则只出便携版） |

不需要数据库、不需要联网服务。

### 构建

```bash
npm install
node build.mjs E:/npm-global/node_modules/token-watcher   # 第 1 参数是上游源目录
```

产物落在 `dist/`：

| 文件 | 说明 |
|---|---|
| `TokenWatcher.exe` | 主程序（内嵌 web 面板 + ECharts + zstd） |
| `TokenWatcherTray.exe` | 托盘宿主（无窗口静默驻留） |
| `TokenWatcher.ini` | 运行时配置示例 |

安装包（需 NSIS）：

```bash
.tools/nsis-3.10/makensis.exe tokenwatcher.nsi
```

---

## 验证要求

改完必须实测，**不接受「看起来对」**：

```bash
dist/TokenWatcher.exe --version
dist/TokenWatcher.exe today --light
```

托盘宿主至少确认：能静默起服务、托盘图标出现、右键菜单可用、
退出时子进程被收掉、开机自启项指向**当前安装路径**。

---

## 提交约定

| 类型 | 前缀 | 例子 |
|---|---|---|
| 功能 | `feat:` | `feat(tray): 支持托盘图标角标提示` |
| 修复 | `fix:` | `fix(nsi): 覆盖安装不冲掉用户 ini` |
| 打包 | `build:` | `build: 版本源对齐 1.5.0 → 1.5.1` |
| 文档 | `docs:` | `docs: 补充托盘配置项说明` |
| 版本记录 | `docs(changelog):` | 由发版脚本自动生成 |

**跨项目的改动**（例如统一错误处理风格）请开一个 PR 只做这件事，
不要夹带在某个功能 PR 里——这类改动 review 成本高，混在一起会拖慢合并。

---

## 许可证

MIT，见 [`LICENSE`](LICENSE)。新增依赖请登记到
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)。
