<script setup>
import { withBase } from 'vitepress'
import { onMounted, onUnmounted, ref } from 'vue'
const frame = ref()
const height = ref(960)
function resize(event) {
  if (event.origin !== location.origin || event.source !== frame.value?.contentWindow || event.data?.type !== 'smarter-diagrams:resize') return
  const next = event.data.height
  if (Number.isFinite(next) && next >= 320 && next <= 20000) height.value = Math.ceil(next)
}
onMounted(() => addEventListener('message', resize))
onUnmounted(() => removeEventListener('message', resize))
</script>

<template>
  <div class="playground-frame">
    <iframe ref="frame" :src="withBase('/demo/')" :style="{height: `${height}px`}" title="Interactive Smarter Diagrams editor" allow="clipboard-write" />
  </div>
  <p class="playground-note"><a :href="withBase('/demo/')" target="_blank" rel="noopener">Open the editor in a full window ↗</a></p>
</template>
