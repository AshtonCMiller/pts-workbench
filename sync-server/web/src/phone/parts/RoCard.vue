<script setup>
// One RO: tap to open it up to its lines; flag a line, or take a flag off (not once it's on your time sheet)
import { computed } from 'vue';
import { data, hours, toast, flagLine, unflagLine } from '../state.js';
import { linesOf, fh, shortDay } from '../lib/hours.js';

const props = defineProps({ ro: { type: Object, required: true } });

const lines = computed(() => linesOf((hours.value && hours.value.doc.records) || {}, props.ro.ro));
const flagged = computed(() => (lines.value || []).filter((l) => l.flag).reduce((s, l) => s + (Number(l.flag.hours) || 0), 0));
const total = computed(() => (lines.value || []).reduce((s, l) => s + l.paid, 0));
const open = computed(() => data.open.has(props.ro.ro));

function toggle() {
  if (open.value) data.open.delete(props.ro.ro);
  else data.open.add(props.ro.ro);
}

function sub(l) {
  if (l.flag) return `Flagged ${shortDay(l.flag.flaggedAt)}${l.flag.confirmed ? ' · on your time sheet' : ''}`;
  return l.completed ? 'Done in Xtime' : l.diag ? 'Diagnosis' : '';
}

function tap(l) {
  if (!l.flag) flagLine(props.ro.ro, l);
  else if (l.flag.confirmed) toast('That one’s on your time sheet: take it off on your computer.', true);
  else if (confirm(`Take the flag off RO ${props.ro.ro} line ${l.dms || ''}?`)) unflagLine(props.ro.ro, l.key);
}
</script>

<template>
  <div class="card ro" :class="{ open }">
    <button class="top" @click="toggle">
      <div class="head">
        <b>RO {{ ro.ro }}</b>
        <span v-if="ro.tag" class="tag">Tag {{ ro.tag }}</span>
        <span v-if="lines" class="hrs">{{ fh(flagged) }}<span class="muted" style="font-weight: 500"> / {{ fh(total) }} h</span></span>
      </div>
      <div class="ymm">{{ ro.ymm || '' }}</div>
      <div v-if="(ro.tags || []).length || ro.open || ro.mine === false" class="chips">
        <span v-for="[label, color] in ro.tags || []" :key="label" class="chip" :style="{ background: color }">{{ label }}</span>
        <span v-if="ro.open" class="chip ghosty">Open on your computer</span>
        <span v-if="ro.mine === false" class="chip ghosty">Not on your list</span>
      </div>
    </button>
    <div class="lines">
      <div>
        <ul>
          <li v-if="!lines" class="ln"><div class="what"><span>Open this RO on your computer once to see its lines here.</span></div></li>
          <li v-else-if="!lines.length" class="ln"><div class="what"><span>No approved lines yet.</span></div></li>
          <li v-for="l in lines || []" :key="l.key" class="ln">
            <span class="letter">{{ l.dms || '·' }}</span>
            <div class="what"><b>{{ l.text || 'Line' }}</b><span v-if="sub(l)">{{ sub(l) }}</span></div>
            <span class="h">{{ fh(l.flag ? l.flag.hours : l.paid) }}</span>
            <button class="flagbtn" :class="l.flag ? 'on' : 'ghost'" @click="tap(l)">{{ l.flag ? 'Flagged' : 'Flag' }}</button>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
