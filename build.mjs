#!/usr/bin/env node
/**
 * TokenWatcher 单文件 Windows exe 构建脚本
 *
 * 链路：token-watcher (ESM 源) → 打补丁 → esbuild CJS bundle → Node SEA → postject 注入 → 单 exe
 *
 * 用法：
 *   node build.mjs [源目录]
 *   node build.mjs E:/npm-global/node_modules/token-watcher
 *
 * 产物：dist/TokenWatcher.exe（内嵌 web/ + echarts + zstd.exe，无外部依赖）
 *
 * 设计要点（为什么这样改）：
 * 1. SEA 的 main 只吃 CommonJS（Node 22 实测），故 esbuild 必须 --format=cjs。
 * 2. bin/tokenwatcher.js 有顶层 await，CJS 不支持 → 整段 CLI 分发逻辑包进 async main()。
 * 3. import.meta.dirname 在 bundle+SEA 下失效 → 改用 process.execPath 推导。
 * 4. 静态资源不逐请求读 SEA asset（避免 MIME/流式复杂度），改为首次启动释放到
 *    %LOCALAPPDATA%\TokenWatcher\runtime\<version>\，WEB_DIR 指过去即可，服务端零改动。
 */
import { build as esbuildBuild } from 'esbuild';
import { inject } from 'postject';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync,
  rmSync, statSync, writeFileSync,
} from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
const BUILD = join(ROOT, '.build');
const DIST = join(ROOT, 'dist');
const EXE_NAME = 'TokenWatcher.exe';
const FUSE = 'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2';

const log = (m) => console.log(m);

/* ------------------------------------------------------------------ 源定位 */

function findSource() {
  if (process.argv[2]) {
    const p = resolve(process.argv[2]);
    if (!existsSync(join(p, 'package.json'))) throw new Error(`源目录无效（缺 package.json）：${p}`);
    return p;
  }
  try {
    const globalRoot = execFileSync('npm', ['root', '-g'], { encoding: 'utf8', shell: true }).trim();
    const p = join(globalRoot, 'token-watcher');
    if (existsSync(join(p, 'package.json'))) return p;
  } catch { /* 落到抛错 */ }
  throw new Error('未找到 token-watcher：请把源目录作为第 1 个参数传入');
}

function findZstd() {
  const cands = [
    process.env.TW_ZSTD,
    process.env.USERPROFILE ? join(process.env.USERPROFILE, '.workbuddy', 'bin', 'zstd.exe') : null,
  ].filter(Boolean);
  return cands.find((p) => existsSync(p)) || null;
}

function walk(dir, base = dir) {
  const out = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, e.name);
    if (e.isDirectory()) out.push(...walk(full, base));
    else out.push(relative(base, full).split('\\').join('/'));
  }
  return out.sort();
}

/* -------------------------------------------------------------- 补丁工具 */

function patch(file, pairs) {
  const p = join(BUILD, file);
  let s = readFileSync(p, 'utf8');
  for (const [from, to, label] of pairs) {
    if (!s.includes(from)) throw new Error(`[patch] 锚点缺失 → ${file} :: ${label || from.slice(0, 70)}`);
    s = s.split(from).join(to);
  }
  writeFileSync(p, s);
  log(`  ✓ ${file}`);
}

/* ------------------------------------------------------------- 主构建流程 */

const SRC = findSource();
// 分发版本以本仓 package.json 为权威源（本仓 = 打包与托盘封装层）；
// 上游 token-watcher 的版本只作为「引擎版本」另行打印，避免两者混淆。
const VERSION = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8')).version;
const ENGINE_VERSION = JSON.parse(readFileSync(join(SRC, 'package.json'), 'utf8')).version;
const ZSTD = findZstd();

log(`\n=== TokenWatcher 单文件构建 ===`);
log(`源      : ${SRC}`);
log(`版本    : v${VERSION}（本仓分发版本）`);
log(`引擎    : token-watcher v${ENGINE_VERSION}`);
log(`Node    : ${process.version} (${process.execPath})`);
log(`zstd    : ${ZSTD || '未找到 → dsh 源将降级'}`);
log(`输出    : ${join(DIST, EXE_NAME)}\n`);

// 0. 清理 + 复制源
rmSync(BUILD, { recursive: true, force: true });
mkdirSync(BUILD, { recursive: true });
mkdirSync(DIST, { recursive: true });
for (const d of ['src', 'bin', 'web']) {
  cpSync(join(SRC, d), join(BUILD, d), { recursive: true });
}
copyFileSync(join(SRC, 'package.json'), join(BUILD, 'package.json'));
log('[1/7] 复制源完成');

// 1. 组装 assets 目录（web + echarts + zstd）
const ASSETS = join(BUILD, 'assets');
cpSync(join(BUILD, 'web'), join(ASSETS, 'web'), { recursive: true });

