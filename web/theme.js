// The guide, standalone editors and Rustdoc share the saved theme. Apply this
// before the page paints; storage may be unavailable in sandboxed embeds.
(()=>{
  const root=document.documentElement;
  const read=key=>{try{return localStorage.getItem(key)}catch{return null}};
  const save=(key,value)=>{try{localStorage.setItem(key,value)}catch{}};
  const preference=()=>read('vitepress-theme-appearance')||read('rustdoc-theme')||'auto';
  const apply=(value=preference())=>{
    const dark=value==='dark'||(value!=='light'&&matchMedia('(prefers-color-scheme: dark)').matches);
    root.dataset.theme=dark?'dark':'light';root.classList.toggle('dark',dark);
    save('rustdoc-theme',dark?'dark':'light');save('rustdoc-preferred-dark-theme','dark');save('rustdoc-preferred-light-theme','light');save('rustdoc-use-system-theme',String(value==='auto'));
    document.querySelector('#theme-toggle')?.setAttribute('aria-pressed',String(dark));
    dispatchEvent(new CustomEvent('diagram-theme-change'));
  };
  apply();
  addEventListener('storage',event=>{if(event.key==='vitepress-theme-appearance')apply()});
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change',()=>apply());
  addEventListener('DOMContentLoaded',()=>{
    const button=document.querySelector('#theme-toggle');if(!button)return;
    button.addEventListener('click',()=>{const next=root.dataset.theme==='dark'?'light':'dark';save('vitepress-theme-appearance',next);apply(next)});apply();
  });
})();
