import { createI18n } from 'vue-i18n'
import en from './en.js'
import it from './it.js'

const systemLang = navigator.language?.toLowerCase() ?? 'en'
const locale = systemLang.startsWith('it') ? 'it' : 'en'

export const i18n = createI18n({
    legacy: false,
    locale,
    fallbackLocale: 'en',
    messages: { en, it },
})
