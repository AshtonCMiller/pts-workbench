<script setup>
// A bar that grows to its value (a percentage) when it appears
import { ref, onMounted } from 'vue';

const props = defineProps({ value: { type: Number, required: true } });
const width = ref(0);
onMounted(() => requestAnimationFrame(() => requestAnimationFrame(() => (width.value = props.value))));
</script>

<template>
  <div class="bar"><i :style="{ width: `${width}%` }" /></div>
</template>

<style scoped>
.bar { height: 8px; border-radius: 99px; background: var(--soft); overflow: hidden; }
i { display: block; height: 100%; border-radius: inherit; background: linear-gradient(90deg, var(--accent), var(--accent-2)); transition: width 1.2s var(--ease); }
</style>
