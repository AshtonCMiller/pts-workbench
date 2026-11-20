<script setup>
import TopBar from './components/TopBar.vue';
import Hero from './components/Hero.vue';
import Problem from './components/Problem.vue';
import Features from './components/Features.vue';
import JobFlow from './components/JobFlow.vue';
import PhoneSection from './components/PhoneSection.vue';
import Roadmap from './components/Roadmap.vue';
import FinalCta from './components/FinalCta.vue';
import SiteFooter from './components/SiteFooter.vue';
</script>

<template>
  <TopBar />
  <main>
    <Hero />
    <Problem />
    <Features />
    <JobFlow />
    <PhoneSection />
    <Roadmap />
    <FinalCta />
  </main>
  <SiteFooter />
</template>
