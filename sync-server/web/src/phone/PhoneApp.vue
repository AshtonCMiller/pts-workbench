<script setup>
// The frame: the header, the view showing (sliding in, from the other side when going back), the tab bar with its
// highlight sliding to the tab, and the toast
import { computed, onMounted, watchEffect } from 'vue';
import { ui, TABS, show, start } from './state.js';
import Icon from './parts/Icon.vue';
import DesktopView from './views/DesktopView.vue';
import InstallView from './views/InstallView.vue';
import PairView from './views/PairView.vue';
import ScanView from './views/ScanView.vue';
import TypeView from './views/TypeView.vue';
import RosView from './views/RosView.vue';
import HoursView from './views/HoursView.vue';
import HomeView from './views/HomeView.vue';
import SheetView from './views/SheetView.vue';

const VIEWS = { desktop: DesktopView, install: InstallView, pair: PairView, scan: ScanView, type: TypeView, ros: RosView, hours: HoursView, home: HomeView, sheet: SheetView };
const TAB_BUTTONS = [
  { tab: 'ros', label: 'ROs', icon: 'ro' },
  { tab: 'hours', label: 'Hours', icon: 'clock' },
  { tab: 'home', label: 'Phone', icon: 'phone' },
];

const tab = computed(() => TABS.indexOf(ui.view));
watchEffect(() => document.body.classList.toggle('tabbed', tab.value >= 0));

onMounted(start);
</script>

<template>
  <div class="wrap">
    <header>
      <img src="./assets/icon.png" alt="">
      <div>
        <h1>Workbench</h1>
        <p>On your phone</p>
      </div>
    </header>
    <Transition :name="ui.back ? 'view-back' : 'view'" mode="out-in">
      <component :is="VIEWS[ui.view]" v-if="ui.view" :key="ui.view" />
    </Transition>
  </div>

  <nav class="tabs">
    <i class="glow" :style="{ transform: `translateX(${Math.max(tab, 0) * 86}px)` }" />
    <button v-for="b in TAB_BUTTONS" :key="b.tab" :class="{ on: ui.view === b.tab }" @click="show(b.tab)">
      <Icon :name="b.icon" />{{ b.label }}
    </button>
  </nav>

  <!-- only there while it's showing: it springs up, then sinks away -->
  <Transition name="toast">
    <div v-if="ui.toast.on" class="toast" :class="{ bad: ui.toast.bad }" role="status">{{ ui.toast.text }}</div>
  </Transition>
</template>
