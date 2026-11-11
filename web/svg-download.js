// Snapshot the active palette and include its surface for standalone viewers.
export function standaloneSvg(original){
  const svg=original.cloneNode(true),style=getComputedStyle(original),[x,y,width,height]=original.getAttribute('viewBox').split(/\s+/).map(Number),box={x,y,width,height};
  for(const name of ['width','height','max-width','max-height','margin'])svg.style.removeProperty(name);
  const background=document.createElementNS(svg.namespaceURI,'rect');
  for(const [name,value]of Object.entries({x:box.x,y:box.y,width:box.width,height:box.height,fill:style.getPropertyValue('--sd-bg').trim()||'#ffffff'}))background.setAttribute(name,String(value));
  background.dataset.exportBackground='true';svg.prepend(background);
  const resolve=value=>value.replace(/var\((--sd-[\w-]+),\s*([^)]*)\)/g,(_,name,fallback)=>style.getPropertyValue(name).trim()||fallback);
  const paints=new Set(['style','fill','stroke','color','stop-color','flood-color','lighting-color']);
  for(const element of [svg,...svg.querySelectorAll('*')]){
    for(const attribute of [...element.attributes])if(paints.has(attribute.name))element.setAttribute(attribute.name,resolve(attribute.value));
    if(element.localName==='style')element.textContent=resolve(element.textContent);
  }
  return new XMLSerializer().serializeToString(svg);
}
