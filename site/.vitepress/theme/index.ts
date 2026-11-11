import DefaultTheme from 'vitepress/theme'
import { withBase } from 'vitepress'
import Playground from './Playground.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('Playground', Playground)
    app.config.globalProperties.$withBase = withBase
  },
}
