<script setup>
// The pay sheet, checked against your flags: what was paid, paid differently, or missing, and the changes it says to
// make to your flags (each with a switch; nothing changes until you tap Apply)
import { ref, computed, watch, toRaw } from 'vue';
import { sheet, show, scanSheet, clearSheet, applyChanges } from '../state.js';
import { Sheet } from '../lib/sheet.js';
import { proposed } from '../lib/proposed.js';
import { h2, niceDay, dayOfMs } from '../lib/hours.js';
import Switch from '../parts/Switch.vue';

const pages = computed(() => sheet.sheet.pages || 0);
const period = computed(() => Sheet.periodOf(toRaw(sheet.sheet)));
const title = computed(() => (period.value ? `Pay sheet · ${niceDay(period.value.from)} – ${niceDay(period.value.to)}` : 'Pay sheet'));
// which pay period it's for: the sheet's own header, or (that unreadable) the days most of its lines were booked
const about = computed(() => {
  if (!pages.value) return 'Scan each page of your Technician Payroll Report.';
  const p = period.value;
  const which = p
    ? ` For the pay period ${niceDay(p.from)} – ${niceDay(p.to)} (${p.read ? 'from the top of the sheet' : 'going by the days its lines were booked'}): only your flags from then are checked.`
    : '';
  return `${pages.value} page${pages.value === 1 ? '' : 's'} scanned, ${sheet.sheet.rows.length} rows read.${which}${sheet.flagsError ? ` ${sheet.flagsError}` : ''}`;
});

const result = computed(() => (pages.value ? Sheet.compare(toRaw(sheet.sheet), toRaw(sheet.flags) || []) : null));

const stats = computed(() => {
  const r = result.value;
  if (!r) return [];
  if (!sheet.flags) return [{ v: h2(r.soldTotal), label: 'sold on these pages' }];
  return [
    { v: `${r.paid.length}`, of: `of ${r.flags}`, label: 'flagged lines paid', cls: r.paid.length === r.flags ? 'good' : '' },
    { v: `${r.different.length}`, label: 'paid different hours', cls: r.different.length ? 'warn' : '' },
    { v: `${r.missing.length}`, label: 'not on these pages', cls: r.missing.length ? 'bad' : '' },
    { v: h2(r.soldTotal), label: `sold · you flagged ${h2(r.flaggedTotal)}` },
  ];
});

// the sections, each a list of rows: { title, sub[], code[], hrs }
const sections = computed(() => {
  const r = result.value;
  if (!r) return [];
  const raw = (rows) => rows.map((x) => x.raw);
  return [
    { title: 'Paid different hours', open: true, items: r.different.map((d) => ({
      title: `RO ${d.flag.ro} · line ${d.line.ln}`,
      sub: [`${d.flag.label} · booked ${niceDay(d.line.booked)}`, `You flagged ${h2(d.flag.hours)}${d.unsure ? ' · the photo wasn’t clear: check the paper' : ''}`],
      code: d.unsure ? raw(d.line.rows) : [], hrs: h2(d.sold) })) },
    { title: 'Not on these pages', open: true, items: r.missing.map((m) => ({
      title: `RO ${m.flag.ro} · line ${m.flag.dms || '?'}`,
      sub: [`${m.flag.label} · flagged ${niceDay(dayOfMs(m.flag.flaggedAt))}${m.flag.confirmed ? ' · marked as on your time sheet' : ''}`, 'On another page, or not paid yet'],
      code: [], hrs: h2(m.flag.hours) })) },
    { title: 'Paid, but not flagged here', open: false, items: r.unflagged.map((u) => ({
      title: `RO ${u.line.ro} · line ${u.line.ln}`,
      sub: [`Booked ${niceDay(u.line.booked)}${u.unsure ? ' · check the paper' : ''}`], code: [], hrs: h2(u.sold) })) },
    { title: 'Paid', open: false, items: r.paid.map((p) => ({
      title: `RO ${p.flag.ro} · line ${p.line.ln}`,
      sub: [`${p.flag.label} · booked ${niceDay(p.line.booked)}${p.flag.confirmed ? ' · already marked' : ''}`], code: [], hrs: h2(p.sold) })) },
  ].filter((s) => s.items.length);
});

// what to change, shown first; each can be switched off
const changes = ref([]);
watch(result, (r) => (changes.value = r && sheet.flags ? proposed(r) : []), { immediate: true });
const picked = computed(() => changes.value.filter((c) => c.on));
const unconfirming = computed(() => changes.value.filter((c) => c.set.confirmed === false).length);
// what needs checking on the paper (the photo wasn't clear): first, where it's easy to see
const toCheck = computed(() => changes.value.filter((c) => c.check));
const sure = computed(() => changes.value.filter((c) => !c.check));
const unread = computed(() => (result.value ? result.value.unread : []));
const applying = ref(false);
async function apply() {
  if (!picked.value.length) return;
  applying.value = true;
  await applyChanges(picked.value);
  applying.value = false;
}

