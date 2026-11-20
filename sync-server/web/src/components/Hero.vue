<script setup>
import Icon from './Icon.vue';
import AppDemo from './AppDemo.vue';
import { hero } from '../content.js';
</script>

<template>
  <section class="hero">
    <div class="wrap">
      <div>
        <span v-reveal class="eyebrow"><i />{{ hero.eyebrow }}</span>
        <h1 v-reveal>{{ hero.title[0] }}<br><span class="grad">{{ hero.title[1] }}</span></h1>
        <p v-reveal class="lede">{{ hero.lede }}</p>
        <div v-reveal class="cta">
          <a class="btn" href="/download/"><Icon name="download" :size="18" :stroke="2.2" />Download for Windows</a>
          <a class="btn ghost" href="/#features">See what it does</a>
        </div>
        <div v-reveal class="fine">
          <span v-for="f in hero.fine" :key="f"><Icon name="check" :size="16" :stroke="2.6" />{{ f }}</span>
        </div>
      </div>
      <div v-reveal aria-hidden="true">
        <AppDemo />
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero { position: relative; padding: 72px 0 40px; }
.hero::before {
  content: ""; position: absolute; inset: -120px 0 auto; height: 620px; z-index: -1; pointer-events: none;
  background:
    radial-gradient(600px 300px at 18% 30%, color-mix(in srgb, var(--accent-2) 22%, transparent), transparent 70%),
    radial-gradient(520px 280px at 85% 20%, color-mix(in srgb, var(--accent) 18%, transparent), transparent 70%);
}
.wrap { display: grid; grid-template-columns: 1fr 1.15fr; gap: 48px; align-items: center; }
@media (max-width: 920px) { .wrap { grid-template-columns: 1fr; gap: 40px; } .hero { padding-top: 40px; } }
.eyebrow {
  display: inline-flex; align-items: center; gap: 8px; padding: 5px 12px; border-radius: 99px;
  background: var(--card); box-shadow: inset 0 0 0 1px var(--line); font-size: 13px; font-weight: 600; color: var(--muted);
}
.eyebrow i { width: 7px; height: 7px; border-radius: 50%; background: var(--good); box-shadow: 0 0 0 4px color-mix(in srgb, var(--good) 20%, transparent); }
h1 { font-size: clamp(38px, 6vw, 62px); line-height: 1.04; letter-spacing: -.035em; margin: 18px 0; font-weight: 800; }
.grad { background: linear-gradient(100deg, var(--accent), var(--accent-2)); -webkit-background-clip: text; background-clip: text; color: transparent; }
.lede { font-size: 19px; color: var(--muted); max-width: 34em; margin: 0 0 28px; }
.cta { display: flex; gap: 12px; flex-wrap: wrap; }
.fine { display: flex; gap: 10px 18px; flex-wrap: wrap; margin-top: 20px; font-size: 14px; color: var(--muted); }
.fine span { display: inline-flex; align-items: center; gap: 6px; }
.fine svg { color: var(--good); }
</style>
