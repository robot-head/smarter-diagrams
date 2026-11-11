<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, watch } from 'vue'
import { useData, useRoute } from 'vitepress'
const { isDark } = useData()
const route=useRoute()
let observer:MutationObserver | undefined
function labels(){document.querySelectorAll<HTMLButtonElement>('.VPSwitchAppearance').forEach(button=>button.setAttribute('aria-label',button.title || (isDark.value?'Switch to light theme':'Switch to dark theme')))}
function sync(){
  const theme=isDark.value?'dark':'light'
  document.documentElement.dataset.theme=theme
  try{localStorage.setItem('rustdoc-theme',theme);localStorage.setItem('rustdoc-preferred-dark-theme','dark');localStorage.setItem('rustdoc-preferred-light-theme','light');localStorage.setItem('rustdoc-use-system-theme','false')}catch{}
  labels()
}
function copies(){
  document.querySelectorAll<HTMLPreElement>('.vp-doc pre').forEach(pre=>{
    const parent=pre.parentElement!;
    const existing=parent.querySelector<HTMLButtonElement>(':scope > button.copy');
    if(existing){existing.setAttribute('aria-label','Copy code');return}
    if(parent.querySelector(':scope > .sd-copy'))return;
    parent.classList.add('sd-copy-host');const button=document.createElement('button');button.className='sd-copy';button.textContent='Copy';button.setAttribute('aria-label','Copy code');button.setAttribute('aria-live','polite');
    button.onclick=async()=>{try{await navigator.clipboard.writeText(pre.textContent||'');button.textContent='✓ Copied';setTimeout(()=>button.textContent='Copy',1500)}catch{button.textContent='Copy unavailable'}};parent.prepend(button);
  })
}
function storage(event:StorageEvent){if(event.key==='rustdoc-theme'&&(event.newValue==='light'||event.newValue==='dark'))isDark.value=event.newValue==='dark'}
onMounted(()=>{sync();copies();addEventListener('storage',storage);observer=new MutationObserver(labels);const nav=document.querySelector('.VPNav');if(nav)observer.observe(nav,{subtree:true,childList:true,attributes:true,attributeFilter:['title']})})
watch(isDark,sync)
watch(()=>route.path,async()=>{await nextTick();copies()})
onUnmounted(()=>{observer?.disconnect();removeEventListener('storage',storage)})
</script>
<template><span hidden aria-hidden="true"></span></template>
