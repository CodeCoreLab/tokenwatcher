# 第三方组件与许可声明 — tokenwatcher（Token 用量看板）

本文件列出本项目使用的第三方组件及其许可协议。

**本仓库不包含任何第三方二进制文件**——所有依赖一律通过语言原生的包管理机制
（`package.json` / `Cargo.toml`）在构建时还原。克隆本仓库后执行对应的构建命令，
即可获得完整的依赖树。

## 技术栈

Node.js 22（SEA 单文件打包）+ Rust（托盘宿主）

| 组件 | 版本 | 许可 | 用途 |
|---|---|---|---|
| [esbuild](https://github.com/evanw/esbuild) | 0.28.2 | MIT | 打包 ESM 源码为 CommonJS bundle（SEA 只吃 CJS） |
| [postject](https://github.com/nodejs/postject) | 1.0.0-alpha.6 | MIT | 向 Node SEA blob 注入资源资产 |
| [token-watcher](https://github.com/luwill/token-watcher) | 1.5.1 | MIT | 采集引擎与 Web 面板的上游实现（本仓为打包与托盘封装） |

## Rust 侧

托盘宿主 `tray/` **零第三方依赖**，仅使用标准库（含 Win32 API via `windows-sys` 之外的
手工 extern 声明），故无额外许可需要登记。

## 附带的原生二进制

构建产物会内嵌 `zstd.exe`（可选，用于 `.dsh` 会话日志解压）与 `echarts.min.js`
（Apache-2.0 / ECharts 官方发布）。二者均为构建期从上游获取，**不入库**。

- ECharts — Apache License 2.0 — <https://github.com/apache/echarts>
- zstd — BSD-3-Clause — <https://github.com/facebook/zstd>

## 上游许可

`token-watcher` 上游采用 MIT 协议（Copyright (c) 2026 TokenMeter contributors）。
本仓对其的使用限于打包分发与托盘封装，完整上游许可随分发物一并附带。
