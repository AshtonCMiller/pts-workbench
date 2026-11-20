<script setup>
// Opened on a computer: this page is for phones, so it shows a code to scan with the phone's camera (this page's
// address). "Use it here anyway" goes on as normal (for trying it out on a computer).
import { begin } from '../state.js';
import QrCode from '../parts/QrCode.vue';

const address = `${location.origin}${location.pathname}`;
const shown = `${location.host}${location.pathname}`.replace(/\/$/, '');
</script>

<template>
  <section class="view">
    <div class="card desk">
      <h2>Open this on your phone</h2>
      <p>This page is made for your phone. Point your phone's camera at the code, then tap the link that comes up.</p>
      <div class="code"><QrCode :text="address" /></div>
      <p class="muted">Or go to <b>{{ shown }}</b> on your phone.</p>
      <ol class="steps">
        <li><div>On an iPhone, add it to your Home Screen from Safari's <b>Share</b> menu, then open it from there.</div></li>
        <li><div>Pair it: in PTS Workbench on your computer, click <b>Pair phone</b> at the bottom right and scan that code.</div></li>
      </ol>
    </div>
    <button class="plain" @click="begin">Use it here anyway</button>
  </section>
</template>

<style scoped>
.desk { text-align: center; }
.code { display: grid; place-items: center; margin: 6px 0 16px; }
.steps { text-align: left; margin-top: 14px; }
b { color: var(--ink); }
</style>
