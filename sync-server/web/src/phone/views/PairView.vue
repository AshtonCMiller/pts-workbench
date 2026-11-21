<script setup>
import { show } from '../state.js';
import InstallCard from '../parts/InstallCard.vue';
</script>

<template>
  <section class="view">
    <div class="card">
      <h2>Pair with your computer</h2>
      <p>In Workbench on your computer, click <b>Pair phone</b> at the bottom right, then scan the code it shows.</p>
      <button @click="show('scan')">Scan the code</button>
      <button class="plain" @click="show('type')">Type it in instead</button>
    </div>
    <InstallCard />
  </section>
</template>
