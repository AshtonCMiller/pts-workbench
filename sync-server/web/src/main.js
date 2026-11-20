import { createApp } from 'vue';
import App from './App.vue';
import { reveal } from './directives/reveal.js';
import './styles.css';

createApp(App).directive('reveal', reveal).mount('#app');
