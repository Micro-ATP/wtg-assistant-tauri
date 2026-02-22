import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import en from '../locales/en.json'
import zhHans from '../locales/zh-Hans.json'
import zhHant from '../locales/zh-Hant.json'

i18n
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
      'zh-Hans': { translation: zhHans },
      'zh-Hant': { translation: zhHant },
    },
    lng: 'zh-Hans',
    fallbackLng: 'en',
    interpolation: {
      escapeValue: false,
    },
  })

export default i18n
