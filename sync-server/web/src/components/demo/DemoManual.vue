<script setup>
import { demo } from '../../content.js';
</script>

<template>
  <div>
    <h4>{{ demo.manual.title }}</h4>
    <div class="crumb">{{ demo.manual.crumb }}</div>
    <div v-for="(s, i) in demo.manual.steps" :key="i" class="step"><em>{{ i + 1 }}</em><span>{{ s }}</span></div>
    <div class="line" style="width: 92%" />
    <div class="line" style="width: 76%" />
    <div class="line" style="width: 84%" />
  </div>
</template>

<style scoped>
h4 { margin: 2px 0 4px; font-size: 15px; }
.crumb { font-size: 12px; color: var(--muted); margin-bottom: 12px; }
.step { display: flex; gap: 10px; margin-bottom: 10px; font-size: 13.5px; }
.step em {
  flex: none; width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center;
  font-style: normal; font-size: 12px; font-weight: 700; background: var(--soft); color: var(--accent);
}
.line { height: 9px; border-radius: 6px; background: var(--soft); margin: 7px 0; }
</style>
