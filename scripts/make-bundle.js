// Makes the "app files" update for a release (used by the GitHub build after electron-builder):
//   dist/app-bundle.asar  — the app's own files, exactly as the installer puts them (resources/app.asar)
//   dist/app-bundle.json  — its version, the Electron "shell" it needs, size, sha512, and a checksum per file
// Installed apps whose shell matches download just these (no installer), and when the only changed files are
// pages / Xtime scripts they switch without restarting (see "quick updates" in src/main.js).
'use strict';
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

// Reads an .asar archive (Electron's format: a small JSON index, then the files' bytes) — no extra packages needed
function readAsar(file) {
  const buf = fs.readFileSync(file);
  const headerSize = buf.readUInt32LE(4);
  const jsonLen = buf.readUInt32LE(12);
  const header = JSON.parse(buf.toString('utf8', 16, 16 + jsonLen));
  const base = 8 + headerSize;
  const out = [];
  const walk = (node, prefix) => {
    for (const [name, e] of Object.entries(node.files || {})) {
      const p = prefix ? `${prefix}/${name}` : name;
      if (e.files) walk(e, p);
      else if (e.link) continue;
      else if (e.unpacked) out.push([p, fs.readFileSync(path.join(`${file}.unpacked`, p))]);
      else out.push([p, buf.subarray(base + Number(e.offset), base + Number(e.offset) + Number(e.size))]);
    }
  };
  walk(header, '');
  return out;
}

const root = path.join(__dirname, '..');
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const src = path.join(root, 'dist', 'win-unpacked', 'resources', 'app.asar');
if (!fs.existsSync(src)) { console.error(`No ${src} — run electron-builder first`); process.exit(1); }

const files = {};
for (const [p, raw] of readAsar(src)) {
  let data = raw;
  // package.json changes its version every release; only the rest of it matters for "did anything change"
  if (p === 'package.json') { const j = JSON.parse(data.toString('utf8')); delete j.version; data = Buffer.from(JSON.stringify(j)); }
  files[p] = crypto.createHash('sha1').update(data).digest('base64');
}
if (fs.existsSync(`${src}.unpacked`)) console.warn('Note: app.asar.unpacked exists — those files are listed but not part of app-bundle.asar');
const out = path.join(root, 'dist', 'app-bundle.asar');
fs.copyFileSync(src, out);
const buf = fs.readFileSync(out);
const manifest = {
  version: pkg.version,
  shell: Number(pkg.ptsWorkbench?.shell) || 1,
  electron: (pkg.devDependencies?.electron || '').replace(/^[^\d]*/, ''),
  file: 'app-bundle.asar',
  size: buf.length,
  sha512: crypto.createHash('sha512').update(buf).digest('base64'),
  files,
};
fs.writeFileSync(path.join(root, 'dist', 'app-bundle.json'), JSON.stringify(manifest));
console.log(`app-bundle v${manifest.version} (shell ${manifest.shell}): ${Object.keys(files).length} files, ${(buf.length / 1048576).toFixed(1)} MB`);
