<script setup>
// The phone page's icons, drawn here
import { computed } from 'vue';

const props = defineProps({ name: { type: String, required: true }, stroke: { type: Number, default: 2 } });
const ICONS = {
  ro: [['rect', { x: 5, y: 3, width: 14, height: 18, rx: 2 }], ['path', { d: 'M9 8h6M9 12h6M9 16h4' }]],
  clock: [['circle', { cx: 12, cy: 12, r: 9 }], ['path', { d: 'M12 7v5l3 2' }]],
  phone: [['rect', { x: 7, y: 2.5, width: 10, height: 19, rx: 2.5 }], ['path', { d: 'M11 18h2' }]],
  refresh: [['path', { d: 'M20 12a8 8 0 1 1-2.34-5.66' }], ['path', { d: 'M20 4v5h-5' }]],
  tick: [['path', { d: 'M5 12.5l4.5 4.5L19 7.5' }]],
};
const shapes = computed(() => ICONS[props.name] || []);
</script>

<template>
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" :stroke-width="stroke" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <component :is="tag" v-for="([tag, attrs], i) in shapes" :key="i" v-bind="attrs" />
  </svg>
</template>