// echarts：优先从源包的依赖树解析（全局安装时被提升到顶层 node_modules）
function findEcharts() {
  const cands = [
    join(dirname(SRC), 'echarts', 'dist', 'echarts.min.js'),
    join(SRC, 'node_modules', 'echarts', 'dist', 'echarts.min.js'),
  ];
  return cands.find((p) => existsSync(p)) || null;
}
const echartsPath = findEcharts();
if (!echartsPath) throw new Error('找不到 echarts.min.js，无法内嵌图表库');
mkdirSync(join(ASSETS, 'web', 'vendor'), { recursive: true });
copyFileSync(echartsPath, join(ASSETS, 'web', 'vendor', 'echarts.min.js'));

let zstdBundled = false;
if (ZSTD) {
  mkdirSync(join(ASSETS, 'bin'), { recursive: true });
  copyFileSync(ZSTD, join(ASSETS, 'bin', 'zstd.exe'));
  zstdBundled = true;
}
const webFiles = walk(join(ASSETS, 'web'));
log(`[2/7] assets 就绪：web ${webFiles.length} 文件 + echarts${zstdBundled ? ' + zstd' : ''}`);

// 2. 打补丁
log('[3/7] 打补丁');

// 2.1 config.js 头部：注入运行时资源释放
const CONFIG_HEADER = `import __twSea from 'node:sea';
import { mkdirSync as __twMkdir, writeFileSync as __twWrite, existsSync as __twExists } from 'node:fs';
import { dirname as __twDirname, join as __twJoin } from 'node:path';

const __TW_VERSION = '${VERSION}';
const __TW_WEB_FILES = ${JSON.stringify(webFiles)};

function ensureRuntimeAssets() {
  try { if (!__twSea.isSea()) return null; } catch { return null; }
  const base = process.env.LOCALAPPDATA || process.env.XDG_DATA_HOME || process.env.HOME || '.';
  const root = __twJoin(base, 'TokenWatcher', 'runtime', __TW_VERSION);
  const zstdBin = __twJoin(root, 'bin', 'zstd.exe');
  if (__twExists(__twJoin(root, '.ready'))) {
    if (__twExists(zstdBin)) process.env.TW_ZSTD_BIN = zstdBin;
    return root;
  }
  for (const rel of __TW_WEB_FILES) {
    const dest = __twJoin(root, 'web', rel);
    __twMkdir(__twDirname(dest), { recursive: true });
    __twWrite(dest, Buffer.from(__twSea.getAsset('assets/web/' + rel)));
  }
  try {
    __twMkdir(__twDirname(zstdBin), { recursive: true });
    __twWrite(zstdBin, Buffer.from(__twSea.getAsset('assets/bin/zstd.exe')), { mode: 0o755 });
    process.env.TW_ZSTD_BIN = zstdBin;
  } catch { /* 未内嵌 zstd：dsh 源自动降级 */ }
  __twWrite(__twJoin(root, '.ready'), __TW_VERSION);
  return root;
}

const __TW_RUNTIME = ensureRuntimeAssets();

`;
const cfgPath = join(BUILD, 'src/config.js');
writeFileSync(cfgPath, CONFIG_HEADER + readFileSync(cfgPath, 'utf8'));

patch('src/config.js', [
  [
    `export const WEB_DIR = join(import.meta.dirname, '..', 'web');`,
    `export const WEB_DIR = __TW_RUNTIME ? join(__TW_RUNTIME, 'web') : join(import.meta.dirname, '..', 'web');`,
    'WEB_DIR',
  ],
  [
    `  try {\n    return createRequire(import.meta.url).resolve('echarts/dist/echarts.min.js');`,
    `  if (__TW_RUNTIME) return join(__TW_RUNTIME, 'web', 'vendor', 'echarts.min.js');\n  try {\n    return createRequire(import.meta.url).resolve('echarts/dist/echarts.min.js');`,
    'ECHARTS_PATH',
  ],
]);

// 2.2 dsh collector：优先用内嵌释放出来的 zstd
patch('src/collectors/dsh.js', [
  [
    `const ZSTD_BINS = ['zstd', '/opt/homebrew/bin/zstd', '/usr/local/bin/zstd', '/usr/bin/zstd'];`,
    `const ZSTD_BINS = [process.env.TW_ZSTD_BIN, 'zstd', '/opt/homebrew/bin/zstd', '/usr/local/bin/zstd', '/usr/bin/zstd'].filter(Boolean);`,
    'ZSTD_BINS',
  ],
]);

// 2.3 macOS 菜单栏/自启：import.meta.dirname 失效，改由 execPath 推导
patch('src/agent.js', [
  [`join(import.meta.dirname, '..', 'bin', 'tokenwatcher.js')`, `join(process.execPath, '..', 'tokenwatcher.js')`, 'agent entryScript'],
]);
patch('src/bar.js', [
  [`join(import.meta.dirname, '..', 'bin', 'token-watcher.app')`, `join(process.execPath, '..', 'token-watcher.app')`, 'bar appPath'],
]);

