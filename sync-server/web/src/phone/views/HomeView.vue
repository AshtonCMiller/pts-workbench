<script setup>
// Paired: notifications (on or off, and why), a test, the last few that came in while the page was open, and unpairing
import { computed } from 'vue';
import { notify, ios, standalone, canPush, turnOnNotifications, sendTest, unpair } from '../state.js';
import Icon from '../parts/Icon.vue';
import InstallCard from '../parts/InstallCard.vue';

const permission = () => (canPush ? Notification.permission : 'unsupported');
const about = computed(() => {
  if (notify.on) return 'When something on your ROs is waiting on you and you haven’t seen it on your computer, it comes here.';
  if (!canPush) {
    return ios && !standalone
      ? 'Open Workbench from your Home Screen to turn on notifications.'
      : "This browser can't show notifications. On an iPhone, it needs iOS 16.4 or newer.";
  }
  if (permission() === 'denied') {
    return ios
      ? 'Notifications are turned off for Workbench. Turn them on in Settings › Notifications › Workbench.'
      : "Notifications are blocked for this site. Allow them in your browser's site settings.";
  }
  return 'When something on your ROs is waiting on you and you haven’t seen it on your computer, it comes to your phone.';
});
const canTurnOn = computed(() => !notify.on && canPush && permission() !== 'denied');
</script>

<template>
  <section class="view">
    <div class="card">
      <div class="status">
        <div class="tick"><Icon name="tick" :stroke="2.6" /></div>
        <div>
          <h2>Paired</h2>
          <p class="muted">With your Workbench profile.</p>
        </div>
      </div>
    </div>
    <div class="card">
      <div class="row">
        <h2>Notifications</h2>
        <span class="pill" :class="{ on: notify.on }">{{ notify.on ? 'On' : 'Off' }}</span>
      </div>
      <p>{{ about }}</p>
      <button v-if="canTurnOn" :disabled="notify.busy" @click="turnOnNotifications">Turn on notifications</button>
      <button v-if="notify.on" class="ghost" :disabled="notify.testing" @click="sendTest">Send a test</button>
      <TransitionGroup name="log" tag="ul" class="log">
        <li v-for="n in notify.log" :key="n.id"><b>{{ n.title }}</b><span>{{ n.body }}</span></li>
      </TransitionGroup>
    </div>
    <InstallCard />
    <button class="danger" @click="unpair">Unpair this phone</button>
  </section>
</template>
