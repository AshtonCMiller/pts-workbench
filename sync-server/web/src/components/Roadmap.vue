<script setup>
import { roadmap } from '../content.js';
</script>

<template>
  <section id="next">
    <div class="wrap">
      <div class="center">
        <div v-reveal class="kicker">What's next</div>
        <h2 v-reveal>{{ roadmap.title }}</h2>
        <p v-reveal class="sub">{{ roadmap.text }}</p>
      </div>
      <div class="road">
        <div v-for="r in roadmap.items" :key="r.title" v-reveal>
          <span class="soon">Planned</span>
          <h3>{{ r.title }}</h3>
          <p>{{ r.text }}</p>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.road { display: grid; grid-template-columns: repeat(4, 1fr); gap: 18px; margin-top: 44px; }
@media (max-width: 920px) { .road { grid-template-columns: 1fr 1fr; } }
@media (max-width: 520px) { .road { grid-template-columns: 1fr; } }
.road > div { padding: 22px; border-radius: 18px; border: 1.5px dashed var(--line); transition: border-color .3s, transform .45s var(--spring); }
.road > div:hover { border-color: var(--accent); transform: translateY(-4px); }
.soon {
  display: inline-block; font-size: 11.5px; font-weight: 700; color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent); padding: 3px 9px; border-radius: 99px; margin-bottom: 12px;
}
h3 { margin: 0 0 6px; font-size: 17px; }
p { margin: 0; color: var(--muted); font-size: 14.5px; }
</style>
