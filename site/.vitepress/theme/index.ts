import DefaultTheme from 'vitepress/theme'
import { withBase } from 'vitepress'
import Playground from './Playground.vue'
import DiagramExamples from './DiagramExamples.vue'
import ThemeBridge from './ThemeBridge.vue'
import { h } from 'vue'
import '../../../web/diagram-palette.css'
import './custom.css'

export default {
  extends: DefaultTheme,
  Layout: () => h(DefaultTheme.Layout, null, { 'layout-top': () => h(ThemeBridge) }),
  enhanceApp({ app }) {
    app.component('Playground', Playground)
    app.component('DiagramExamples', DiagramExamples)
    app.config.globalProperties.$withBase = withBase
  },
}
