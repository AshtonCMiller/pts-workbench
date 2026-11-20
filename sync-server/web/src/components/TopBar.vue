<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue';
import Icon from './Icon.vue';
import { brand } from '../content.js';

// the bar gets its line once the page is scrolled
const scrolled = ref(false);
const onScroll = () => (scrolled.value = window.scrollY > 8);
onMounted(() => {
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();
});
onBeforeUnmount(() => window.removeEventListener('scroll', onScroll));
</script>

<template>
  <header class="top" :class="{ scrolled }">
    <div class="wrap">
      <a class="brand" href="/">
        <span class="mark"><Icon name="wrench" :size="17" :stroke="2.4" /></span>
        <span>{{ brand.name }}<span class="tld">{{ brand.tld }}</span></span>
      </a>
      <nav class="links">
        <a href="/#features">Features</a>
        <a href="/#phone">Phone</a>
        <a href="/#next">What's next</a>
        <a class="btn small" href="/download/">Download</a>
      </nav>
    </div>
  </header>
</template>

<style scoped>
.top {
  position: sticky; top: 0; z-index: 10;
  background: color-mix(in srgb, var(--bg) 82%, transparent);
  backdrop-filter: saturate(1.6) blur(14px); -webkit-backdrop-filter: saturate(1.6) blur(14px);
  border-bottom: 1px solid transparent; transition: border-color .3s;
}
.top.scrolled { border-bottom-color: var(--line); }
.wrap { display: flex; align-items: center; gap: 24px; height: 64px; }
.brand { display: flex; align-items: center; gap: 10px; text-decoration: none; font-weight: 750; font-size: 18px; letter-spacing: -.01em; }
.mark {
  width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center;
  background: linear-gradient(135deg, var(--accent), var(--accent-2)); color: #fff;
  transition: transform .5s var(--spring);
}
.brand:hover .mark { transform: rotate(-12deg) scale(1.08); }
.tld { color: var(--muted); font-weight: 600; }
.links { display: flex; gap: 22px; margin-left: auto; align-items: center; }
.links a:not(.btn) { text-decoration: none; color: var(--muted); font-size: 15px; font-weight: 550; transition: color .2s; }
.links a:not(.btn):hover { color: var(--ink); }
@media (max-width: 720px) { .links a:not(.btn) { display: none; } }
</style>
