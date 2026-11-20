<script setup>
import { brand } from '../content.js';

const year = new Date().getFullYear();
</script>

<template>
  <footer>
    <div class="wrap">
      <span>© {{ year }} {{ brand.name }} Works. Built by technicians, for dealership service departments.</span>
      <nav><a href="/#features">Features</a><a href="/phone/">Phone</a><a href="/download/">Download</a></nav>
    </div>
  </footer>
</template>

<style scoped>
footer { padding: 40px 0 56px; color: var(--muted); font-size: 14px; }
.wrap { display: flex; flex-wrap: wrap; gap: 14px 28px; align-items: center; }
nav { display: flex; gap: 20px; margin-left: auto; }
a { text-decoration: none; }
a:hover { color: var(--ink); }
</style>