// 2.4 CLI：版本硬编码 + 顶层 await 收进 async main
const binPath = join(BUILD, 'bin/tokenwatcher.js');
let binSrc = readFileSync(binPath, 'utf8');
const verAnchor = `const VERSION = JSON.parse(readFileSync(join(import.meta.dirname, '..', 'package.json'), 'utf8')).version;`;
if (!binSrc.includes(verAnchor)) throw new Error('[patch] 锚点缺失 → bin VERSION');
binSrc = binSrc.replace(verAnchor, `const VERSION = '${VERSION}';`);

const mainAnchor = 'if (opts.version)';
if (!binSrc.includes(mainAnchor)) throw new Error('[patch] 锚点缺失 → bin main');
const lines = binSrc.split('\n');
const mainIdx = lines.findIndex((l) => l.startsWith(mainAnchor));
lines.splice(mainIdx, 0, 'async function main() {');
lines.push('}', 'main().catch((e) => { console.error(e?.stack ?? e); process.exit(1); });');
writeFileSync(binPath, lines.join('\n'));
log('  ✓ bin/tokenwatcher.js');

// 3. esbuild 打包（必须 CJS：SEA main 只支持 CommonJS）
log('[4/7] esbuild 打包');
const bundleOut = join(BUILD, 'app.cjs');
await esbuildBuild({
  entryPoints: [join(BUILD, 'bin/tokenwatcher.js')],
  outfile: bundleOut,
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node22',
  external: ['node:*'],
  logLevel: 'warning',
  banner: {
    // SEA 下 process.argv[0] 已是 exe 自身；CJS 顶层无 import.meta，保持干净
    js: '/* TokenWatcher standalone build — bundled from token-watcher (MIT, luwill) */',
  },
});
log(`  ✓ app.cjs ${(statSync(bundleOut).size / 1024).toFixed(0)} KB`);

// 4. SEA blob（含 assets）
log('[5/7] 生成 SEA blob');
const seaAssets = {};
for (const rel of webFiles) seaAssets[`assets/web/${rel}`] = join(ASSETS, 'web', rel.split('/').join('\\'));
if (zstdBundled) seaAssets['assets/bin/zstd.exe'] = join(ASSETS, 'bin', 'zstd.exe');

const seaConfigPath = join(BUILD, 'sea-config.json');
const blobPath = join(BUILD, 'sea.blob');
writeFileSync(seaConfigPath, JSON.stringify({
  main: bundleOut,
  output: blobPath,
  disableExperimentalSEAWarning: true,
  assets: seaAssets,
}, null, 2));

execFileSync(process.execPath, ['--experimental-sea-config', seaConfigPath], { stdio: 'inherit' });
log(`  ✓ sea.blob ${(statSync(blobPath).size / 1048576).toFixed(1)} MB`);

// 5. 注入生成单 exe
log('[6/7] postject 注入');
const outExe = join(DIST, EXE_NAME);
copyFileSync(process.execPath, outExe);
await inject(outExe, 'NODE_SEA_BLOB', readFileSync(blobPath), { sentinelFuse: FUSE });
log('  ✓ 注入完成（原 Node 签名失效属正常，企业分发需自行重签）');

// 6. 编译托盘宿主（Rust，零第三方依赖）
log('[7/7] 编译托盘宿主');
const trayDir = join(ROOT, 'tray');
const trayBuilt = join(trayDir, 'target', 'release', 'TokenWatcherTray.exe');
let trayOk = false;
if (existsSync(join(trayDir, 'Cargo.toml')) && existsSync(join(trayDir, 'icon.ico'))) {
  try {
    execFileSync('cargo', ['build', '--release', '--manifest-path', join(trayDir, 'Cargo.toml')], {
      stdio: 'inherit',
      cwd: trayDir,
    });
    copyFileSync(trayBuilt, join(DIST, 'TokenWatcherTray.exe'));
    trayOk = true;
    log('  ✓ TokenWatcherTray.exe');
  } catch (e) {
    log(`  ! 托盘编译失败，已跳过：${e.message.split('\n')[0]}`);
  }
} else {
  log('  ! 跳过托盘：缺 tray/Cargo.toml 或 tray/icon.ico');
}

// 7. 报告
const size = statSync(outExe).size;
const sha = createHash('sha256').update(readFileSync(outExe)).digest('hex');
const traySize = trayOk ? statSync(join(DIST, 'TokenWatcherTray.exe')).size : 0;
log(`\n=== 产物 ===`);
log(`主程序  : ${outExe}`);
log(`大小    : ${(size / 1048576).toFixed(1)} MB (${size} B)`);
log(`sha256  : ${sha}`);
log(`内嵌    : web ${webFiles.length} 文件 / echarts / ${zstdBundled ? 'zstd' : '无 zstd'}`);
if (trayOk) {
  log(`托盘    : ${join(DIST, 'TokenWatcherTray.exe')} (${(traySize / 1024).toFixed(0)} KB)`);
}
log(`\n验证：`);
log(`  "${outExe}" --version`);
log(`  "${outExe}" today --light`);
log(`  "${join(DIST, 'TokenWatcherTray.exe')}"            # 静默起服务 + 托盘`);
log(`  "${join(DIST, 'TokenWatcherTray.exe')}" --set-autostart on`);
