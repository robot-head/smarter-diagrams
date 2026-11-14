<script setup>
import { withBase } from 'vitepress'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

const props = defineProps({
  mode: { type: String, default: 'coupled' },
  initialSource: { type: String, default: '' },
  title: { type: String, default: '' },
})

const DEFAULT_STATECHART = `order [label: "Order Lifecycle", context: "retries=0; max_retries=2"] {
  init [shape: initial]
  draft [label: "Draft"]
  checkout [label: "Checkout"] {
    c_init [shape: initial]
    h [shape: history]
    dh [shape: deep_history]
    review [label: "Review Order"]
    fulfilling [label: "Fulfillment", parallel: true] {
      billing [label: "Billing Region", region: true] {
        b_init [shape: initial]
        authorizing [label: "Authorizing"]
        retrying [label: "Retrying"]
        paid [label: "Captured"]
        b_init -> authorizing
        authorizing -> retrying: "decline [retries < max_retries] / retries += 1"
        retrying -> authorizing: "retry"
        authorizing -> paid: "capture"
      }
      shipping [label: "Shipping Region", region: true] {
        s_init [shape: initial]
        picking [label: "Picking"]
        packed [label: "Packed"]
        s_init -> picking
        picking -> packed: "pack"
      }
    }
    c_init -> review
    review -> fulfilling: "approve"
  }
  paused [label: "On Hold"]
  settled [shape: final]
  failed [label: "Cancelled"]

  init -> draft
  draft -> checkout: "submit"
  checkout -> paused: "hold"
  paused -> checkout.h: "resume_shallow"
  paused -> checkout.dh: "resume_deep"
  checkout.fulfilling.billing.authorizing -> failed: "decline [retries >= max_retries]"
  checkout.fulfilling -> settled: "complete"
}`

const DEFAULT_COUPLED = `api: "API Gateway" @sim(rate: 95, service_rate: 160)
workers: "Worker Pool" @sim(workers: 2, service_rate: 18, queue_cap: 25, retry: exp_backoff(0.4, 0.25))
db: "Database" [shape: database] @sim(service_rate: 80)

api -> workers: "dispatch"
workers -> db: "write"

breaker [label: "Circuit Breaker", context: "trips=0"] {
  b_init [shape: initial]
  closed [label: "Closed"]
  open [label: "Open"]
  half_open [label: "Half-Open"]
  b_init -> closed
  closed -> open: "[workers.saturation >= 0.85] / trips += 1; sever(api -> workers)"
  open -> half_open: "probe / restore(api -> workers); set_rate(api, 30)"
  half_open -> closed: "stabilized / set_rate(api, 55)"
  half_open -> open: "[workers.saturation >= 0.85] / trips += 1; sever(api -> workers)"
}`

const DEFAULT_SIMULATION = `client: "Client" @sim(rate: 85, service_rate: 200)
worker: "Worker" @sim(service_rate: 50, queue_cap: 80, workers: 1, timeout: 0.5, retry: exp_backoff(0.5, 0.2))
db: "Database" [shape: database] @sim(service_rate: 80)

client -> worker: "requests"
worker -> db: "queries"
align row [client, worker, db]`

const source = ref(
  props.initialSource.trim() ||
    (props.mode === 'statechart'
      ? DEFAULT_STATECHART
      : props.mode === 'simulation'
        ? DEFAULT_SIMULATION
        : DEFAULT_COUPLED),
)
const svg = ref('')
const snapshot = ref(null)
const events = ref([])
const status = ref('Loading WebAssembly simulation runtime…')
const error = ref('')
const playing = ref(false)
const showSource = ref(false)
const recording = ref(false)
const ingressNode = ref('')
const ingressRate = ref(80)

let rtMod = null
let chart = null
let timer = null
let recorder = null
let recordTimer = null
let recordCanvas = null
let recordedChunks = []

async function loadRuntime() {
  if (typeof window === 'undefined') return null
  if (!window.__sdRuntimePromise) {
    const rtUrl = withBase('/demo/pkg-runtime/smarter_diagrams.js')
    window.__sdRuntimePromise = import(/* @vite-ignore */ rtUrl).then(async (m) => {
      await m.default()
      return m
    })
  }
  return window.__sdRuntimePromise
}

