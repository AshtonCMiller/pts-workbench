<script setup>
import { final } from '../content.js';
</script>

<template>
  <section class="tight">
    <div class="wrap">
      <div v-reveal class="final">
        <h2>{{ final.title }}</h2>
        <p>{{ final.text }}</p>
        <div class="cta">
          <a class="btn" href="/download/">Download for Windows</a>
          <a class="btn ghost" href="/phone/">Set up your phone</a>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tight { padding-top: 8px; }
.final {
  margin: 0 auto; max-width: 880px; text-align: center; padding: 52px 28px; border-radius: 28px; color: #fff;
  background: radial-gradient(500px 220px at 20% 0%, rgba(255, 255, 255, .18), transparent 70%), linear-gradient(135deg, #1a5fb4, #2f7be6);
  box-shadow: 0 20px 60px rgba(26, 95, 180, .35);
}
h2 { color: #fff; }
p { color: rgba(255, 255, 255, .85); font-size: 18px; margin: 0 0 26px; }
.cta { display: flex; gap: 12px; flex-wrap: wrap; justify-content: center; }
.btn { background: #fff; color: #1a5fb4; box-shadow: 0 8px 24px rgba(0, 0, 0, .18); }
.btn.ghost { background: rgba(255, 255, 255, .14); color: #fff; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, .4); }
</style>
