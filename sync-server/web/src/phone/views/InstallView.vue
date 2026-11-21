<script setup>
// On a phone, in the browser: Workbench only works once it's installed, so this is all it shows until then
import { ios, install, installApp } from '../state.js';
</script>

<template>
  <section class="view">
    <!-- iPhone, in Safari: it needs to be on the Home Screen first -->
    <div v-if="ios" class="card">
      <h2>Add it to your Home Screen</h2>
      <p>On an iPhone, Workbench works once it's on your Home Screen.</p>
      <ol class="steps">
        <li>
          <div>Tap the <b>Share</b> button
            <svg class="share" viewBox="0 0 16 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M8 12V1.5M4.5 5 8 1.5 11.5 5" /><path d="M5 8H2.5v10.5h11V8H11" /></svg>
            at the bottom of Safari (in the <b>…</b> menu on newer iPhones).</div>
        </li>
        <li><div>Pick <b>Add to Home Screen</b>, then <b>Add</b>.</div></li>
        <li><div>Open <b>Workbench</b> from your Home Screen, and scan the code on your computer again.</div></li>
      </ol>
    </div>

    <!-- Android, in Chrome: installed first (Chrome's own install prompt when it offers one) -->
    <div v-else class="card">
      <div class="head">
        <img src="../assets/icon.png" alt="">
        <div>
          <h2>Install Workbench</h2>
          <p class="muted">It works once it's installed: it opens from your Home screen like an app, and notifications come to it.</p>
        </div>
      </div>
      <Transition name="fade" mode="out-in">
        <p v-if="install.done" key="done" class="done">
          Installed. Open <b>Workbench</b> from your Home screen to carry on.
        </p>
        <button v-else-if="install.offer" key="offer" @click="installApp">Install</button>
        <ol v-else key="steps" class="steps">
          <li><div>Tap the <b>⋮</b> menu at the top right of Chrome.</div></li>
          <li><div>Pick <b>Install app</b> (or <b>Add to Home screen</b>), then <b>Install</b>.</div></li>
          <li><div>Open <b>Workbench</b> from your Home screen.</div></li>
        </ol>
      </Transition>
      <p v-if="!install.done" class="muted small">Already installed? Open it from your Home screen, or from Chrome's menu (<b>Open app</b>).</p>
    </div>
  </section>
</template>

<style scoped>
.head { display: flex; gap: 14px; align-items: center; margin-bottom: 14px; }
.head img { width: 52px; height: 52px; border-radius: 13px; flex: none; }
.head h2 { margin: 0 0 2px; }
.head p { margin: 0; }
.done { font-size: 16px; }
.small { font-size: 13px; margin: 12px 0 0; }
b { color: var(--ink); }
</style>
