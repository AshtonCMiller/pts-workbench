// Bridge for the app's own workshop manual viewer (manual/viewer.html). Only loaded into that page.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('manual', {
  init: () => ipcRenderer.invoke('manual', 'init'),
  book: () => ipcRenderer.invoke('manual', 'book'),
  tree: () => ipcRenderer.invoke('manual', 'tree'),
  proc: (link) => ipcRenderer.invoke('manual', 'proc', link),
  oasis: () => ipcRenderer.invoke('manual', 'oasis'),
  cv: () => ipcRenderer.invoke('manual', 'cv'),
  copy: (text) => ipcRenderer.invoke('manual', 'copy', text),
  spa: (q) => ipcRenderer.invoke('manual', 'spa', q),
  report: (ev) => ipcRenderer.invoke('manual', 'report', ev),
  signIn: () => ipcRenderer.invoke('manual', 'signin'),
  openPts: () => ipcRenderer.invoke('manual', 'open-pts'),
  openUrl: (url) => ipcRenderer.invoke('manual', 'open-url', url),
  openLog: () => ipcRenderer.invoke('manual', 'open-log'),
  log: (text) => ipcRenderer.invoke('manual', 'log', text),
  save: (state) => ipcRenderer.send('manual-save', state),
});