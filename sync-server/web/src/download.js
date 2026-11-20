// The download page (/download/): starts the installer downloading and says what to do next
import { createApp } from 'vue';
import DownloadPage from './pages/DownloadPage.vue';
import { reveal } from './directives/reveal.js';
import './styles.css';

createApp(DownloadPage).directive('reveal', reveal).mount('#app');
