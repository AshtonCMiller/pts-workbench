<script setup>
// /download/: looks up the newest version, starts its installer downloading in the background (on Windows only:
// it's no use on a phone or a Mac), and says what to do next
import { ref, computed, onMounted } from 'vue';
import TopBar from '../components/TopBar.vue';
import SiteFooter from '../components/SiteFooter.vue';
import Icon from '../components/Icon.vue';
import { download } from '../content.js';

const state = ref('loading'); // loading → started | other (not Windows) | none (nothing released yet) | error
const release = ref(null); // { version, name, size, url }
const frame = ref(null);
const windows = /Windows NT/i.test(navigator.userAgent);

const size = computed(() => (release.value ? `${(release.value.size / 1048576).toFixed(0)} MB` : ''));
const fileName = computed(() => release.value?.name || 'the installer');
const steps = computed(() => download.steps.map((s) => ({ ...s, text: s.text.replace('{file}', fileName.value) })));

function start() {
  if (!release.value) return;
  // a hidden frame: the file downloads (the server sends it as an attachment) and this page stays put
  frame.value.src = release.value.url;
  state.value = 'started';
}

onMounted(async () => {
  try {
    const res = await fetch('/updates/latest/workbench.json', { cache: 'no-store' });
    if (res.status === 404) {
      state.value = 'none';
      return;
    }
    if (!res.ok) throw new Error(String(res.status));
    const m = await res.json();
    if (!m?.setup?.name) {
      state.value = 'none';
      return;
    }
    release.value = {
      version: m.version,
      name: m.setup.name,
      size: m.setup.size,
      url: `/updates/v${encodeURIComponent(m.version)}/${encodeURIComponent(m.setup.name)}`,
    };
    if (windows) setTimeout(start, 600); // a moment for the page to settle first
    else state.value = 'other';
  } catch {
    state.value = 'error';
  }
});
</script>

<template>
  <TopBar />
  <main>
    <section class="top">
      <div class="wrap center">
        <div class="badge" :class="state">
          <span class="ring" />
          <Icon name="download" :size="34" :stroke="2.2" />
        </div>

        <Transition name="swap" mode="out-in">
          <div v-if="state === 'loading'" key="loading">
            <h1>Getting the newest version…</h1>
          </div>
          <div v-else-if="state === 'started'" key="started">
            <h1>Your download is starting</h1>
            <p class="sub">Workbench {{ release.version }} for Windows · {{ size }}</p>
            <p class="again">Didn't start? <a href="#" @click.prevent="start">Download it again</a></p>
          </div>
          <div v-else-if="state === 'other'" key="other">
            <h1>Workbench is for Windows</h1>
            <p class="sub">Open <b>prodealer.works/download</b> on the shop computer you'll use it on.
              On your phone, set up the phone page instead.</p>
            <div class="cta">
              <a class="btn" href="/phone/">Set up your phone</a>
              <a class="btn ghost" href="#" @click.prevent="start">Download anyway ({{ size }})</a>
            </div>
          </div>
          <div v-else-if="state === 'none'" key="none">
            <h1>Nothing to download yet</h1>
            <p class="sub">The first version of Workbench isn't up yet. Check back soon.</p>
          </div>
          <div v-else key="error">
            <h1>Couldn't reach the server</h1>
            <p class="sub">Check the internet connection, then try again.</p>
            <div class="cta"><a class="btn" href="/download/">Try again</a></div>
          </div>
        </Transition>
      </div>
    </section>

    <section class="next">
      <div class="wrap">
        <h2 v-reveal class="center">What to do next</h2>
        <ol class="steps">
          <li v-for="(s, i) in steps" :key="s.title" v-reveal>
            <span class="n">{{ i + 1 }}</span>
            <div>
              <h3>{{ s.title }}</h3>
              <p>{{ s.text }}</p>
            </div>
          </li>
        </ol>
        <p v-reveal class="phone"><Icon name="bell" :size="18" />{{ download.phone }}</p>
      </div>
    </section>
  </main>
  <SiteFooter />
  <iframe ref="frame" class="hidden" title="download" aria-hidden="true" tabindex="-1" />
