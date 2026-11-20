<script setup>
// Hours: the ring fills and the total counts up when the tab opens
import { ref, onMounted, onBeforeUnmount } from 'vue';
import Bar from './Bar.vue';
import { demo } from '../../content.js';

const RING = 2 * Math.PI * 60; // the circle's length (r = 60)
const { flagged, ringMax, days } = demo.hours;
const dayMax = Math.max(...days.map((d) => d[1])) * 1.15;

const shown = ref(0);
const offset = ref(RING);
let frame = 0;

onMounted(() => {
  requestAnimationFrame(() => requestAnimationFrame(() => (offset.value = RING * (1 - flagged / ringMax))));
  const t0 = performance.now();
  const count = (t) => {
    const p = Math.min(1, (t - t0) / 1400);
    shown.value = flagged * (1 - (1 - p) ** 3);
    if (p < 1) frame = requestAnimationFrame(count);
  };
  frame = requestAnimationFrame(count);
});
onBeforeUnmount(() => cancelAnimationFrame(frame));
</script>

<template>
  <div class="hours">
    <div class="ring">
      <svg width="150" height="150" viewBox="0 0 150 150">
        <circle class="track" cx="75" cy="75" r="60" />
        <circle class="fill" cx="75" cy="75" r="60" :stroke-dasharray="RING" :stroke-dashoffset="offset" />
      </svg>
      <div class="val"><b>{{ shown.toFixed(1) }}</b><small>flagged this period</small></div>
    </div>
    <div class="days">
      <div v-for="[day, hrs] in days" :key="day">{{ day }}<Bar :value="(hrs / dayMax) * 100" /><b>{{ hrs.toFixed(1) }}</b></div>
    </div>
  </div>
</template>

<style scoped>
.hours { display: grid; grid-template-columns: 150px 1fr; gap: 18px; align-items: center; height: 100%; }
@media (max-width: 520px) { .hours { grid-template-columns: 1fr; justify-items: center; height: auto; } }
.ring { position: relative; width: 150px; height: 150px; }
svg { transform: rotate(-90deg); }
circle { fill: none; stroke-width: 12; }
.track { stroke: var(--soft); }
.fill { stroke: var(--accent); stroke-linecap: round; transition: stroke-dashoffset 1.4s var(--ease); }
.val { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; }
.val b { font-size: 28px; font-variant-numeric: tabular-nums; line-height: 1; }
.val small { color: var(--muted); font-size: 12px; }
.days { display: grid; gap: 7px; width: 100%; }
.days > div { display: grid; grid-template-columns: 34px 1fr 38px; gap: 8px; align-items: center; font-size: 12.5px; color: var(--muted); }
.days b { color: var(--ink); text-align: right; font-variant-numeric: tabular-nums; }
</style>
