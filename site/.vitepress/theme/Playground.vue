<script setup>
import { withBase } from 'vitepress'
</script>

<template>
  <div class="playground-frame">
    <iframe :src="withBase('/demo/')" title="Interactive Smarter Diagrams editor" allow="clipboard-write" />
  </div>
  <p class="playground-note"><a :href="withBase('/demo/')" target="_blank" rel="noopener">Open the editor in a full window ↗</a></p>
</template>
