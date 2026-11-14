const loaderCache = new Map();

function defaultModuleUrl() {
  return (
    globalThis.SMARTER_DIAGRAMS_WASM_URL ||
    new URL('./pkg/smarter_diagrams.js', import.meta.url).href
  );
}

async function loadBindings(moduleUrl) {
  const url = moduleUrl || defaultModuleUrl();
  if (!loaderCache.has(url)) {
    loaderCache.set(
      url,
      (async () => {
        const mod = await import(/* @vite-ignore */ url);
        if (typeof mod.default === 'function') {
          await mod.default();
        }
        return mod;
      })()
    );
  }
  return loaderCache.get(url);
}

const SHADOW_STYLES = `
:host {
  display: block;
  max-width: 100%;
  --sd-paper: var(--sd-embed-paper, #f8fafc);
  --sd-ink: var(--sd-embed-ink, #0f172a);
  --sd-muted: var(--sd-embed-muted, #475569);
  --sd-accent: var(--sd-embed-accent, #0369a1);
  --sd-card: var(--sd-embed-card, #ffffff);
  --sd-zone: var(--sd-embed-zone, rgba(2, 132, 199, 0.08));
}
@media (prefers-color-scheme: dark) {
  :host(:not([data-color-scheme="light"])) {
    --sd-paper: var(--sd-embed-paper, #0f172a);
    --sd-ink: var(--sd-embed-ink, #f8fafc);
    --sd-muted: var(--sd-embed-muted, #cbd5e1);
    --sd-accent: var(--sd-embed-accent, #38bdf8);
    --sd-card: var(--sd-embed-card, #1e293b);
    --sd-zone: var(--sd-embed-zone, rgba(56, 189, 248, 0.14));
  }
}
.sd-stage {
  width: 100%;
  overflow-x: auto;
}
.sd-stage svg {
  display: block;
  width: 100%;
  height: auto;
}
.sd-error {
  margin: 0;
  padding: 0.75rem 1rem;
  border: 1px solid #ef4444;
  border-radius: 6px;
  background: rgba(239, 68, 68, 0.08);
  color: #b91c1c;
  font: 13px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  white-space: pre-wrap;
}
`;

export class SmarterDiagramElement extends HTMLElement {
  static get observedAttributes() {
    return ['src', 'theme', 'view', 'step', 'focus', 'hops', 'wasm-url'];
  }

  constructor() {
    super();
    this._renderSeq = 0;
    this._inlineSource = null;
    this._root = this.attachShadow({ mode: 'open' });
  }

  connectedCallback() {
    this._captureInlineSource();
    this.render();
  }

  disconnectedCallback() {
    this._renderSeq++;
  }

  attributeChangedCallback() {
    if (this.isConnected) {
      this.render();
    }
  }

  _captureInlineSource() {
    if (this._inlineSource !== null) return;
    const tpl = this.querySelector('script[type="text/plain"], template');
    if (tpl) {
      this._inlineSource = (tpl.textContent || '').trim();
    } else {
      this._inlineSource = (this.textContent || '').trim();
    }
  }

  async _resolveSource() {
    const srcAttr = this.getAttribute('src');
    if (srcAttr) {
      const resp = await fetch(srcAttr);
      if (!resp.ok) {
        throw new Error(`Failed to fetch diagram source '${srcAttr}' (${resp.status})`);
      }
      return await resp.text();
    }
    this._captureInlineSource();
    return this._inlineSource || '';
  }

  _showError(err) {
    const style = `<style>${SHADOW_STYLES}</style>`;
    const pre = document.createElement('pre');
    pre.className = 'sd-error';
    pre.setAttribute('role', 'alert');
    pre.textContent = err instanceof Error ? err.message : String(err);
    this._root.innerHTML = style;
    this._root.appendChild(pre);
  }

  async render() {
    const seq = ++this._renderSeq;
    try {
      const source = await this._resolveSource();
      if (seq !== this._renderSeq || !this.isConnected) return;
      if (!source.trim()) {
        this._root.innerHTML = `<style>${SHADOW_STYLES}</style><div class="sd-stage"></div>`;
        return;
      }

      const bindings = await loadBindings(this.getAttribute('wasm-url'));
      if (seq !== this._renderSeq || !this.isConnected) return;

      const theme = (this.getAttribute('theme') || '').trim();
      const step = (this.getAttribute('step') || this.getAttribute('view') || '').trim();
      const focus = (this.getAttribute('focus') || '').trim();
      const hops = Math.max(1, Number.parseInt(this.getAttribute('hops') || '1', 10) || 1);

      let svg;
      const doc = new bindings.WasmDocument(source);
      try {
        if (focus) {
          svg = doc.focus_svg(focus, hops);
        } else if (step) {
          svg = doc.step_svg(step, theme || 'auto');
        } else if (theme) {
          svg = doc.svg_themed(theme);
        } else {
          svg = doc.svg();
        }
      } finally {
        doc.free();
      }

      if (seq !== this._renderSeq || !this.isConnected) return;
      this.setAttribute('data-sd-ready', 'true');
      const normalized = svg.replace(
        /width="([^"]+)" height="([^"]+)" role="img"/,
        'width="100%" height="auto" role="img"'
      );
      this._root.innerHTML = `<style>${SHADOW_STYLES}</style><div class="sd-stage">${normalized}</div>`;
    } catch (err) {
      if (seq !== this._renderSeq || !this.isConnected) return;
      this.removeAttribute('data-sd-ready');
      this._showError(err);
    }
  }
}

export function registerSmarterDiagram(tagName = 'smarter-diagram') {
  if (!customElements.get(tagName)) {
    customElements.define(tagName, SmarterDiagramElement);
  }
}

if (typeof customElements !== 'undefined') {
  registerSmarterDiagram();
}