const picker = ref(null);
function chosen(e) {
  const file = e.target.files && e.target.files[0];
  e.target.value = '';
  if (file) scanSheet(file);
}
</script>

<template>
  <section class="view">
    <div class="card">
      <div class="row">
        <h2>{{ title }}</h2>
        <button class="plain" style="width: auto" @click="show('hours')">Done</button>
      </div>
      <p>{{ about }}</p>
      <Transition name="fade">
        <div v-if="sheet.busy">
          <div class="bar"><i :style="{ width: `${Math.round(sheet.busy.pct * 100)}%` }" /></div>
          <p class="muted">{{ sheet.busy.text }}</p>
        </div>
      </Transition>
      <div v-if="!sheet.busy && stats.length" class="stats">
        <div v-for="s in stats" :key="s.label" class="stat" :class="s.cls">
          <b>{{ s.v }}<span v-if="s.of" class="count">{{ s.of }}</span></b><span>{{ s.label }}</span>
        </div>
      </div>
      <button class="ghost" style="margin-top: 14px" :disabled="!!sheet.busy" @click="picker.click()">{{ pages ? 'Scan another page' : 'Scan a page' }}</button>
      <button v-if="pages" class="plain" :disabled="!!sheet.busy" @click="clearSheet">Start over</button>
      <input ref="picker" type="file" accept="image/*" hidden @change="chosen">
    </div>

    <div v-if="!sheet.busy && (toCheck.length || unread.length)" class="card check">
      <h2>Check these on the paper<span class="count">{{ toCheck.length + unread.length }}</span></h2>
      <p>The photo wasn’t clear on these. Look at the sheet, and switch on the ones that are right.</p>
      <ul class="items">
        <li v-for="(c, i) in toCheck" :key="c.id" :style="{ animationDelay: `${Math.min(i, 12) * 0.03}s` }">
          <Switch v-model="c.on" />
          <div class="what"><b>{{ c.text }}</b><span>{{ c.why }}</span></div>
        </li>
        <li v-for="(x, i) in unread" :key="`u${i}`" :style="{ animationDelay: `${Math.min(toCheck.length + i, 12) * 0.03}s` }">
          <div class="what">
            <b>{{ x.ro ? `RO ${x.ro}…` : 'A row' }}</b>
            <span>Couldn’t be read: if it’s one of your lines, change it on the Hours tab</span>
            <code>{{ x.raw }}</code>
          </div>
        </li>
      </ul>
    </div>

    <div v-if="!sheet.busy && changes.length" class="card">
      <h2>Changes to make</h2>
      <p>Here’s what the sheet says to change. Nothing changes until you tap Apply; turn off any you don’t want. Your computer picks them up at its next sync.</p>
      <p v-if="unconfirming" class="warn-note">{{ unconfirming }} line{{ unconfirming === 1 ? '' : 's' }} you’d marked as on your time sheet {{ unconfirming === 1 ? 'isn’t' : 'aren’t' }} on these pages. If the sheet has more pages, scan them before you apply.</p>
      <p v-if="toCheck.length" class="muted">Plus {{ toCheck.length }} to check on the paper, above ({{ toCheck.filter((c) => c.on).length }} switched on).</p>
      <ul class="items">
        <li v-for="(c, i) in sure" :key="c.id" :style="{ animationDelay: `${Math.min(i, 12) * 0.03}s` }">
          <Switch v-model="c.on" />
          <div class="what"><b>{{ c.text }}</b><span>{{ c.why }}</span></div>
        </li>
      </ul>
      <button :disabled="!picked.length || applying" @click="apply">
        {{ applying ? 'Applying…' : picked.length ? `Apply ${picked.length} change${picked.length === 1 ? '' : 's'}` : 'Nothing to apply' }}
      </button>
    </div>

    <template v-if="!sheet.busy">
      <details v-for="s in sections" :key="s.title" class="card" :open="s.open">
        <summary><h2>{{ s.title }}<span class="count">{{ s.items.length }}</span></h2></summary>
        <ul class="items">
          <li v-for="(it, i) in s.items" :key="i" :style="{ animationDelay: `${Math.min(i, 12) * 0.03}s` }">
            <div class="what">
              <b>{{ it.title }}</b>
              <span v-for="(line, j) in it.sub" :key="j">{{ line }}</span>
              <code v-for="(line, j) in it.code" :key="`c${j}`">{{ line }}</code>
            </div>
            <div v-if="it.hrs" class="hrs">{{ it.hrs }}</div>
          </li>
        </ul>
      </details>
    </template>
  </section>
</template>

<style scoped>
.warn-note { color: #b45309; font-size: 14px; font-weight: 600; }
.check { box-shadow: inset 4px 0 0 #d97706; }
.check h2 { color: #b45309; }
</style>