function syncFromChart() {
  if (!chart) return
  svg.value = chart.svg()
  const snap = JSON.parse(chart.state_json())
  snapshot.value = snap
  events.value = JSON.parse(chart.events_json())
  if (snap.simulation?.traits && !ingressNode.value) {
    const entry = Object.entries(snap.simulation.traits).find(([, t]) => t.rate > 0)
    if (entry) {
      ingressNode.value = entry[0]
      ingressRate.value = Math.round(entry[1].rate)
    }
  }
  if (recording.value) paintCanvasFrame()
}

function rebuild() {
  if (!rtMod) return
  stopPlay()
  error.value = ''
  try {
    chart?.free()
    chart = new rtMod.WasmStatechart(source.value)
    ingressNode.value = ''
    syncFromChart()
    status.value = hasSim.value
      ? 'Shift-click edges to cut/restore traffic, adjust ingress rate, or trigger state transitions.'
      : 'Click any enabled event button to step the statechart and inspect history & context.'
  } catch (e) {
    error.value = String(e)
    status.value = 'Compilation error in source.'
  }
}

const hasSim = computed(() => {
  const sim = snapshot.value?.simulation
  if (!sim?.traits) return false
  return Object.values(sim.traits).some(
    (t) => t.rate > 0 || t.queue_cap < 10000 || t.service_rate < 100,
  )
})

const simRows = computed(() => {
  const sim = snapshot.value?.simulation
  if (!sim?.nodes) return []
  return Object.entries(sim.nodes)
    .filter(([id]) => {
      const q = sim.nodes[id]
      const t = sim.traits?.[id]
      return q.arrival_rate > 0 || q.queued > 0 || q.dropped > 0 || (t && t.rate > 0)
    })
    .map(([id, q]) => ({ id, ...q }))
})

function triggerEvent(name) {
  if (!chart) return
  try {
    chart.dispatch(name)
    syncFromChart()
  } catch (e) {
    error.value = String(e)
  }
}

function stepOnce() {
  if (!chart) return
  try {
    if (hasSim.value) {
      chart.step_coupled(10)
    } else {
      const enabled = events.value.find((ev) => ev.enabled)
      if (enabled) chart.dispatch(enabled.name)
    }
    syncFromChart()
  } catch (e) {
    error.value = String(e)
  }
}

function resetAll() {
  if (!chart) return
  stopPlay()
  chart.reset()
  if (ingressNode.value && snapshot.value?.simulation?.traits?.[ingressNode.value]) {
    ingressRate.value = Math.round(snapshot.value.simulation.traits[ingressNode.value].rate)
  }
  syncFromChart()
}

function togglePlay() {
  if (playing.value) {
    stopPlay()
    return
  }
  playing.value = true
  timer = setInterval(() => {
    stepOnce()
  }, hasSim.value ? 150 : 900)
}

