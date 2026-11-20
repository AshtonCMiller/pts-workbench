<script setup>
import { flow } from '../content.js';
</script>

<template>
  <section class="tight">
    <div class="wrap">
      <div class="center">
        <div v-reveal class="kicker">How a job goes</div>
        <h2 v-reveal>From assigned to flagged</h2>
      </div>
      <ol class="flow">
        <li v-for="(s, i) in flow" :key="s.title" v-reveal>
          <span class="n">{{ i + 1 }}</span>
          <h3>{{ s.title }}</h3>
          <p>{{ s.text }}</p>
        </li>
      </ol>
    </div>
  </section>
</template>

<style scoped>
.tight { padding-top: 24px; }
.flow { list-style: none; padding: 0; display: grid; grid-template-columns: repeat(4, 1fr); gap: 18px; margin: 44px 0 0; }
@media (max-width: 860px) { .flow { grid-template-columns: 1fr 1fr; } }
@media (max-width: 520px) { .flow { grid-template-columns: 1fr; } }
li { padding: 22px; border-radius: 18px; background: var(--card); box-shadow: inset 0 0 0 1px var(--line); transition: transform .45s var(--spring); }
li:hover { transform: translateY(-4px); }
.n {
  display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; margin-bottom: 14px;
  background: linear-gradient(135deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 800;
  transition: transform .5s var(--spring);
}
li:hover .n { transform: scale(1.15) rotate(-10deg); }
h3 { margin: 0 0 6px; font-size: 17px; }
p { margin: 0; color: var(--muted); font-size: 14.5px; }
</style>
