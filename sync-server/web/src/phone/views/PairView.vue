<script setup>
import { show } from '../state.js';
</script>

<template>
  <section class="view">
    <div class="card">
      <h2>Pair with your computer</h2>
      <p>In PTS Workbench on your computer, click <b>Pair phone</b> at the bottom right, then scan the code it shows.</p>
      <button @click="show('scan')">Scan the code</button>
      <button class="plain" @click="show('type')">Type it in instead</button>
    </div>
  </section>
</template>
