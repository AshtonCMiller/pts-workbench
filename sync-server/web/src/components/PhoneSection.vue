<script setup>
import Icon from './Icon.vue';
import PhoneDemo from './PhoneDemo.vue';
import { phone } from '../content.js';
</script>

<template>
  <section id="phone" class="band">
    <div class="wrap split">
      <div>
        <div v-reveal class="kicker">On your phone</div>
        <h2 v-reveal>{{ phone.title }}</h2>
        <p v-reveal class="sub">{{ phone.text }}</p>
        <ul class="ticks">
          <li v-for="p in phone.points" :key="p" v-reveal><Icon name="check" :size="18" :stroke="2.6" />{{ p }}</li>
        </ul>
        <a v-reveal class="btn" href="/phone/">Open on your phone</a>
      </div>
      <div v-reveal aria-hidden="true">
        <PhoneDemo />
      </div>
    </div>
  </section>
</template>

<style scoped>
.band { background: var(--bg-2); }
.split { display: grid; grid-template-columns: 1fr 1fr; gap: 56px; align-items: center; }
@media (max-width: 860px) { .split { grid-template-columns: 1fr; gap: 40px; } }
.sub { margin: 0; }
.ticks { list-style: none; padding: 0; margin: 22px 0 28px; display: grid; gap: 12px; }
.ticks li { display: flex; gap: 12px; align-items: flex-start; font-size: 16px; }
.ticks svg { flex: none; margin-top: 3px; color: var(--accent); }
</style>
