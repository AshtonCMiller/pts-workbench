<script setup>
// The camera, looking for the pairing code (jsQR, from jsDelivr) until it's found or you cancel
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { show, toast, paired } from '../state.js';
import { readPairCode } from '../lib/keys.js';
import { loadScript } from '../lib/ocr.js';

const JSQR = 'https://cdn.jsdelivr.net/npm/jsqr@1.4.0/dist/jsQR.js';
const video = ref(null);
const found = ref(false);
const message = ref('Point the camera at the code on your computer.');
let stream = null;
let stopped = false;

function stop() {
  stopped = true;
  stream?.getTracks().forEach((t) => t.stop());
  stream = null;
  if (video.value) video.value.srcObject = null;
}

onMounted(async () => {
  try {
    if (!window.jsQR) await loadScript(JSQR, 'The code reader');
    stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' }, audio: false });
    if (stopped) return stop();
    video.value.srcObject = stream;
    await video.value.play();
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    const look = () => {
      if (stopped) return;
      const v = video.value;
      if (v && v.readyState >= 2 && v.videoWidth) {
        const scale = Math.min(1, 720 / v.videoWidth);
        canvas.width = Math.round(v.videoWidth * scale);
        canvas.height = Math.round(v.videoHeight * scale);
        ctx.drawImage(v, 0, 0, canvas.width, canvas.height);
        const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const code = window.jsQR(img.data, img.width, img.height, { inversionAttempts: 'attemptBoth' });
        const pair = code && readPairCode(code.data);
        if (pair) {
          found.value = true;
          if (navigator.vibrate) navigator.vibrate(40);
          stop();
          setTimeout(() => paired(pair), 450);
          return;
        }
        if (code) message.value = 'That code isn’t from PTS Workbench. Use the one under Pair phone.';
      }
      requestAnimationFrame(look);
    };
    requestAnimationFrame(look);
  } catch (e) {
    stop();
    show('pair');
    toast(e && e.name === 'NotAllowedError'
      ? 'The camera wasn’t allowed. You can type it in instead.'
      : `Couldn't start the camera (${e.message || e}).`, true);
  }
});
onBeforeUnmount(stop);
</script>

<template>
  <section class="view">
    <div class="card">
      <div class="camera" :class="{ found }"><video ref="video" playsinline muted /><div class="frame" /></div>
      <p>{{ message }}</p>
      <button class="ghost" @click="show('pair')">Cancel</button>
    </div>
  </section>
</template>