function stopPlay() {
  playing.value = false
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

function updateIngress(ev) {
  const rate = Number(ev.target.value)
  ingressRate.value = rate
  if (chart && ingressNode.value) {
    chart.set_rate(ingressNode.value, rate)
    syncFromChart()
  }
}

function onSvgClick(ev) {
  if (!chart || !ev.shiftKey) return
  const edgeEl = ev.target?.closest?.('[data-edge]')
  const edgeId = edgeEl?.dataset?.edge
  if (!edgeId || !snapshot.value?.simulation) return
  const severed = snapshot.value.simulation.severed || []
  const next = !severed.includes(edgeId)
  chart.sever(edgeId, next)
  syncFromChart()
}

function paintCanvasFrame() {
  if (!recordCanvas || !svg.value) return
  const ctx = recordCanvas.getContext('2d')
  if (!ctx) return
  const blob = new Blob([svg.value], { type: 'image/svg+xml;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const img = new Image()
  img.onload = () => {
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(0, 0, recordCanvas.width, recordCanvas.height)
    ctx.drawImage(img, 0, 0, recordCanvas.width, recordCanvas.height)
    URL.revokeObjectURL(url)
  }
  img.onerror = () => URL.revokeObjectURL(url)
  img.src = url
}

function toggleWebm() {
  if (recording.value) {
    stopWebm()
    return
  }
  if (typeof MediaRecorder === 'undefined') {
    status.value = 'MediaRecorder is unavailable in this browser; use --format gif or --format apng via CLI.'
    return
  }
  const parser = new DOMParser()
  const doc = parser.parseFromString(svg.value, 'image/svg+xml')
  const svgEl = doc.documentElement
  const wAttr = Number(svgEl.getAttribute('width')) || 800
  const hAttr = Number(svgEl.getAttribute('height')) || 450
  const scale = Math.min(2, 960 / Math.max(1, wAttr))
  recordCanvas = document.createElement('canvas')
  recordCanvas.width = Math.max(320, Math.round(wAttr * scale))
  recordCanvas.height = Math.max(180, Math.round(hAttr * scale))
  if (typeof recordCanvas.captureStream !== 'function') {
    status.value = 'HTMLCanvasElement.captureStream() is unavailable in this browser.'
    return
  }
  const mime = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm'].find((m) =>
    MediaRecorder.isTypeSupported(m),
  )
  recordedChunks = []
  const stream = recordCanvas.captureStream(10)
  recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined)
  recorder.ondataavailable = (e) => {
    if (e.data && e.data.size > 0) recordedChunks.push(e.data)
  }
  recorder.onstop = () => {
    const outBlob = new Blob(recordedChunks, { type: mime || 'video/webm' })
    const dlUrl = URL.createObjectURL(outBlob)
    const a = document.createElement('a')
    a.href = dlUrl
    a.download = `smarter-diagrams-${props.mode}.webm`
    document.body.appendChild(a)
    a.click()
    a.remove()
    setTimeout(() => URL.revokeObjectURL(dlUrl), 1000)
    status.value = 'Saved WebM recording.'
  }
  recording.value = true
  paintCanvasFrame()
  recorder.start(100)
  status.value = 'Recording WebM… click Stop WebM to download.'
  if (!playing.value) togglePlay()
  recordTimer = setTimeout(() => {
    if (recording.value) stopWebm()
  }, 4000)
}

function stopWebm() {
  if (recordTimer) {
    clearTimeout(recordTimer)
    recordTimer = null
  }
  recording.value = false
  stopPlay()
  if (recorder && recorder.state !== 'inactive') {
    recorder.stop()
  }
}

onMounted(async () => {
  try {
    rtMod = await loadRuntime()
    rebuild()
  } catch (e) {
    error.value = `Failed to load Wasm runtime: ${String(e)}`
    status.value = 'Runtime unavailable.'
  }
})

onBeforeUnmount(() => {
  stopWebm()
  stopPlay()
  chart?.free()
})
</script>

<template>
  <section class="sd-sim-embed">
    <div v-if="title" class="sd-sim-title">{{ title }}</div>
    <div class="sd-sim-toolbar">
      <button type="button" class="sd-btn sd-btn-primary" @click="togglePlay">
        {{ playing ? 'Pause' : 'Play' }}
      </button>
      <button type="button" class="sd-btn" @click="stepOnce">Step</button>
      <button type="button" class="sd-btn" @click="resetAll">Reset</button>
      <label v-if="hasSim && ingressNode" class="sd-slider-label">
        Ingress ({{ ingressNode }}): <strong>{{ ingressRate }}/s</strong>
        <input type="range" min="0" max="220" :value="ingressRate" @input="updateIngress" />
      </label>
      <button type="button" class="sd-btn" @click="toggleWebm">
        {{ recording ? 'Stop WebM' : 'Export WebM' }}
      </button>
      <button type="button" class="sd-btn" @click="showSource = !showSource">
        {{ showSource ? 'Hide source' : 'Edit source' }}
      </button>
    </div>

    <div v-if="showSource" class="sd-source-box">
      <textarea v-model="source" rows="10" spellcheck="false" aria-label="Executable diagram source" />
      <div class="sd-source-actions">
        <button type="button" class="sd-btn sd-btn-primary" @click="rebuild">Apply source</button>
      </div>
    </div>

    <p v-if="error" class="sd-sim-error" role="alert">{{ error }}</p>

    <div v-if="snapshot" class="sd-sim-meta">
      <div class="sd-meta-row">
        <span class="sd-meta-label">Active:</span>
        <span v-for="leaf in snapshot.leaf_states" :key="leaf" class="sd-pill sd-pill-active">
          {{ leaf }}
        </span>
      </div>
      <div v-if="Object.keys(snapshot.context || {}).length" class="sd-meta-row">
        <span class="sd-meta-label">Context:</span>
        <span v-for="(val, k) in snapshot.context" :key="k" class="sd-pill">
          {{ k }}={{ val }}
        </span>
      </div>
      <div v-if="events.length" class="sd-meta-row">
        <span class="sd-meta-label">Events:</span>
        <button
          v-for="ev in events"
          :key="`${ev.source}-${ev.name}-${ev.target}`"
          type="button"
          class="sd-ev-btn"
          :disabled="!ev.enabled"
          :title="ev.guard ? `Guard: [${ev.guard}]` : `Trigger ${ev.name}`"
          @click="triggerEvent(ev.name)"
        >
          {{ ev.name }}
          <span v-if="ev.guard" class="sd-guard">[{{ ev.guard }}]</span>
        </button>
      </div>
    </div>

    <div class="sd-sim-canvas" v-html="svg" @click="onSvgClick" />

    <div class="sd-sim-footer">
      <p class="sd-sim-status">{{ status }}</p>
      <div v-if="simRows.length" class="sd-sim-metrics">
        <div v-for="row in simRows" :key="row.id" class="sd-metric-card">
          <strong>{{ row.id }}</strong>
          <span>in {{ row.arrival_rate.toFixed(1) }}/s</span>
          <span>out {{ row.throughput.toFixed(1) }}/s</span>
          <span>q {{ row.queued.toFixed(1) }}</span>
          <span :class="{ 'sd-hot': row.saturation >= 0.85 }">
            sat {{ (row.saturation * 100).toFixed(0) }}%
          </span>
          <span v-if="row.dropped > 0.05" class="sd-hot">
            drop {{ row.dropped.toFixed(1) }}
          </span>
        </div>
      </div>
      <div v-if="snapshot?.history_log?.length" class="sd-sim-log">
        <div
          v-for="item in snapshot.history_log.slice(-4).reverse()"
          :key="`${item.step}-${item.edge_id}`"
          class="sd-log-entry"
        >
          #{{ item.step }} <code>{{ item.event }}</code>: {{ item.source }} → {{ item.target }}
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.sd-sim-embed {
  margin: 18px 0 24px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 10px;
  padding: 14px;
  background: var(--vp-c-bg-soft);
}
.sd-sim-title {
  font-weight: 600;
  margin-bottom: 10px;
}
.sd-sim-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 10px;
}
.sd-btn {
  padding: 5px 11px;
  border-radius: 6px;
  border: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font-size: 13px;
  cursor: pointer;
}
.sd-btn-primary {
  border-color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  font-weight: 600;
}
.sd-slider-label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  margin: 0 4px;
}
.sd-source-box textarea {
  width: 100%;
  font-family: ui-monospace, monospace;
  font-size: 12.5px;
  line-height: 1.5;
  padding: 10px;
  border-radius: 6px;
  border: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
}
.sd-source-actions {
  margin: 6px 0 10px;
}
.sd-sim-error {
  color: #dc2626;
  font-family: ui-monospace, monospace;
  font-size: 12.5px;
  white-space: pre-wrap;
}
.sd-sim-meta {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 10px;
}
.sd-meta-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}
.sd-meta-label {
  font-size: 12px;
  color: var(--vp-c-text-2);
  font-weight: 600;
}
.sd-pill {
  font-family: ui-monospace, monospace;
  font-size: 12px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--vp-c-bg);
  border: 1px solid var(--vp-c-divider);
}
.sd-pill-active {
  border-color: #16a34a;
  color: #15803d;
  background: rgba(22, 163, 74, 0.1);
}
.sd-ev-btn {
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--vp-c-brand-1);
  background: var(--vp-c-bg);
  font-size: 12.5px;
  cursor: pointer;
}
.sd-ev-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
  border-color: var(--vp-c-divider);
}
.sd-guard {
  color: var(--vp-c-text-2);
  font-family: ui-monospace, monospace;
  font-size: 11px;
  margin-left: 4px;
}
.sd-sim-canvas {
  background: #ffffff;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  padding: 10px;
  overflow: auto;
}
.sd-sim-canvas :deep(svg) {
  max-width: 100%;
  height: auto;
  display: block;
  margin: 0 auto;
}
.sd-sim-status {
  margin: 8px 0 4px;
  font-size: 12.5px;
  color: var(--vp-c-text-2);
}
.sd-sim-metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 6px;
}
.sd-metric-card {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  padding: 4px 9px;
  border-radius: 6px;
  background: var(--vp-c-bg);
  border: 1px solid var(--vp-c-divider);
  font-family: ui-monospace, monospace;
  font-size: 12px;
}
.sd-hot {
  color: #dc2626;
  font-weight: 600;
}
.sd-sim-log {
  margin-top: 8px;
  font-size: 12px;
  color: var(--vp-c-text-2);
}
</style>
