import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const app = JSON.parse(readFileSync(join(root, 'apps/calmmd-gpui/app.json'), 'utf8'));
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const os = process.platform;
const dist = join(root, 'dist');
const name = `calmmd-${version}-${os}`;
const stage = join(dist, name);

function run(command, args, options = {}) {
  execFileSync(command, args, { stdio: 'inherit', cwd: root, ...options });
}

function copyCommon() {
  mkdirSync(stage, { recursive: true });
  const binary = join(root, 'target/release', os === 'win32' ? `${app.binary}.exe` : app.binary);
  if (!existsSync(binary)) throw new Error(`Missing ${binary}; run cargo build --release -p calmmd-gpui`);
  copyFileSync(binary, join(stage, basename(binary)));
  copyFileSync(join(root, 'apps/calmmd-gpui/resources/manifest.json'), join(stage, 'manifest.json'));
  copyFileSync(join(root, 'README.md'), join(stage, 'README.md'));
  copyFileSync(join(root, 'LICENSE'), join(stage, 'LICENSE'));
  copyFileSync(join(root, 'THIRD_PARTY_LICENSES.md'), join(stage, 'THIRD_PARTY_LICENSES.md'));
  copyFileSync(join(root, 'docs/PLATFORM-LIMITS.md'), join(stage, 'PLATFORM-LIMITS.md'));
  writeFileSync(join(stage, 'VERSION.txt'), `${version}\n`);
}

function basename(path) {
  return path.split(/[\\/]/).pop();
}

function packageWindows() {
  copyFileSync(join(root, 'packaging/windows/install.ps1'), join(stage, 'install.ps1'));
  const zip = join(dist, `${name}.zip`);
  rmSync(zip, { force: true });
  run('powershell', [
    '-NoProfile',
    '-Command',
    `Compress-Archive -Path '${stage}\\*' -DestinationPath '${zip}'`,
  ]);
  console.log(`PACKAGE=${zip}`);
}

function packageMac() {
  const appDir = join(dist, `${app.name}.app`);
  rmSync(appDir, { recursive: true, force: true });
  const contents = join(appDir, 'Contents');
  mkdirSync(join(contents, 'MacOS'), { recursive: true });
  mkdirSync(join(contents, 'Resources'), { recursive: true });
  copyFileSync(join(stage, app.binary), join(contents, 'MacOS', app.binary));
  copyFileSync(join(root, 'apps/calmmd-gpui/resources/icons/icon.icns'), join(contents, 'Resources', 'icon.icns'));
  const plist = readFileSync(join(root, 'packaging/macos/Info.plist'), 'utf8').replaceAll('__VERSION__', version);
  writeFileSync(join(contents, 'Info.plist'), plist);
  const tar = join(dist, `${name}.app.tar.gz`);
  rmSync(tar, { force: true });
  run('tar', ['-czf', tar, '-C', dist, `${app.name}.app`]);
  console.log(`PACKAGE=${tar}`);
  if (which('hdiutil')) {
    const dmg = join(dist, `${name}.dmg`);
    rmSync(dmg, { force: true });
    run('hdiutil', ['create', '-volname', app.name, '-srcfolder', appDir, '-format', 'UDRW', dmg]);
    console.log(`PACKAGE=${dmg}`);
  } else {
    console.log('SKIP=dmg (hdiutil unavailable)');
  }
}

function packageLinux() {
  const tar = `${stage}.tar.gz`;
  rmSync(tar, { force: true });
  run('tar', ['-czf', tar, '-C', dist, name]);
  console.log(`PACKAGE=${tar}`);
  if (!which('dpkg-deb')) {
    console.log('SKIP=deb (dpkg-deb unavailable)');
    return;
  }
  const deb = join(dist, `${name}.deb`);
  const debRoot = join(dist, `${name}-deb`);
  rmSync(deb, { force: true });
  rmSync(debRoot, { recursive: true, force: true });
  mkdirSync(join(debRoot, 'DEBIAN'), { recursive: true });
  mkdirSync(join(debRoot, 'usr/bin'), { recursive: true });
  mkdirSync(join(debRoot, 'usr/share/applications'), { recursive: true });
  mkdirSync(join(debRoot, 'usr/share/icons/hicolor/256x256/apps'), { recursive: true });
  copyFileSync(join(stage, app.binary), join(debRoot, 'usr/bin', app.binary));
  copyFileSync(
    join(root, 'packaging/linux/calmmd.desktop'),
    join(debRoot, 'usr/share/applications/calmmd.desktop'),
  );
  copyFileSync(
    join(root, 'apps/calmmd-gpui/resources/icons/128x128@2x.png'),
    join(debRoot, 'usr/share/icons/hicolor/256x256/apps/calmmd.png'),
  );
  writeFileSync(
    join(debRoot, 'DEBIAN/control'),
    [
      'Package: calmmd',
      `Version: ${version}`,
      'Section: editors',
      'Priority: optional',
      'Architecture: amd64',
      'Depends: libc6, libssl3',
      `Maintainer: ${app.publisher}`,
      `Description: ${app.shortDescription}`,
      ' CalmMD keeps notes in plain Markdown files and exports PDFs locally.',
      '',
    ].join('\n'),
  );
  run('dpkg-deb', ['--build', debRoot, deb]);
  console.log(`PACKAGE=${deb}`);
}

function which(command) {
  try {
    execFileSync(os === 'win32' ? 'where' : 'which', [command], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

rmSync(stage, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
if (!process.argv.includes('--skip-build')) {
  run('cargo', ['build', '--release', '-p', 'calmmd-gpui']);
}
copyCommon();
if (os === 'win32') packageWindows();
else if (os === 'darwin') packageMac();
else packageLinux();
