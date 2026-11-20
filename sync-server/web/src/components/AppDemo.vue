<script setup>
// The app, drawn: its tabs take turns (every few seconds, or when clicked), the highlight sliding to the tab and the
// page sliding in. Made-up repair orders (content.js demo).
import { ref, reactive, onMounted, onBeforeUnmount, nextTick } from 'vue';
import DemoRos from './demo/DemoRos.vue';
import DemoManual from './demo/DemoManual.vue';
import DemoDiag from './demo/DemoDiag.vue';
import DemoHours from './demo/DemoHours.vue';
import { brand } from '../content.js';
import { reducedMotion } from '../motion.js';

const tabs = [
  { name: 'My ROs', pane: DemoRos },
  { name: 'Manual', pane: DemoManual },
  { name: 'Diagnostics', pane: DemoDiag },
  { name: 'Hours', pane: DemoHours },
];
const EVERY = 3800;

const at = ref(0);
const buttons = [];
const glow = reactive({ left: '0px', width: '0px' });
let timer = 0;

function place() {
  const b = buttons[at.value];
  if (!b) return;
  glow.left = `${b.offsetLeft}px`;
  glow.width = `${b.offsetWidth}px`;
}
function show(i) {
  if (i === at.value) return;
  at.value = i;
  nextTick(place);
}
function cycle() {
  clearInterval(timer);
  if (!reducedMotion()) timer = setInterval(() => show((at.value + 1) % tabs.length), EVERY);
}
function pick(i) {
  show(i);
  cycle();
}

onMounted(() => {
  place();
  window.addEventListener('resize', place);
  document.fonts?.ready.then(place);
  cycle();
});
onBeforeUnmount(() => {
  clearInterval(timer);
  window.removeEventListener('resize', place);
});
</script>

<template>
  <div class="window">
    <div class="chrome"><b /><b /><b /><span class="title">{{ brand.app }}</span></div>
    <div class="tabs">
      <button
        v-for="(t, i) in tabs"
        :key="t.name"
        :ref="(el) => (buttons[i] = el)"
        :class="{ on: i === at }"
        type="button"
        tabindex="-1"
        @click="pick(i)"
      >{{ t.name }}</button>
      <span class="glow" :style="glow" />
    </div>
    <div class="panes">
      <Transition name="pane">
        <component :is="tabs[at].pane" :key="at" class="pane" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.window {
  background: var(--card); border-radius: 18px; box-shadow: var(--shadow), inset 0 0 0 1px var(--line);
  overflow: hidden; animation: float 7s ease-in-out infinite;
}
@keyframes float { 0%, 100% { transform: translateY(0) rotate(-.4deg); } 50% { transform: translateY(-10px) rotate(.2deg); } }
.chrome { display: flex; align-items: center; gap: 7px; padding: 12px 14px; border-bottom: 1px solid var(--line); background: var(--soft); }
.chrome b { width: 11px; height: 11px; border-radius: 50%; background: var(--line); }
.title { margin-left: 10px; font-size: 13px; font-weight: 650; color: var(--muted); }
.tabs { position: relative; display: flex; gap: 4px; padding: 10px 12px 0; border-bottom: 1px solid var(--line); }
.tabs button {
  position: relative; z-index: 1; border: 0; background: none; font: inherit; font-size: 13.5px; font-weight: 600;
  color: var(--muted); padding: 8px 12px 10px; cursor: pointer; transition: color .25s;
}
.tabs button.on { color: var(--accent); }
.glow {
  position: absolute; bottom: -1px; height: 3px; border-radius: 3px 3px 0 0; background: var(--accent);
  transition: left .55s var(--spring), width .55s var(--spring);
}
.panes { position: relative; height: 318px; }
.pane { position: absolute; inset: 0; padding: 16px; }
.pane-enter-active, .pane-leave-active { transition: opacity .35s var(--ease), transform .55s var(--spring); }
.pane-enter-from { opacity: 0; transform: translateX(24px) scale(.98); }
.pane-leave-to { opacity: 0; transform: translateX(-24px) scale(.98); }
@media (max-width: 520px) {
  .panes { height: 360px; }
  .tabs button { padding: 8px 8px 10px; font-size: 12.5px; }
}
</style>
