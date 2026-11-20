<script setup>
import { problem } from '../content.js';
</script>

<template>
  <section>
    <div class="wrap center">
      <div v-reveal class="kicker">The problem</div>
      <h2 v-reveal>{{ problem.title }}</h2>
      <p v-reveal class="sub">{{ problem.text }}</p>
      <div v-reveal class="juggle">
        <span v-for="p in problem.places" :key="p">{{ p }}</span>
        <span class="arrow">→</span>
        <span class="one">One window</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.juggle { display: flex; flex-wrap: wrap; justify-content: center; gap: 10px; margin: 34px auto 0; max-width: 780px; }
.juggle span {
  padding: 9px 14px; border-radius: 12px; background: var(--card); box-shadow: inset 0 0 0 1px var(--line);
  font-weight: 600; font-size: 14.5px; color: var(--muted); transition: transform .45s var(--spring);
}
.juggle span:hover { transform: translateY(-3px) rotate(-2deg); }
.juggle .arrow { background: none; box-shadow: none; color: var(--accent); padding: 9px 4px; }
.juggle .one { background: var(--accent); color: #fff; box-shadow: 0 8px 22px color-mix(in srgb, var(--accent) 35%, transparent); }
</style>
