<script setup>
import Bar from './Bar.vue';
import { demo } from '../../content.js';
</script>

<template>
  <div>
    <div v-for="d in demo.dtcs" :key="d.code" class="dtc">
      <code>{{ d.code }}</code><span class="what">{{ d.text }}</span><span class="tag" :class="d.tag[0]">{{ d.tag[1] }}</span>
    </div>
    <div class="mini">
      <div v-for="[label, value] in demo.readings" :key="label"><small>{{ label }}</small><b>{{ value }}</b></div>
    </div>
    <div class="label">Health reports, last 30 days</div>
    <Bar :value="72" />
  </div>
</template>

<style scoped>
.dtc { display: flex; align-items: center; gap: 10px; padding: 9px 12px; border-radius: 12px; background: var(--soft); margin-bottom: 8px; font-size: 13.5px; }
code { font: 700 13px ui-monospace, SFMono-Regular, Consolas, monospace; color: var(--bad); }
.what { flex: 1; color: var(--muted); }
.mini { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-top: 12px; }
.mini div { background: var(--soft); border-radius: 12px; padding: 10px; }
.mini small { display: block; font-size: 11.5px; color: var(--muted); }
.mini b { font-size: 17px; font-variant-numeric: tabular-nums; }
.label { margin: 14px 0 6px; font-size: 12.5px; color: var(--muted); }
</style>
