<script setup>
// My ROs: a new one is assigned a moment after the tab opens
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { demo } from '../../content.js';

const arrived = ref(false);
let t = 0;
onMounted(() => (t = setTimeout(() => (arrived.value = true), 900)));
onBeforeUnmount(() => clearTimeout(t));
</script>

<template>
  <div>
    <div v-for="r in demo.ros" :key="r.num" class="ro">
      <span class="num">{{ r.num }}</span>
      <span class="veh"><b>{{ r.vehicle }}</b><small>{{ r.detail }}</small></span>
      <span class="tag" :class="r.tag[0]">{{ r.tag[1] }}</span>
    </div>
    <Transition name="pop">
      <div v-if="arrived" class="ro">
        <span class="num">{{ demo.newRo.num }}</span>
        <span class="veh"><b>{{ demo.newRo.vehicle }}</b><small>{{ demo.newRo.detail }}</small></span>
        <span class="tag" :class="demo.newRo.tag[0]">{{ demo.newRo.tag[1] }}</span>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.ro { display: flex; align-items: center; gap: 12px; padding: 11px 12px; border-radius: 12px; background: var(--soft); margin-bottom: 8px; }
.num { font-weight: 750; font-variant-numeric: tabular-nums; min-width: 62px; }
.veh { flex: 1; min-width: 0; }
.veh b { display: block; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.veh small { color: var(--muted); font-size: 12.5px; }
.pop-enter-active { transition: opacity .4s var(--ease), transform .7s var(--spring); }
.pop-enter-from { opacity: 0; transform: translateY(10px) scale(.96); }
</style>
