/**
 * build.mjs 的可测试纯逻辑（被 build.mjs 引用）。
 *
 * 抽出来是为了能单测：
 * - npm 的定位与免 shell 调用（不用 shell:true，避免命令注入面）；
 * - SEA 资源路径的跨平台拼接（用 path.join，不手工替换分隔符）。
 */
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';

/**
 * 定位 npm 的 CLI 入口（npm-cli.js）。
 *
 * 为什么不用 `npm` 命令本身：Windows 上 npm 是 `npm.cmd` shim，
 * Node ≥18.20/20.12 起无 shell 启动 .cmd 会直接报 EINVAL；
 * 即便能启动，也免不了 shell 解析。
 * 直接用当前 node 进程跑 npm-cli.js 即可完全绕开 shell。
 *
 * @param {string} [execPath] 当前 node 可执行文件路径
 * @param {Record<string,string|undefined>} [env] 环境变量
 * @returns {string|null} npm-cli.js 的绝对路径；找不到返回 null
 */
export function resolveNpmCli(execPath = process.execPath, env = process.env) {
  const cands = [];
  if (env.npm_execpath) cands.push(env.npm_execpath);
  // node 与 npm 装在一起时（官方安装包 / 便携版）都成立
  cands.push(join(dirname(execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js'));
  cands.push(join(dirname(execPath), '..', 'lib', 'node_modules', 'npm', 'bin', 'npm-cli.js'));
  return cands.find((p) => p && existsSync(p)) ?? null;
}

/**
 * 取 npm 全局安装根目录（等价 `npm root -g`）。
 *
 * 优先用 `process.execPath` + npm-cli.js（免 shell、参数数组传参）；
 * 找不到 npm-cli.js 时退回直接执行 `npm`（POSIX 上 npm 是可执行脚本，同样免 shell）。
 *
 * @param {(file: string, args: string[], opts: object) => string} [runner] 执行器，便于测试注入
 * @param {string|null} [cli] npm-cli.js 路径；传 null 强制走 `npm` 回退分支
 * @returns {string} 全局 node_modules 目录
 */
export function npmGlobalRoot(runner = execFileSync, cli = resolveNpmCli()) {
  if (cli) {
    return runner(process.execPath, [cli, 'root', '-g'], { encoding: 'utf8' }).trim();
  }
  return runner('npm', ['root', '-g'], { encoding: 'utf8' }).trim();
}

/**
 * SEA 资源 key（正斜杠）对应的本地文件路径。
 *
 * key 必须是 `assets/web/<相对路径>`（Node SEA 要求正斜杠），
 * 而本地路径要按平台规则拼接 —— 两者分开处理，不再手工替换分隔符。
 *
 * @param {string} assetsDir assets 根目录
 * @param {string} rel 相对 assets/web 的路径（正斜杠）
 * @returns {string} 本地路径
 */
export function seaAssetPath(assetsDir, rel) {
  return join(assetsDir, 'web', ...rel.split('/'));
}
