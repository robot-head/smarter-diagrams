<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, useId } from 'vue'
import { withBase, useData } from 'vitepress'
import {touchGestures} from '../../../web/touch-gestures.js'

type NodeRow = { id: string; label: string; shape: string; parent: string | null; bounds: { x:number; y:number; w:number; h:number }; ports: { id:string; label:string; constraint:unknown }[] }
type Example = { file:string; darkFile:string; label:string; description:string; viewBox:number[]; grid:number; nodes:NodeRow[]; edges:{ id:string; source:string; target:string; label:string; kind:string }[]; zones:{label:string;members:string[]}[]; flows:{label:string;steps:string[]}[] }
const props = defineProps<{ examples:Example[]; title:string; guides?:boolean }>()
const { isDark } = useData()
const fitted = ref(true), readable = ref(true), coarse = ref(false)
const active = ref(0), zoom = ref(1), showGrid = ref(false), viewport = ref<HTMLElement>(), viewportWidth = ref(640)
const id = useId(), current = computed(() => props.examples[active.value])
const bounds = computed(() => current.value.viewBox)
const imageStyle = computed(() => ({ width:`${bounds.value[2]*zoom.value}px`, height:`${bounds.value[3]*zoom.value}px` }))
const fitScale = computed(() => Math.max(readable.value ? coarse.value ? .55 : .7 : .05, Math.min(1, viewportWidth.value / bounds.value[2], 520 / bounds.value[3])))
const percent = computed(() => `${Math.round(zoom.value*100)}%`)
const gridLines = computed(() => {
  const [x,y,w,h]=bounds.value, grid=current.value.grid
  const xs:number[]=[],ys:number[]=[]
  for(let n=Math.ceil(x/grid)*grid;n<=x+w;n+=grid) xs.push(n)
  for(let n=Math.ceil(y/grid)*grid;n<=y+h;n+=grid) ys.push(n)
  return {xs,ys}
})
let observer:ResizeObserver | undefined
onMounted(() => { if(viewport.value){coarse.value=matchMedia('(pointer: coarse)').matches;viewportWidth.value=viewport.value.clientWidth;zoom.value=fitScale.value;touchGestures(viewport.value,{getZoom:()=>zoom.value,setZoom});observer=new ResizeObserver(entries=>{viewportWidth.value=entries[0].contentRect.width;if(fitted.value)zoom.value=fitScale.value});observer.observe(viewport.value)} })
onUnmounted(() => observer?.disconnect())
function setZoom(value:number){ fitted.value=false;zoom.value=Math.max(0.05,Math.min(4,value)) }
function fit(){readable.value=false;fitted.value=true;zoom.value=fitScale.value}
function wheel(event:WheelEvent){if(!event.ctrlKey&&!event.metaKey)return;event.preventDefault();setZoom(zoom.value*Math.exp(-Math.max(-100,Math.min(100,event.deltaY))*.005))}
async function choose(index:number){ active.value=index;await nextTick();if(fitted.value)zoom.value=fitScale.value }
function tabs(event:KeyboardEvent,index:number){
  const keys=['ArrowLeft','ArrowRight','Home','End'];if(!keys.includes(event.key))return
  event.preventDefault();const n=props.examples.length
  const next=event.key==='Home'?0:event.key==='End'?n-1:(index+(event.key==='ArrowRight'?1:-1)+n)%n
  choose(next);document.getElementById(`${id}-tab-${next}`)?.focus()
}
const units = (value:number) => Number((value/current.value.grid).toFixed(2))
</script>

