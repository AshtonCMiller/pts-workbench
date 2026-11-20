<script setup>
// Your hours: clocked in or out, the pay period's flagged hours (step back through earlier ones), each RO's flags
// with a switch for "on your time sheet", and scanning the pay sheet
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { hours, records, confirmFlags, clockInOrOut, scanSheet } from '../state.js';
import { payPeriod, shiftsOf, worked, fh, hm, clockTime, shortDay } from '../lib/hours.js';
import { live } from '../lib/doc.js';
import Switch from '../parts/Switch.vue';

const offset = ref(0);
const now = ref(Date.now());
let tick = 0;
onMounted(() => (tick = setInterval(() => (now.value = Date.now()), 30000)));
onBeforeUnmount(() => clearInterval(tick));

const recs = computed(() => records());
const shifts = computed(() => shiftsOf(recs.value));
const openShift = computed(() => shifts.value.filter((s) => s.end === null).pop());
const today = computed(() => {
  const d = new Date(now.value);
  d.setHours(0, 0, 0, 0);
  return worked(shifts.value, d.getTime(), now.value + 1, now.value);
});
const clockSub = computed(() => {
  if (openShift.value) return `Since ${clockTime(openShift.value.start)} · ${hm(now.value - openShift.value.start)}`;
  if (today.value) return `${hm(today.value)} today`;
  return hours.value ? 'Not clocked in today' : '';
});

const period = computed(() => payPeriod(now.value, offset.value));
const periodName = computed(() => (offset.value === 0 ? 'This pay period' : offset.value === -1 ? 'Last pay period' : `${-offset.value} pay periods ago`));

const flags = computed(() => {
  const p = period.value;
  return Object.entries(recs.value)
    .filter(([k, r]) => k.startsWith('f/') && live(r) && r.v.flaggedAt >= p.start && r.v.flaggedAt < p.end)
    .map(([k, r]) => {
      const [, ro, ...rest] = k.split('/');
      return { k, ro, key: rest.join('/'), ...r.v, hours: Number(r.v.hours) || 0 };
    })
    .sort((a, b) => a.flaggedAt - b.flaggedAt);
});

const stats = computed(() => {
  const fs = flags.value;
  const total = fs.reduce((s, f) => s + f.hours, 0);
  const conf = fs.filter((f) => f.confirmed).reduce((s, f) => s + f.hours, 0);
  const clocked = worked(shifts.value, period.value.start, Math.min(period.value.end, now.value + 1), now.value);
  const eff = clocked > 0 ? `${Math.round((total / (clocked / 3600000)) * 100)}% efficiency` : 'not clocked in yet';
  return [
    { v: `${fh(total)} h`, label: `flagged · ${fs.length} line${fs.length === 1 ? '' : 's'}` },
    { v: `${fh(conf)} h`, label: 'on your time sheet', cls: conf && conf === total ? 'good' : '' },
    { v: `${fh(total - conf)} h`, label: total - conf > 0 ? 'still to check' : 'all checked', cls: total - conf > 0 ? 'warn' : '' },
    { v: hm(clocked).replace(' m', 'm').replace(' h ', 'h '), label: eff },
  ];
});

// by RO, in the order first flagged
const groups = computed(() => {
  const ros = [...new Set(flags.value.map((f) => f.ro))];
  return ros.map((ro) => {
    const fs = flags.value.filter((f) => f.ro === ro).sort((a, b) => String(a.dms).localeCompare(String(b.dms)));
    return { ro, fs, ymm: fs.find((f) => f.ymm)?.ymm || '', hours: fs.reduce((s, f) => s + f.hours, 0), all: fs.every((f) => f.confirmed) };
  });
});

const picker = ref(null);
function picked(e) {
  const file = e.target.files && e.target.files[0];
  e.target.value = '';
  if (file) scanSheet(file);
}
</script>

<template>
  <section class="view">
    <div class="card clock" :class="{ in: !!openShift }">
      <div class="row">
        <div><h2>{{ openShift ? 'Clocked in' : 'Clocked out' }}</h2><p class="muted">{{ clockSub }}</p></div>
        <span class="dot" />
      </div>
      <button :class="{ ghost: !!openShift }" @click="clockInOrOut">{{ openShift ? 'Clock out' : 'Clock in' }}</button>
    </div>

    <div class="card">
      <div class="period">
        <button class="round" aria-label="The pay period before" @click="offset--">‹</button>
        <div><h2>{{ shortDay(period.start) }} – {{ shortDay(period.end - 1) }}</h2><p class="muted">{{ periodName }}</p></div>
        <button class="round" aria-label="The next pay period" :disabled="offset >= 0" @click="offset < 0 && offset++">›</button>
      </div>
      <p v-if="!hours" class="muted">Getting your hours…</p>
      <div v-else :key="offset" class="stats">
        <div v-for="s in stats" :key="s.label" class="stat" :class="s.cls"><b>{{ s.v }}</b><span>{{ s.label }}</span></div>
      </div>
    </div>

    <div v-if="hours && !flags.length" class="card empty">Nothing flagged in this pay period yet.</div>
    <p v-if="groups.length" class="muted" style="margin: 4px 4px 10px">Switch a line on once it’s on your time sheet.</p>
    <div v-for="(g, i) in groups" :key="g.ro" class="card" :style="{ animationDelay: `${Math.min(i, 10) * 0.035}s` }">
      <div class="grp">
        <div class="what"><b>RO {{ g.ro }}</b><span>{{ g.ymm }}</span></div>
        <span class="hrs" style="font-weight: 700">{{ fh(g.hours) }} h</span>
        <Switch :model-value="g.all" title="All on your time sheet" @update:model-value="(on) => confirmFlags(g.fs.map((f) => f.k), on)" />
      </div>
      <div v-for="f in g.fs" :key="f.k" class="fl">
        <span class="fl-letter">{{ f.dms || '·' }}</span>
        <div class="fl-what"><b>{{ f.label || 'Line' }}</b><span>{{ shortDay(f.flaggedAt) }}</span></div>
        <span class="fl-h">{{ fh(f.hours) }}</span>
        <Switch :model-value="!!f.confirmed" @update:model-value="(on) => confirmFlags([f.k], on)" />
      </div>
    </div>

    <div class="card">
      <h2>Your pay sheet</h2>
      <p>Take a picture of your Technician Payroll Report to see which of your flagged lines were paid. The picture is read on this phone, and never leaves it.</p>
      <button class="ghost" @click="picker.click()">Scan a page</button>
      <input ref="picker" type="file" accept="image/*" hidden @change="picked">
    </div>
  </section>
</template>

<style scoped>
.fl-letter {
  flex: none; width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center;
  font-weight: 700; font-size: 12px; background: var(--soft); color: var(--accent);
}
.fl-what { flex: 1; min-width: 0; }
.fl-what b { display: block; font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.fl-what span { font-size: 12px; color: var(--muted); }
.fl-h { font-weight: 700; font-variant-numeric: tabular-nums; }
</style>
