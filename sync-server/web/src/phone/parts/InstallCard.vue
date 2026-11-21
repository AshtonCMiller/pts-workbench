<script setup>
// On Android, in the browser: install Workbench, so it opens from the Home screen like an app (Chrome's own install
// prompt when it's offered; otherwise where to find it in the browser's menu)
import { install, android, standalone, installApp } from '../state.js';
</script>

<template>
  <div v-if="android && !standalone && !install.done" class="card install">
    <div class="row">
      <img src="../assets/icon.png" alt="">
      <div>
        <h2>Install Workbench</h2>
        <p class="muted">It opens from your Home screen like an app, and notifications come to it.</p>
      </div>
    </div>
    <button v-if="install.offer" @click="installApp">Install</button>
    <p v-else class="hint">In Chrome, tap the <b>⋮</b> menu at the top right, then <b>Install app</b> (or <b>Add to Home screen</b>).</p>
  </div>
</template>

<style scoped>
.install .row { justify-content: flex-start; gap: 14px; margin-bottom: 12px; }
.install img { width: 48px; height: 48px; border-radius: 12px; flex: none; }
.install h2 { margin: 0 0 2px; }
.install p { margin: 0; }
.hint { font-size: 14px; color: var(--muted); }
.hint b { color: var(--ink); }
</style>
