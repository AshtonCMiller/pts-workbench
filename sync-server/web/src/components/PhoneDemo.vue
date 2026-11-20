<script setup>
// The phone, drawn: notifications spring in one after another once it's on screen (made up, content.js)
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { phone } from '../content.js';
import { reducedMotion } from '../motion.js';

const EVERY = 2600;
const now = new Date();
const time = now.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }).replace(/\s?[AP]M$/i, '');
const date = now.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' });

const root = ref(null);
const shown = ref([]);
let next = 0;
let id = 0;
let timer = 0;
let pause = 0;
let seen = null;

function add() {
  // three on screen: they all go, then the next one comes in
  if (shown.value.length >= 3) {
    shown.value = [];
    pause = setTimeout(add, 420);
    return;
  }
  const n = phone.notes[next++ % phone.notes.length];
  shown.value = [{ ...n, id: id++ }, ...shown.value];
}

onMounted(() => {
  seen = new IntersectionObserver(
    ([e]) => {
      if (!e.isIntersecting) return;
      seen.disconnect();
      add();
      if (reducedMotion()) {
        add();
        add();
      } else timer = setInterval(add, EVERY);
    },
    { threshold: 0.3 },
  );
  seen.observe(root.value);
});
onBeforeUnmount(() => {
  seen?.disconnect();
  clearInterval(timer);
  clearTimeout(pause);
});
</script>

<template>
  <div ref="root" class="phone">
    <div class="screen">
      <div class="clock"><b>{{ time }}</b><small>{{ date }}</small></div>
      <TransitionGroup name="note" tag="div" class="notes">
        <div v-for="n in shown" :key="n.id" class="note">
          <div class="app"><i />WORKBENCH<span>now</span></div>
          <b>{{ n.title }}</b>
          <div>{{ n.body }}</div>
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>

<style scoped>
.phone {
  width: 270px; height: 540px; margin: 0 auto; border-radius: 44px; padding: 12px; position: relative;
  background: #0d1422; box-shadow: var(--shadow), inset 0 0 0 2px #2a3550;
}
.screen {
  height: 100%; border-radius: 33px; overflow: hidden; position: relative;
  background: linear-gradient(160deg, #1d4f9c, #0e2a57 55%, #0a1730);
}
.clock { text-align: center; color: #fff; padding-top: 54px; }
.clock b { display: block; font-size: 58px; font-weight: 300; letter-spacing: -.02em; line-height: 1; }
.clock small { opacity: .8; font-size: 14px; }
.notes { position: absolute; left: 10px; right: 10px; bottom: 26px; display: grid; gap: 8px; }
.note {
  background: rgba(255, 255, 255, .86); color: #0f1a2b; border-radius: 18px; padding: 11px 13px; font-size: 12.5px;
  backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px); transform-origin: 50% 100%;
}
.app { display: flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 700; color: #5b6880; margin-bottom: 3px; }
.app i { width: 15px; height: 15px; border-radius: 4px; background: linear-gradient(135deg, #1a5fb4, #3d8bfd); }
.app span { margin-left: auto; font-weight: 500; }
.note b { display: block; font-size: 13px; }
/* in: springs up; the others slide down to make room; out: all fade together */
.note-enter-active { transition: opacity .4s var(--ease), transform .8s var(--spring); }
.note-enter-from { opacity: 0; transform: translateY(30px) scale(.9); }
.note-move { transition: transform .6s var(--spring); }
.note-leave-active { transition: opacity .35s var(--ease), transform .35s var(--ease); }
.note-leave-to { opacity: 0; transform: scale(.94); }
</style>
