// "+ Manual" copies the current PTS tab: PTS keeps its state inside its frames, which can't be cloned, so the page each
// frame shows (and how far it's scrolled) is recorded and rebuilt in the new tab.
'use strict';
const { sleep } = require('./config');

// ---------- "+ Manual" copies the current PTS tab ----------
// PTS keeps its state inside its frames, which can't be cloned directly. Instead we record the
// page each frame is showing (and how far it's scrolled), then rebuild that in the new tab
// after its VIN has been entered.
async function snapshotFrames(wc) {
  const out = [];
  const walk = async (frame, p, depth) => {
    if (depth > 6) return;
    const kids = frame.frames || [];
    for (let i = 0; i < kids.length; i++) {
      const f = kids[i];
      const fp = [...p, i];
      let scroll = null;
      try { scroll = await f.executeJavaScript('[window.scrollX, window.scrollY]'); } catch { /* frame gone */ }
      out.push({ path: fp, name: f.name || '', url: f.url || '', scroll });
      await walk(f, fp, depth + 1);
    }
  };
  await walk(wc.mainFrame, [], 1);
  return out;
}

function frameAt(wc, entry) {
  let f = wc.mainFrame;
  for (const i of entry.path) f = f?.frames?.[i];
  if (f && (!entry.name || f.name === entry.name)) return f;
  // Layout differs slightly: fall back to a frame with the same name at the same depth
  if (entry.name) {
    const depth = (fr) => { let d = 0; while (fr.parent) { d++; fr = fr.parent; } return d; };
    return wc.mainFrame.framesInSubtree.find((x) => x.name === entry.name && depth(x) === entry.path.length) || null;
  }
  return null;
}

async function applySnapshot(tab, snap, token) {
  const wc = tab.view?.webContents;
  const alive = () => wc && !wc.isDestroyed() && tab.vinToken === token;
  await sleep(2500); // let PTS finish loading the vehicle first
  const maxDepth = Math.max(0, ...snap.map((e) => e.path.length));
  for (let d = 1; d <= maxDepth && alive(); d++) {
    let navigated = false;
    for (const e of snap.filter((x) => x.path.length === d)) {
      if (!/^https?:/i.test(e.url) || /VehicleMenu/i.test(e.url)) continue; // the vehicle comes from the VIN
      const f = frameAt(wc, e);
      if (!f || f.url === e.url) continue;
      try { await f.executeJavaScript(`location.replace(${JSON.stringify(e.url)})`); navigated = true; } catch { /* skip */ }
    }
    if (navigated) await sleep(2000); // child frames load after their parent
  }
  if (!alive()) return;
  for (const e of snap) {
    if (!e.scroll || (!e.scroll[0] && !e.scroll[1])) continue;
    const f = frameAt(wc, e);
    try { await f?.executeJavaScript(`window.scrollTo(${Number(e.scroll[0]) || 0}, ${Number(e.scroll[1]) || 0})`); } catch { /* skip */ }
  }
}

module.exports = { snapshotFrames, applySnapshot };
