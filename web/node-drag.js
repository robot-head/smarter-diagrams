// Preview from the immutable gesture baseline; commit once on pointer release.
// Capture belongs to the canvas, which survives replacement of the SVG children.
export class NodeDragController {
  constructor({canvas,view,status,getDocument,touchEnabled,beforeStart,show,onCommit}) {
    Object.assign(this,{canvas,view,status,getDocument,touchEnabled,beforeStart,show,onCommit});
    canvas.addEventListener('pointerdown',event=>this.start(event));
    canvas.addEventListener('pointermove',event=>this.move(event));
    canvas.addEventListener('pointerup',event=>this.finish(event));
    for(const type of ['pointercancel','lostpointercapture'])canvas.addEventListener(type,event=>{if(event.pointerId===this.gesture?.pointer)this.cancel()});
    canvas.addEventListener('dragstart',event=>event.preventDefault());
    window.addEventListener('keydown',event=>{if(this.active&&event.key==='Escape'){event.preventDefault();this.cancel()}});
    window.addEventListener('blur',()=>this.cancel());
  }
  get active(){return !!this.gesture}
  point(event){return new DOMPoint(event.clientX,event.clientY).matrixTransform(this.canvas.querySelector('svg').getScreenCTM().inverse())}
  start(event){
    if(this.active||event.button!==0||!event.isPrimary||(event.pointerType==='touch'&&!this.touchEnabled()))return;
    const hit=event.target.closest('[data-node],[data-owner-node]'),id=hit?.dataset.node||hit?.dataset.ownerNode;
    if(!id||this.beforeStart()===false)return;
    const node=[...this.canvas.querySelectorAll('[data-node]')].find(node=>node.dataset.node===id),body=node?.querySelector('[data-silhouette]');
    if(!body)return;
    event.preventDefault();this.canvas.focus({preventScroll:true});
    const svg=this.canvas.querySelector('svg'),box=body.getBBox(),point=this.point(event);
    this.gesture={node:id,pointer:event.pointerId,x:event.clientX,y:event.clientY,offset:{x:point.x-box.x-box.width/2,y:point.y-box.y-box.height/2},bounds:[...this.view.bounds],fitted:this.view.fitted,markup:svg.outerHTML,status:this.status.textContent,document:this.getDocument(),frame:0,moved:false};
    this.view.fitted=false;
    this.canvas.classList.add('is-dragging');document.body.classList.add('diagram-dragging');
    this.canvas.setPointerCapture(event.pointerId);
  }
  operations(event){
    const point=this.point(event),gesture=this.gesture;
    return JSON.stringify([{node:gesture.node,position:{x:point.x-gesture.offset.x,y:point.y-gesture.offset.y}}]);
  }
  move(event){
    const gesture=this.gesture;if(!gesture||event.pointerId!==gesture.pointer)return;
    event.preventDefault();
    gesture.moved=Math.hypot(event.clientX-gesture.x,event.clientY-gesture.y)>=4;
    gesture.operations=this.operations(event);
    if(!gesture.frame)gesture.frame=requestAnimationFrame(()=>{
      gesture.frame=0;if(this.gesture!==gesture)return;
      try{
        const markup=gesture.moved?gesture.document.preview_drag(gesture.operations):gesture.markup;
        this.show(markup,gesture.bounds);
        this.status.textContent=gesture.status;
        this.canvas.classList.remove('invalid-drag');
      }catch(error){this.canvas.classList.add('invalid-drag');this.status.textContent=String(error)}
    });
  }
  release(){
    const gesture=this.gesture;if(!gesture)return;
    this.gesture=undefined;cancelAnimationFrame(gesture.frame);
    this.view.fitted=gesture.fitted;
    this.canvas.classList.remove('is-dragging','invalid-drag');document.body.classList.remove('diagram-dragging');
    if(this.canvas.hasPointerCapture(gesture.pointer))this.canvas.releasePointerCapture(gesture.pointer);
    return gesture;
  }
  finish(event){
    const gesture=this.gesture;if(!gesture||event.pointerId!==gesture.pointer)return;
    event.preventDefault();
    const operations=this.operations(event),moved=Math.hypot(event.clientX-gesture.x,event.clientY-gesture.y)>=4;
    this.release();
    if(!moved){this.show(gesture.markup);this.status.textContent=gesture.status;return}
    try{gesture.document.drag(operations);this.onCommit()}
    catch(error){this.show(gesture.markup);this.status.textContent=String(error)}
  }
  cancel(){
    const gesture=this.release();if(!gesture)return;
    this.show(gesture.markup);this.status.textContent=gesture.status;
  }
}
