import DefaultTheme from 'vitepress/theme'
import { withBase } from 'vitepress'
import Playground from './Playground.vue'
import DiagramExamples from './DiagramExamples.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('Playground', Playground)
    app.component('DiagramExamples', DiagramExamples)
    app.config.globalProperties.$withBase = withBase
  },
}
