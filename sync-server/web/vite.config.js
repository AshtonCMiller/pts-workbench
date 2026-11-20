// The prodealer.works website. `npm run dev` for a live preview, `npm run build` to build it into ../site/ (what
// Cloudflare serves), next to the phone page (../site/phone/, which isn't part of this and is left alone).
import { rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

const out = fileURLToPath(new URL('../site', import.meta.url));
// what's the server's, not this site's (the old phone page and its files, the API, the releases): the dev preview
// gets them from prodealer.works
const live = { target: 'https://prodealer.works', changeOrigin: true };

export default defineConfig({
  plugins: [
    vue(),
    // last build's files go first (the folder itself isn't emptied: the phone page lives there too)
    {
      name: 'clear-old-assets',
      apply: 'build',
      buildStart() {
        rmSync(`${out}/assets`, { recursive: true, force: true });
      },
    },
  ],
  build: {
    outDir: out,
    emptyOutDir: false,
    assetsDir: 'assets',
    // the pages: the home page, and /download/ (it starts the download and says what to do next)
    rollupOptions: {
      input: {
        home: fileURLToPath(new URL('./index.html', import.meta.url)),
        download: fileURLToPath(new URL('./download/index.html', import.meta.url)),
        // the phone page as a Vue app, being tried out at /phone/next/ (phone/next/index.html says how to switch over)
        phone: fileURLToPath(new URL('./phone/index.html', import.meta.url)),
      },
    },
  },
  server: {
    proxy: { '/phone/sw.js': live, '/v1': live, '/download/latest': live, '/updates': live },
  },
});