<template>
  <div class="diagram-examples" :data-example-count="examples.length">
    <div v-if="examples.length>1" class="diagram-tabs" role="tablist" :aria-label="`${title} comparisons`">
      <button v-for="(example,index) in examples" :id="`${id}-tab-${index}`" :key="example.file" role="tab" :aria-selected="active===index" :aria-controls="`${id}-panel`" :tabindex="active===index?0:-1" @click="choose(index)" @keydown="tabs($event,index)">{{ example.label }}</button>
    </div>
    <figure :id="`${id}-panel`" class="diagram-example" :role="examples.length>1?'tabpanel':undefined" :aria-labelledby="examples.length>1?`${id}-tab-${active}`:undefined">
      <div class="diagram-controls" role="group" :aria-label="`${title} viewing controls`">
        <button aria-label="Zoom out" @click="setZoom(zoom/1.25)">−</button>
        <button aria-label="Actual size (100%)" @click="setZoom(1)"><output aria-live="polite" aria-label="Diagram scale">{{ percent }}</output></button>
        <button aria-label="Zoom in" @click="setZoom(zoom*1.25)">+</button>
        <button @click="fit">Fit</button>
        <label v-if="guides"><input v-model="showGrid" type="checkbox"> Show grid</label>
        <span class="diagram-pan-hint">{{ coarse ? 'Pinch to zoom · drag to pan' : 'Ctrl/⌘ + scroll to zoom · swipe to pan' }}</span>
        <span v-if="bounds[2]*zoom > viewportWidth+2" class="diagram-overflow-hint">Pan for more · Fit shows all</span>
      </div>
      <div ref="viewport" class="diagram-viewport" @wheel="wheel" tabindex="0" :aria-label="`${title} diagram. Ctrl or Command plus scroll to zoom; swipe or use arrow keys to pan.`">
        <div class="diagram-stage" :style="imageStyle">
          <img :src="withBase(`/generated/language/${isDark ? current.darkFile : current.file}`)" loading="lazy" :alt="`${title} — ${current.label}. ${current.nodes.length} nodes and ${current.edges.length} connections. Details follow in Diagram description.`" :aria-describedby="`${id}-description`">
          <svg v-if="showGrid" class="diagram-grid" :viewBox="bounds.join(' ')" aria-hidden="true">
            <g stroke="#147467" stroke-opacity=".2" stroke-width=".6">
              <line v-for="x in gridLines.xs" :key="`x${x}`" :x1="x" :x2="x" :y1="bounds[1]" :y2="bounds[1]+bounds[3]"/>
              <line v-for="y in gridLines.ys" :key="`y${y}`" :x1="bounds[0]" :x2="bounds[0]+bounds[2]" :y1="y" :y2="y"/>
            </g>
            <g stroke="#08695d" stroke-opacity=".55" stroke-width="1"><line x1="0" x2="0" :y1="bounds[1]" :y2="bounds[1]+bounds[3]"/><line :x1="bounds[0]" :x2="bounds[0]+bounds[2]" y1="0" y2="0"/></g>
          </svg>
        </div>
      </div>
      <figcaption>{{ current.label }} · <a :href="withBase(`/generated/language/${isDark ? current.darkFile : current.file}`)" target="_blank" rel="noopener" :aria-label="`Open SVG: ${title} — ${current.label}`">Open SVG ↗</a><span v-if="examples.length>1"> · Comparisons keep the same scale and frame.</span></figcaption>
      <details :id="`${id}-description`" class="diagram-description">
        <summary>Diagram description · {{ current.nodes.length }} nodes, {{ current.edges.length }} connections</summary>
        <p>{{ current.description }}</p>
        <table><caption>Nodes and ownership</caption><thead><tr><th scope="col">Node</th><th scope="col">Label</th><th scope="col">Shape</th><th scope="col">Parent</th><th scope="col">Ports</th></tr></thead><tbody><tr v-for="node in current.nodes" :key="node.id"><th scope="row">{{node.id}}</th><td>{{node.label}}</td><td>{{node.shape}}</td><td>{{node.parent || '—'}}</td><td>{{node.ports.map(p=>`${p.id}: ${p.label}`).join('; ') || '—'}}</td></tr></tbody></table>
        <table v-if="current.edges.length"><caption>Connections</caption><thead><tr><th scope="col">From</th><th scope="col">To</th><th scope="col">Direction</th><th scope="col">Label</th></tr></thead><tbody><tr v-for="edge in current.edges" :key="edge.id"><td>{{edge.source}}</td><td>{{edge.target}}</td><td>{{edge.kind}}</td><td>{{edge.label || '—'}}</td></tr></tbody></table>
        <p v-for="zone in current.zones" :key="zone.label">Zone {{zone.label}}: {{zone.members.join(', ')}}</p>
        <p v-for="flow in current.flows" :key="flow.label">Flow {{flow.label}}: {{flow.steps.join(' → ')}}</p>
      </details>
      <div v-if="showGrid" class="diagram-coordinates"><p>Grid spacing: {{current.grid}} px. Stronger lines mark zero on each axis. Coordinates describe node centers; coordinates and dimensions are in grid units.</p><table><caption>Grid coordinates and dimensions</caption><thead><tr><th scope="col">Node</th><th scope="col">X</th><th scope="col">Y</th><th scope="col">Width</th><th scope="col">Height</th></tr></thead><tbody><tr v-for="node in current.nodes" :key="node.id"><th scope="row">{{node.id}}</th><td>{{units(node.bounds.x)}}</td><td>{{units(node.bounds.y)}}</td><td>{{units(node.bounds.w)}}</td><td>{{units(node.bounds.h)}}</td></tr></tbody></table></div>
    </figure>
  </div>
</template>
