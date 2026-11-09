// Bridge between the app's own UI and the main process. Only loaded into the app UI,
// never into Xtime or PTS pages.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('workbench', {
  send: (name, payload) => ipcRenderer.send('action', name, payload),
  onState: (fn) => ipcRenderer.on('state', (_e, state) => fn(state)),
  onNotice: (fn) => ipcRenderer.on('notice', (_e, notice) => fn(notice)),
  onLines: (fn) => ipcRenderer.on('ro-lines', (_e, data) => fn(data)),
  onChat: (fn) => ipcRenderer.on('chat', (_e, data) => fn(data)),
});