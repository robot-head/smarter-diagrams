<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useData } from 'vitepress'
const { isDark } = useData()
let observer:MutationObserver | undefined
function labels(){document.querySelectorAll<HTMLButtonElement>('.VPSwitchAppearance').forEach(button=>button.setAttribute('aria-label',button.title || (isDark.value?'Switch to light theme':'Switch to dark theme')))}
function sync(){
  const theme=isDark.value?'dark':'light'
  document.documentElement.dataset.theme=theme
  try{localStorage.setItem('rustdoc-theme',theme);localStorage.setItem('rustdoc-preferred-dark-theme','dark');localStorage.setItem('rustdoc-preferred-light-theme','light');localStorage.setItem('rustdoc-use-system-theme','false')}catch{}
  labels()
}
onMounted(()=>{sync();observer=new MutationObserver(labels);const nav=document.querySelector('.VPNav');if(nav)observer.observe(nav,{subtree:true,childList:true,attributes:true,attributeFilter:['title']})})
watch(isDark,sync)
onUnmounted(()=>observer?.disconnect())
</script>
<template><span hidden aria-hidden="true"></span></template>
