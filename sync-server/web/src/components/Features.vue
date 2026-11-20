<script setup>
import Icon from './Icon.vue';
import { features } from '../content.js';
</script>

<template>
  <section id="features" class="tight">
    <div class="wrap">
      <div class="center">
        <div v-reveal class="kicker">What it does</div>
        <h2 v-reveal>Everything for the job, in the bay</h2>
      </div>
      <div class="grid">
        <div v-for="f in features" :key="f.title" v-reveal class="feat">
          <div class="ic"><Icon :name="f.icon" :size="22" /></div>
          <h3>{{ f.title }}</h3>
          <p>{{ f.text }}</p>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tight { padding-top: 24px; }
.grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 18px; margin-top: 44px; }
@media (max-width: 920px) { .grid { grid-template-columns: repeat(2, 1fr); } }
@media (max-width: 600px) { .grid { grid-template-columns: 1fr; } }
.feat {
  background: var(--card); border-radius: 18px; padding: 24px; box-shadow: inset 0 0 0 1px var(--line);
  transition: transform .45s var(--spring), box-shadow .3s;
}
.feat:hover { transform: translateY(-5px); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, var(--line)), var(--shadow); }
.ic {
  width: 44px; height: 44px; border-radius: 13px; display: grid; place-items: center; margin-bottom: 16px;
  background: color-mix(in srgb, var(--accent) 13%, transparent); color: var(--accent);
  transition: transform .5s var(--spring);
}
.feat:hover .ic { transform: rotate(-8deg) scale(1.1); }
h3 { margin: 0 0 8px; font-size: 18px; letter-spacing: -.01em; }
p { margin: 0; color: var(--muted); font-size: 15px; }
</style>
