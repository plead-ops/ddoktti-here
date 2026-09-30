import { readFileSync, writeFileSync, readdirSync, mkdirSync, copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
const config = JSON.parse(readFileSync('apps/desktop/src-tauri/tauri.conf.json', 'utf8'));
const version = config.version;
const [mode, platform, rootArg] = process.argv.slice(2);
if (process.env.GITHUB_REF_TYPE === 'tag' && process.env.GITHUB_REF_NAME !== `v${version}`) throw Error('Tag and app version differ');
const root = resolve(rootArg || 'release-assets');
const base = `https://github.com/plead-ops/ddoktti-here/releases/download/v${version}/`;
const one = (dir, suffix) => {
  const names = readdirSync(dir).filter(n => n.endsWith(suffix));
  if (names.length !== 1) throw Error(`Expected one ${suffix} in ${dir}: ${names}`);
  return join(dir, names[0]);
};
if (mode === 'prepare') {
  mkdirSync(root, { recursive: true });
  const bundle = platform === 'windows' ? 'apps/desktop/src-tauri/target/release/bundle' : 'apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle';
  const source = platform === 'windows' ? one(join(bundle, 'nsis'), '-setup.exe') : one(join(bundle, 'macos'), '.app.tar.gz');
  const name = platform === 'windows' ? `ddoktti-here_${version}_x64-setup.exe` : `ddoktti-here_${version}_universal.app.tar.gz`;
  copyFileSync(source, join(root, name));
  copyFileSync(source + '.sig', join(root, name + '.sig'));
  if (platform === 'macos') copyFileSync(one(join(bundle, 'dmg'), '.dmg'), join(root, `ddoktti-here_${version}_universal.dmg`));
  const entry = { signature: readFileSync(source + '.sig', 'utf8').trim(), url: base + name };
  const platforms = platform === 'windows' ? { 'windows-x86_64': entry } : { 'darwin-aarch64': entry, 'darwin-x86_64': entry };
  writeFileSync(join(root, `${platform}.json`), JSON.stringify({ version, platforms }, null, 2));
} else if (mode === 'merge') {
  const platforms = {};
  for (const os of ['windows', 'macos']) {
    const part = JSON.parse(readFileSync(join(root, `${os}.json`), 'utf8'));
    if (part.version !== version) throw Error('Mixed release versions');
    Object.assign(platforms, part.platforms);
  }
  if (Object.keys(platforms).sort().join() !== ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64'].join()) throw Error('Missing release platform');
  const temp = mkdtempSync(join(tmpdir(), 'ddoktti-release-'));
  try {
    const pub = join(temp, 'updater.pub');
    writeFileSync(pub, Buffer.from(config.plugins.updater.pubkey, 'base64'));
    for (const entry of new Map(Object.values(platforms).map(e => [e.url, e])).values()) {
      if (!entry.url.startsWith(base)) throw Error('Unexpected asset URL');
      const name = entry.url.slice(base.length);
      if (name.includes('/') || name.includes('..')) throw Error('Invalid asset name');
      const file = join(root, name);
      if (readFileSync(file).length < 1024) throw Error('Empty release asset');
      if (readFileSync(file + '.sig', 'utf8').trim() !== entry.signature) throw Error('Signature mismatch');
      const sig = join(temp, 'updater.minisig');
      writeFileSync(sig, Buffer.from(entry.signature, 'base64'));
      execFileSync('minisign', ['-Vm', file, '-p', pub, '-x', sig], { stdio: 'inherit' });
    }
    if (readFileSync(join(root, `ddoktti-here_${version}_universal.dmg`)).length < 1024) throw Error('Missing DMG');
    writeFileSync(join(root, 'latest.json'), JSON.stringify({ version, pub_date: new Date().toISOString(), platforms }, null, 2) + '\n');
    for (const os of ['windows', 'macos']) rmSync(join(root, `${os}.json`));
  } finally { rmSync(temp, { recursive: true, force: true }); }
} else { throw Error('Use prepare windows|macos or merge all [directory]'); }
