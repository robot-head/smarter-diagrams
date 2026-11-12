// Keep SVG elements alive during gestures; only changed ink is updated.
function key(node){
  if(node.nodeType!==1)return null;
  for(const name of ['data-node','data-label','data-callout-for','id'])if(node.hasAttribute(name))return `${node.localName}:${name}:${node.getAttribute(name)}`;
  if(node.hasAttribute('data-edge'))return `${node.localName}:edge:${node.getAttribute('data-edge')}:${node.getAttribute('data-arrowhead')||node.getAttribute('class')||''}`;
  return null;
}
function sync(current,next){
  if(current.nodeType!==1){if(current.nodeValue!==next.nodeValue)current.nodeValue=next.nodeValue;return}
  for(const attr of [...current.attributes])if(!next.hasAttribute(attr.name)&&!(attr.name==='style'&&current.localName==='svg'&&!current.ownerSVGElement))current.removeAttribute(attr.name);
  for(const attr of next.attributes)if(current.getAttribute(attr.name)!==attr.value)current.setAttributeNS(attr.namespaceURI,attr.name,attr.value);
  const children=[...current.childNodes],used=new Set(),keys=new Map();for(const child of children){const id=key(child);if(id){if(!keys.has(id))keys.set(id,[]);keys.get(id).push(child)}}let cursor=current.firstChild;
  for(const desired of next.childNodes){
    const id=key(desired);let child=id?keys.get(id)?.find(c=>!used.has(c)):children.find(c=>!used.has(c)&&!key(c)&&c.nodeType===desired.nodeType&&c.nodeName===desired.nodeName);
    if(!child){child=document.importNode(desired,true);current.insertBefore(child,cursor)}else{sync(child,desired);if(child!==cursor)current.insertBefore(child,cursor)}
    used.add(child);cursor=child.nextSibling;
  }
  for(const child of children)if(!used.has(child))child.remove();
}
export function patchSvg(canvas,markup){
  const next=new DOMParser().parseFromString(markup,'image/svg+xml').documentElement;
  if(next.localName!=='svg')throw new Error('Invalid diagram SVG');
  const current=canvas.querySelector('svg');
  if(!current){const svg=document.importNode(next,true);canvas.replaceChildren(svg);return svg}
  sync(current,next);return current;
}
