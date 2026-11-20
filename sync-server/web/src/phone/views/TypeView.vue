<script setup>
import { ref, onMounted } from 'vue';
import { show, toast, paired } from '../state.js';
import { derive } from '../lib/keys.js';

const storeId = ref('');
const employee = ref('');
const first = ref(null);
onMounted(() => setTimeout(() => first.value?.focus(), 300));

async function pair() {
  const p = await derive(storeId.value, employee.value);
  if (!p) toast('Fill in both.', true);
  else paired(p);
}
</script>

<template>
  <section class="view">
    <div class="card">
      <h2>Type it in</h2>
      <p>The same as in the app's Settings on your computer.</p>
      <form autocomplete="off" @submit.prevent="pair">
        <label for="i-store">Store ID</label>
        <input id="i-store" ref="first" v-model="storeId" inputmode="numeric" autocapitalize="off" spellcheck="false" required>
        <label for="i-emp">Employee number</label>
        <input id="i-emp" v-model="employee" inputmode="numeric" autocapitalize="off" spellcheck="false" required>
        <button type="submit">Pair</button>
        <button type="button" class="plain" @click="show('pair')">Back</button>
      </form>
    </div>
  </section>
</template>
