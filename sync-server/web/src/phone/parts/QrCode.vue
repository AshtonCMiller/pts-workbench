<script setup>
// A QR code of `text`, drawn as SVG (each dark module a square), popping in a moment after it appears
import { computed } from 'vue';
import qrcode from 'qrcode-generator';

const props = defineProps({ text: { type: String, required: true }, size: { type: Number, default: 220 } });

const QUIET = 4; // the white border the code needs around it, in modules
const code = computed(() => {
  const qr = qrcode(0, 'M'); // 0: the smallest version that fits
  qr.addData(props.text);
  qr.make();
  const n = qr.getModuleCount();
  let d = '';
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) if (qr.isDark(r, c)) d += `M${c + QUIET} ${r + QUIET}h1v1h-1z`;
  }
  return { side: n + QUIET * 2, d };
});
</script>

<template>
  <svg class="qr" :width="size" :height="size" :viewBox="`0 0 ${code.side} ${code.side}`" shape-rendering="crispEdges" role="img" :aria-label="`QR code: ${text}`">
    <rect :width="code.side" :height="code.side" fill="#fff" />
    <path :d="code.d" fill="#0b1220" />
  </svg>
</template>

<style scoped>
.qr { display: block; border-radius: 16px; box-shadow: var(--shadow); animation: pop .7s var(--spring) both .15s; }
</style>