</template>

<style scoped>
.top { position: relative; padding: 64px 0 24px; }
.top::before {
  content: ""; position: absolute; inset: -120px 0 auto; height: 520px; z-index: -1; pointer-events: none;
  background: radial-gradient(560px 280px at 50% 30%, color-mix(in srgb, var(--accent-2) 22%, transparent), transparent 70%);
}
h1 { font-size: clamp(32px, 5vw, 48px); letter-spacing: -.03em; line-height: 1.1; margin: 22px 0 12px; font-weight: 800; }
.sub b { color: var(--ink); }
.again { margin: 14px 0 0; color: var(--muted); font-size: 15px; }
.again a { color: var(--accent); font-weight: 600; }
.cta { display: flex; gap: 12px; flex-wrap: wrap; justify-content: center; margin-top: 24px; }

/* the badge: the arrow bounces while the download starts, and a ring sweeps round once */
.badge {
  position: relative; width: 96px; height: 96px; margin: 0 auto; border-radius: 30px; display: grid; place-items: center;
  color: #fff; background: linear-gradient(135deg, var(--accent), var(--accent-2));
  box-shadow: 0 16px 40px color-mix(in srgb, var(--accent) 40%, transparent);
  animation: badge-in .8s var(--spring) both;
}
.badge svg { position: relative; z-index: 1; }
.badge.loading svg, .badge.started svg { animation: drop 1.4s var(--spring) infinite; }
.badge.none, .badge.error { background: linear-gradient(135deg, #7d8aa3, #a3aec2); box-shadow: none; }
.ring { position: absolute; inset: -8px; border-radius: 36px; border: 3px solid color-mix(in srgb, var(--accent) 35%, transparent); opacity: 0; }
.badge.started .ring { animation: pulse 1.8s var(--ease) infinite; }
@keyframes badge-in { from { opacity: 0; transform: scale(.6) rotate(-12deg); } }
@keyframes drop { 0% { transform: translateY(-6px); } 45% { transform: translateY(6px); } 100% { transform: translateY(-6px); } }
@keyframes pulse { 0% { opacity: .9; transform: scale(.92); } 100% { opacity: 0; transform: scale(1.18); } }

.swap-enter-active { transition: opacity .35s var(--ease), transform .6s var(--spring); }
.swap-leave-active { transition: opacity .2s var(--ease), transform .2s var(--ease); }
.swap-enter-from { opacity: 0; transform: translateY(14px) scale(.98); }
.swap-leave-to { opacity: 0; transform: translateY(-8px); }

.next { padding-top: 40px; }
.steps { list-style: none; padding: 0; margin: 36px auto 0; max-width: 760px; display: grid; gap: 14px; }
.steps li {
  display: flex; gap: 18px; align-items: flex-start; padding: 22px; border-radius: 18px;
  background: var(--card); box-shadow: inset 0 0 0 1px var(--line); transition: transform .45s var(--spring);
}
.steps li:hover { transform: translateX(6px); }
.n {
  flex: none; display: grid; place-items: center; width: 36px; height: 36px; border-radius: 50%;
  background: linear-gradient(135deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 800;
  transition: transform .5s var(--spring);
}
.steps li:hover .n { transform: scale(1.15) rotate(-10deg); }
h3 { margin: 4px 0 6px; font-size: 18px; }
.steps p { margin: 0; color: var(--muted); font-size: 15.5px; overflow-wrap: anywhere; }
.phone {
  display: flex; gap: 10px; align-items: flex-start; max-width: 760px; margin: 22px auto 0; padding: 16px 20px;
  border-radius: 16px; background: color-mix(in srgb, var(--accent) 9%, transparent); color: var(--ink); font-size: 15px;
}
.phone svg { flex: none; margin-top: 2px; color: var(--accent); }
.hidden { position: absolute; width: 0; height: 0; border: 0; visibility: hidden; }
</style>
