import { createApp } from 'vue'
import { i18n } from './i18n/index.js'
import App from './App.vue'
import './styles.css'

createApp(App).use(i18n).mount('#app')
