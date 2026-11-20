<script setup>
// Your ROs: the list your computer sent, each opening up to its lines, which you can flag from here
import { computed } from 'vue';
import { data, hours, refresh } from '../state.js';
import { ago } from '../lib/hours.js';
import Icon from '../parts/Icon.vue';
import RoCard from '../parts/RoCard.vue';

const ros = computed(() => data.rosDoc?.ros || []);
const asOf = computed(() => (data.rosDoc?.at ? `From your computer, ${ago(data.rosDoc.at)}` : data.refreshing ? 'Getting them…' : ''));
const empty = computed(() => {
  if (!hours.value && data.refreshing) return 'Getting your ROs…';
  if (data.rosError) return `Couldn't get your ROs (${data.rosError}).`;
  if (data.rosDoc) return 'No ROs on your list right now.';
  return 'Your RO list shows here once Workbench on your computer has sent it (it does every few minutes while it’s open, once this phone is paired).';
});
</script>

<template>
  <section class="view">
    <div class="titlebar">
      <div><h2>Your ROs</h2><p class="muted">{{ asOf }}</p></div>
      <button class="round" :class="{ spin: data.refreshing }" aria-label="Refresh" @click="refresh()"><Icon name="refresh" :stroke="2.2" /></button>
    </div>
    <div v-if="!ros.length" class="card empty">{{ empty }}</div>
    <RoCard v-for="(ro, i) in ros" :key="ro.ro" :ro="ro" :style="{ animationDelay: `${Math.min(i, 10) * 0.035}s` }" />
  </section>
</template>
