import { defineConfig } from 'vitepress'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
const diagramSamples = createRequire(import.meta.url)('./diagram-samples.cjs')

// These are downloadable assets, not routes with an implicit .html suffix.
process.env.VITE_EXTRA_EXTENSIONS = 'diagram,wasm'

const guide = [
  ['Getting started', 'getting-started'], ['Core concepts', 'concepts'],
  ['Using Rust', 'rust'], ['Structured editing', 'editing'],
  ['CLI and export', 'cli'], ['WebAssembly', 'wasm'],
  ['Editor previews', 'editors'], ['MCP tools', 'mcp'],
].map(([text, page]) => ({ text, link: `/guide/${page}` }))
const language = [
  ['Nodes and connections', 'basics'], ['Ports and records', 'ports'],
  ['Layout constraints', 'layout'], ['Zones, flows and views', 'overlays'],
  ['Grammar reference', 'reference'],
].map(([text, page]) => ({ text, link: `/language/${page}` }))
const runtime = [
  ['Views, zoom and lenses', 'views'], ['Queue simulation', 'simulation'],
  ['Telemetry and code drift', 'drift'], ['Bounded verification', 'verification'],
  ['Strata and Spatial Git', 'spatial'],
].map(([text, page]) => ({ text, link: `/runtime/${page}` }))
const reference = [
  ['Module map', 'modules'], ['Feature flags', 'features'],
  ['Errors and troubleshooting', 'errors'], ['Architecture', 'architecture'],
  ['Performance', 'performance'], ['Validation evidence', 'verification'],
].map(([text, page]) => ({ text, link: `/reference/${page}` }))

export default defineConfig({
  title: 'Smarter Diagrams',
  description: 'A Rust library for text-authored diagrams, lossless editing and stable orthogonal layout.',
  base: '/smarter-diagrams/',
  srcExclude: ['public/**'],
  vite: { define: { 'import.meta.env.VITE_EXTRA_EXTENSIONS': JSON.stringify('diagram,wasm') } },
  cleanUrls: false,
  lastUpdated: true,
  sitemap: { hostname: 'https://robot-head.github.io/smarter-diagrams/' },
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/smarter-diagrams/logo.svg' }],
    ['link', { rel: 'alternate icon', href: '/smarter-diagrams/favicon.ico' }],
    ['script', { src: '/smarter-diagrams/demo/theme.js' }],
  ],
  markdown: {
    config(md) {
      md.renderer.rules.table_open = () => '<div class="table-scroll" role="region" aria-label="Table, scroll horizontally for more columns" tabindex="0"><table>'
      md.renderer.rules.table_close = () => '</table></div>'
      const fence = md.renderer.rules.fence!
      const manifestPath = new URL('../public/generated/language/manifest.json', import.meta.url)
      md.renderer.rules.fence = (tokens, index, options, env, renderer) => {
        const code = fence(tokens, index, options, env, renderer)
        const token = tokens[index]
        if (!(env.relativePath?.startsWith('language/') || env.relativePath?.startsWith('guide/') || env.relativePath === 'index.md' || env.relativePath === 'examples.md') || !/^diagram(?:\s|$)/.test(token.info)) return code
        const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
        let heading = 'Language example'
        for (let i = index - 1; i >= 0; i--) {
          if (tokens[i].type === 'heading_open') { heading = tokens[i + 1].content; break }
        }
        const escape = md.utils.escapeHtml
        const examples = diagramSamples.samples(token.content, token.info).map(sample => {
          const data = manifest[diagramSamples.key(sample)]
          if (!data) throw new Error(`Missing rendered example in ${env.relativePath}; run npm run docs:build`)
          return { ...data, label: sample.label }
        })
        if (examples[0].diagnostic) return `${code}<div class="diagram-diagnostic" role="note"><strong>Expected layout diagnostic</strong><pre>${escape(examples[0].diagnostic)}</pre></div>`
        return `${code}<DiagramExamples :examples="${escape(JSON.stringify(examples))}" title="${escape(heading)}" :guides="${/\bguides\b/.test(token.info)}" />`

      }
      const open = md.renderer.rules.link_open!
      md.renderer.rules.link_open = (tokens, index, ...args) => {
        // Demo HTML is served as a static application, outside the docs router.
        const demo = tokens[index].attrGet('href')?.includes('/demo/')
        const rendered = open(tokens, index, ...args)
        if (!demo) return rendered
        tokens[index].attrSet('target', '_self')
        return args[2].renderToken(tokens, index, args[0])
      }
    },
    languages: [{
      name: 'diagram', scopeName: 'source.diagram', patterns: [
        { match: '(^|\\s)#.*$', name: 'comment.line.number-sign' },
        { match: '"(?:[^"\\\\]|\\\\.)*"', name: 'string.quoted.double' },
        { match: '\\b(port|zone|around|align|distribute|row|col|flow|view|include|strata|invariant)\\b', name: 'keyword.control' },
        { match: '@[a-zA-Z_][a-zA-Z_0-9]*', name: 'entity.name.function' },
        { match: '<->|==>|->|--', name: 'keyword.operator' },
        { match: '\\b\\d+(?:\\.\\d+)?g?\\b', name: 'constant.numeric' },
        { match: '\\b(N|E|S|W)\\b', name: 'constant.language' },
      ],
    }],
  },
  themeConfig: {
    logo: '/logo.svg',
    siteTitle: 'Smarter Diagrams',
    nav: [
      { text: 'Guide', link: '/guide/getting-started', activeMatch: '/guide/' },
      { text: 'Language', link: '/language/basics', activeMatch: '/language/' },
      { text: 'Playground', link: '/playground' },
      { text: 'Rust API', link: 'https://robot-head.github.io/smarter-diagrams/api/smarter_diagrams/index.html', target: '_blank', rel: 'noopener' },
    ],
    sidebar: [
      { text: 'Start here', items: [{ text: 'Overview', link: '/' }, { text: 'Playground', link: '/playground' }, { text: 'Examples', link: '/examples' }] },
      { text: 'Build with the library', collapsed: false, items: guide },
      { text: 'Diagram language', collapsed: false, items: language },
      { text: 'Executable diagrams', collapsed: true, items: runtime },
      { text: 'Reference', collapsed: true, items: reference },
      { text: 'Project', items: [{ text: 'Contributing and publishing', link: '/contributing' }] },
    ],
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/robot-head/smarter-diagrams', ariaLabel: 'GitHub repository' }],
    editLink: { pattern: 'https://github.com/robot-head/smarter-diagrams/edit/main/site/:path', text: 'Edit this page on GitHub' },
    outline: [2, 3],
    footer: { message: 'Rust · WebAssembly · MIT licensed', copyright: 'Smarter Diagrams contributors' },
  },
})
