<script setup>
// One of your lines, as the pay sheet says to change it (lib/proposed.js): a switch to make the change or not (when
// the photo was clear), the hours it really paid (pick what the photo read, or type them in), the day it was
// booked, and whether it's on the paper at all
import { computed } from 'vue';
import { h2 } from '../lib/hours.js';
import { hoursOf, setOf } from '../lib/proposed.js';
import Switch from './Switch.vue';

const props = defineProps({ item: { type: Object, required: true } });
const it = props.item;

const showHours = computed(() => !it.missing && (it.paid !== null || it.hoursUnsure || it.readings.length));
const typed = computed(() => hoursOf(it.paid));
// (a button that's what's typed: it's picked)
const isPicked = (h) => typed.value !== null && Math.abs(typed.value - h) < 0.001;
const change = computed(() => setOf(it));
const summary = computed(() => {
  const s = change.value;
  if (!s) return it.check ? 'Not answered yet: left as it is' : 'Nothing will change';
  const parts = [];
  if (s.confirmed === false) parts.push('marked as not on your time sheet');
  else if (s.confirmed) parts.push('marked as on your time sheet');
  if (s.hours !== undefined) parts.push(`${h2(it.flagged)} → ${h2(s.hours)}`);
  if (s.flaggedAt !== undefined) parts.push(`moved to ${it.move.to}`);
  return parts.join(' · ').replace(/^./, (c) => c.toUpperCase());
});
</script>

<template>
  <li class="sheet-item" :class="{ off: !it.check && !it.on }">
    <div class="head">
      <div class="what">
        <b>{{ it.title }}</b>
        <span v-if="it.label">{{ it.label }}</span>
      </div>
      <Switch v-if="!it.check" v-model="it.on" title="Make this change" />
    </div>
    <p v-for="w in it.why" :key="w" class="why">{{ w }}</p>

    <template v-if="it.missing && it.check">
      <p class="q">Is it on the paper?</p>
      <div class="answers">
        <button :class="{ picked: it.onSheet === true }" @click="it.onSheet = it.onSheet === true ? null : true">Yes: leave it</button>
        <button :class="{ picked: it.onSheet === false }" @click="it.onSheet = it.onSheet === false ? null : false">No: unmark it</button>
      </div>
    </template>

    <template v-if="showHours">
      <div class="paid">
        <label :for="`paid-${it.id}`">Paid</label>
        <input :id="`paid-${it.id}`" v-model="it.paid" inputmode="decimal" placeholder="?" @focus="$event.target.select()">
        <span>h</span>
        <small>flagged {{ h2(it.flagged) }}</small>
      </div>
      <div v-if="it.hoursUnsure" class="answers">
        <button v-for="h in it.readings" :key="h" :class="{ picked: isPicked(h) }" @click="it.paid = String(h)">{{ h2(h) }}</button>
        <button :class="{ picked: isPicked(it.flagged) }" @click="it.paid = String(it.flagged)">{{ h2(it.flagged) }}, as flagged</button>
      </div>
    </template>

    <template v-if="it.move">
      <p v-if="it.move.sure" class="why">Booked {{ it.move.to }} (you flagged it {{ it.move.from }})</p>
      <template v-else>
        <p class="q">Which day was it booked? (the date was hard to read)</p>
        <div class="answers">
          <button :class="{ picked: it.moveOn === true }" @click="it.moveOn = it.moveOn === true ? null : true">{{ it.move.to }}: move it</button>
          <button :class="{ picked: it.moveOn === false }" @click="it.moveOn = it.moveOn === false ? null : false">{{ it.move.from }}: leave it</button>
        </div>
      </template>
    </template>

    <p class="result" :class="{ none: !change }">{{ summary }}</p>
  </li>
</template>

<style scoped>
.sheet-item { flex-direction: column; align-items: stretch !important; gap: 8px !important; transition: opacity .3s; }
.sheet-item.off { opacity: .55; }
.head { display: flex; gap: 10px; align-items: flex-start; }
.head .what { flex: 1; min-width: 0; }
.why { margin: 0; font-size: 13px; color: var(--muted); }
.q { margin: 2px 0 0; font-size: 14px; font-weight: 600; }
.answers { display: flex; flex-wrap: wrap; gap: 8px; }
.answers button {
  flex: 1 1 30%; margin: 0; padding: 10px 8px; font-size: 14px; background: var(--soft); color: var(--accent);
  transition: background .25s, color .25s, transform .3s var(--spring);
}
.answers button.picked { background: var(--accent); color: var(--accent-ink); transform: scale(1.03); }
.paid { display: flex; align-items: center; gap: 8px; }
.paid label { margin: 0; font-size: 14px; color: var(--text, inherit); font-weight: 600; }
.paid input { width: 90px; padding: 8px 10px; font-size: 16px; text-align: right; font-variant-numeric: tabular-nums; }
.paid span { font-weight: 600; }
.paid small { margin-left: auto; color: var(--muted); font-size: 12px; }
.result { margin: 0; font-size: 13px; font-weight: 600; color: var(--good); }
.result.none { color: var(--muted); font-weight: 500; }
</style>
