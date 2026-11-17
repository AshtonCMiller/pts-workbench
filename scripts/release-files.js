// Makes a release's update files, after the GitHub build has built the app and its installer (no packages needed):
//
//   dist/workbench.json   the new app's own updates (crates/app/src/updates.rs): the version, the launcher level it
//                         needs, each program file's size and SHA-256, and the installer
//   dist/app-bundle.asar  for computers still on the Electron app: the move to the new app (handoff/), as the Electron
//   dist/app-bundle.json  app's "app files" update (its boot.js starts it; see handoff/src/main.js)
//   dist/latest.yml       what the Electron app's updater reads first (beta.yml with Beta updates on): this version,
//   dist/beta.yml         and this release's installer (used if the app files can't be)
//
// Run as:  VERSION=0.5.0-beta.3 TAG=v0.5.0-beta.3 REPO=AshtonCMiller/pts-workbench node scripts/release-files.js
// with the program files in dist/app/ and the installer in dist/.
'use strict';
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const root = path.join(__dirname, '..');
const dist = path.join(root, 'dist');
const { VERSION, TAG, REPO } = process.env;
if (!VERSION || !TAG || !REPO) { console.error('VERSION, TAG and REPO are needed'); process.exit(1); }

const PROGRAM = ['pts-workbench.exe', 'pts-workbench-app.exe', 'pts-signin.exe', 'pdfium.dll', 'pts-vci32.exe'];
const SETUP = `PTS-Workbench-Setup-${VERSION}.exe`;

const sha = (alg, buf, enc) => crypto.createHash(alg).update(buf).digest(enc);
const info = (file, name) => {
  const buf = fs.readFileSync(file);
  return { name, size: buf.length, sha256: sha('sha256', buf, 'hex') };
};

// The launcher level this build's launcher has (an update needing a newer one comes with the installer)
const launcherSrc = fs.readFileSync(path.join(root, 'crates', 'launcher', 'src', 'lib.rs'), 'utf8');
const level = Number((launcherSrc.match(/pub const LAUNCHER_LEVEL: u32 = (\d+);/) || [])[1]);
if (!level) { console.error("Couldn't find LAUNCHER_LEVEL in crates/launcher/src/lib.rs"); process.exit(1); }

// ---------- workbench.json ----------
const files = PROGRAM.map((n) => {
  const f = path.join(dist, 'app', n);
  if (!fs.existsSync(f)) { console.error(`Missing ${f}`); process.exit(1); }
  return info(f, n);
});
const setupFile = path.join(dist, SETUP);
if (!fs.existsSync(setupFile)) { console.error(`Missing ${setupFile}`); process.exit(1); }
const setup = info(setupFile, SETUP);
// (with its "What's new", shown while it waits for a restart)
const notes = fs.readFileSync(path.join(root, 'crates', 'app', 'whats-new.md'), 'utf8');
fs.writeFileSync(path.join(dist, 'workbench.json'), JSON.stringify({ version: VERSION, launcher: level, files, setup, notes }, null, 2));

// ---------- the move, as the Electron app's "app files" ----------
// .asar is Electron's archive: an 8-byte size, a JSON index of the files (with their offsets), then the files
function packAsar(entries) {
  const index = { files: {} };
  let offset = 0;
  const blobs = [];
  for (const [p, data] of entries) {
    const parts = p.split('/');
    let node = index;
    for (const d of parts.slice(0, -1)) node = (node.files[d] = node.files[d] || { files: {} });
    const blockSize = 4 * 1024 * 1024;
    const blocks = [];
    for (let i = 0; i < data.length; i += blockSize) blocks.push(sha('sha256', data.subarray(i, i + blockSize), 'hex'));
    node.files[parts[parts.length - 1]] = {
      size: data.length, offset: String(offset),
      integrity: { algorithm: 'SHA256', hash: sha('sha256', data, 'hex'), blockSize, blocks },
    };
    offset += data.length;
    blobs.push(data);
  }
  const json = Buffer.from(JSON.stringify(index), 'utf8');
  const padded = json.length + ((4 - (json.length % 4)) % 4);
  const header = Buffer.alloc(8 + padded);
  header.writeUInt32LE(4 + padded, 0);
  header.writeUInt32LE(json.length, 4);
  json.copy(header, 8);
  const size = Buffer.alloc(8);
  size.writeUInt32LE(4, 0);
  size.writeUInt32LE(header.length, 4);
  return Buffer.concat([size, header, ...blobs]);
}

const handoffDir = path.join(root, 'handoff');
const pkg = JSON.parse(fs.readFileSync(path.join(handoffDir, 'package.json'), 'utf8'));
pkg.version = VERSION;
const entries = [
  ['handoff.json', Buffer.from(JSON.stringify({ version: VERSION, tag: TAG, repo: REPO, setup: { name: setup.name, size: setup.size, sha256: setup.sha256 } }, null, 2))],
  ['package.json', Buffer.from(JSON.stringify(pkg, null, 2))],
  ['src/main.js', fs.readFileSync(path.join(handoffDir, 'src', 'main.js'))],
  ['src/whats-new.md', fs.readFileSync(path.join(handoffDir, 'src', 'whats-new.md'))],
];
const asar = packAsar(entries);
fs.writeFileSync(path.join(dist, 'app-bundle.asar'), asar);
// (as the Electron app's own scripts/make-bundle.js writes it: a checksum per file, package.json without its version)
const perFile = {};
for (const [p, data] of entries) {
  let d = data;
  if (p === 'package.json') { const j = JSON.parse(d.toString('utf8')); delete j.version; delete j.build; d = Buffer.from(JSON.stringify(j)); }
  perFile[p] = sha('sha1', d, 'base64');
}
fs.writeFileSync(path.join(dist, 'app-bundle.json'), JSON.stringify({
  version: VERSION, shell: Number(pkg.ptsWorkbench.shell), electron: '', file: 'app-bundle.asar',
  size: asar.length, sha512: sha('sha512', asar, 'base64'), files: perFile,
}));

// ---------- latest.yml / beta.yml (electron-updater's format) ----------
const setupBuf = fs.readFileSync(setupFile);
const sha512 = sha('sha512', setupBuf, 'base64');
const yml = [
  `version: ${VERSION}`,
  'files:',
  `  - url: ${SETUP}`,
  `    sha512: ${sha512}`,
  `    size: ${setupBuf.length}`,
  `path: ${SETUP}`,
  `sha512: ${sha512}`,
  `releaseDate: '${new Date().toISOString()}'`,
  '',
].join('\n');
fs.writeFileSync(path.join(dist, 'latest.yml'), yml);
fs.writeFileSync(path.join(dist, 'beta.yml'), yml);

console.log(`v${VERSION} (launcher level ${level}): ${files.map((f) => `${f.name} ${(f.size / 1048576).toFixed(1)} MB`).join(', ')}; installer ${(setup.size / 1048576).toFixed(1)} MB; move bundle ${asar.length} bytes`);
