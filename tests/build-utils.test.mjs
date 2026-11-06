/**
 * A38 回归测试：构建脚本不得使用 `shell: true` 调用外部命令，
 * 且 SEA 资源路径必须按平台规则（path.join）拼接，不能手工拼反斜杠。
 *
 * 运行：node --test tests/
 */
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { npmGlobalRoot, resolveNpmCli, seaAssetPath } from '../build-utils.mjs';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

test('A38: npm 调用不经 shell（参数数组传参，opts 不含 shell）', () => {
  const calls = [];
  const runner = (file, args, opts) => {
    calls.push({ file, args, opts });
    return 'C:\\npm-global\\node_modules\n';
  };
  const root = npmGlobalRoot(runner);

  assert.equal(root, 'C:\\npm-global\\node_modules');
  assert.equal(calls.length, 1);
  const [call] = calls;
  assert.ok(Array.isArray(call.args), '参数必须是数组');
  assert.ok(!('shell' in call.opts), 'opts 不得包含 shell 选项');
  assert.equal(call.opts.shell, undefined);
  // 走 npm-cli.js 路线时，命令是 node 本身，参数是 npm 脚本路径 + 子命令
  assert.equal(call.file, process.execPath);
  assert.equal(call.args.length, 3);
  assert.ok(call.args[0].endsWith('npm-cli.js'), '应定位到 npm-cli.js');
  assert.deepEqual(call.args.slice(1), ['root', '-g']);
});

test('A38: 找不到 npm-cli.js 时退回直接执行 npm，同样免 shell', () => {
  const calls = [];
  const runner = (file, args, opts) => {
    calls.push({ file, args, opts });
    return '/usr/lib/node_modules\n';
  };
  // 用一个必定不存在的 execPath 模拟「无 npm-cli.js」
  const original = process.env.npm_execpath;
  delete process.env.npm_execpath;
  try {
    const cli = resolveNpmCli('/nonexistent/node');
    assert.equal(cli, null, '模拟环境下不应找到 npm-cli.js');
  } finally {
    if (original !== undefined) process.env.npm_execpath = original;
  }
  assert.equal(npmGlobalRoot(runner, null), '/usr/lib/node_modules');
  assert.equal(calls[0].file, 'npm');
  assert.deepEqual(calls[0].args, ['root', '-g']);
  assert.ok(!('shell' in calls[0].opts));
});

test('A38: SEA 资源本地路径按平台规则拼接，不手工拼反斜杠', () => {
  const assets = join('C:', 'build', 'assets');
  const p = seaAssetPath(assets, 'vendor/echarts.min.js');

  assert.equal(p, join(assets, 'web', 'vendor', 'echarts.min.js'));
  // POSIX 上不得出现反斜杠（这正是旧实现的缺陷）
  if (process.platform !== 'win32') {
    assert.ok(!p.includes('\\'), `路径不应含反斜杠：${p}`);
  }
  // 归一化后必须仍指向同一个文件
  assert.equal(p, join(assets, 'web', 'vendor', 'echarts.min.js'));
});

test('A38: build.mjs 源码中不再出现 shell:true 与手工反斜杠拼接', () => {
  const src = readFileSync(join(ROOT, 'build.mjs'), 'utf8');
  assert.ok(!/shell\s*:\s*true/.test(src), 'build.mjs 不得使用 shell: true');
  assert.ok(
    !/join\('\\\\'\)/.test(src) && !/\.join\('\\\\'\)/.test(src),
    'build.mjs 不得手工用反斜杠拼接路径',
  );
  assert.ok(src.includes('seaAssetPath'), 'build.mjs 应改用 seaAssetPath 拼接资源路径');
});

test('A38: build-utils.mjs 中 npm-cli.js 入口真实可解析', () => {
  const cli = resolveNpmCli();
  if (!cli) return; // 环境里没有随 node 分发的 npm —— 跳过
  assert.ok(existsSync(cli), `npm-cli.js 应存在：${cli}`);
});
