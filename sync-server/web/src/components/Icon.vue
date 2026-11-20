<script setup>
// Line icons, drawn here (no icon library): <Icon name="bell" :size="22" />
import { computed } from 'vue';

const props = defineProps({
  name: { type: String, required: true },
  size: { type: Number, default: 20 },
  stroke: { type: Number, default: 2 },
});

const ICONS = {
  download: [['path', { d: 'M12 3v12m0 0-5-5m5 5 5-5M4 19h16' }]],
  check: [['path', { d: 'M5 12.5 10 17 19 7' }]],
  wrench: [['path', { d: 'M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.6 2.6-2.4-.6-.6-2.4z' }]],
  ro: [['rect', { x: 5, y: 3, width: 14, height: 18, rx: 2 }], ['path', { d: 'M9 8h6M9 12h6M9 16h4' }]],
  pulse: [['path', { d: 'M3 12h4l3-8 4 16 3-8h4' }]],
  clock: [['circle', { cx: 12, cy: 12, r: 9 }], ['path', { d: 'M12 7v5l3 2' }]],
  bell: [['path', { d: 'M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9' }], ['path', { d: 'M10.3 21a1.94 1.94 0 0 0 3.4 0' }]],
  lock: [['rect', { x: 4, y: 11, width: 16, height: 10, rx: 2 }], ['path', { d: 'M8 11V7a4 4 0 0 1 8 0v4' }]],
  refresh: [['path', { d: 'M21 12a9 9 0 1 1-3-6.7L21 8' }], ['path', { d: 'M21 3v5h-5' }]],
};

const shapes = computed(() => ICONS[props.name] || []);
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 24 24" fill="none" stroke="currentColor" :stroke-width="stroke"
       stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <component :is="tag" v-for="([tag, attrs], i) in shapes" :key="i" v-bind="attrs" />
  </svg>
</template>
