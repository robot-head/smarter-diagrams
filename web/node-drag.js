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
    canvas.addEventListener('dblclick',event=>this.resetLabel(event));
    for(const type of ['scroll','resize','wheel'])window.addEventListener(type,()=>{if(this.gesture)this.gesture.inverse=undefined},true);
    window.addEventListener('keydown',event=>{if(this.active&&event.key==='Escape'){event.preventDefault();this.cancel()}});
    window.addEventListener('blur',()=>this.cancel());
  }
  get active(){return !!this.gesture}
  point(event){const inverse=this.gesture?.inverse||this.canvas.querySelector('svg').getScreenCTM().inverse();if(this.gesture)this.gesture.inverse=inverse;return new DOMPoint(event.clientX,event.clientY).matrixTransform(inverse)}
  start(event){
    if(this.active||event.button!==0||!event.isPrimary||(event.pointerType==='touch'&&!this.touchEnabled()))return;
    const label=event.target.closest('[data-label][data-draggable-label="true"]'),hit=event.target.closest('[data-node],[data-owner-node]'),id=label?.dataset.label||hit?.dataset.node||hit?.dataset.ownerNode;
    if(!id||this.beforeStart()===false)return;
    const node=[...this.canvas.querySelectorAll(label?'[data-label]':'[data-node]')].find(node=>(label?node.dataset.label:node.dataset.node)===id),body=label?node?.querySelector('rect,text'):node?.querySelector('[data-silhouette]');
    if(!body)return;
    event.preventDefault();this.canvas.focus({preventScroll:true});
    const svg=this.canvas.querySelector('svg'),box=label?{x:Number(node.dataset.labelX),y:Number(node.dataset.labelY),width:0,height:0}:body.getBBox(),point=this.point(event);
    const doc=this.getDocument();let preview;try{preview=doc.begin_drag()}catch(error){this.status.textContent=String(error);return}
    this.gesture={label:!!label,inverse:svg.getScreenCTM().inverse(),node:id,pointer:event.pointerId,x:event.clientX,y:event.clientY,offset:{x:point.x-(label?Number(node.dataset.labelX):box.x+box.width/2),y:point.y-(label?Number(node.dataset.labelY):box.y+box.height/2)},bounds:[...this.view.bounds],fitted:this.view.fitted,markup:svg.outerHTML,status:this.status.textContent,document:doc,preview,frame:0,moved:false,savedFrame:this.view.frame&&[...this.view.frame],valid:true};
    this.view.lockFrame(this.gesture.bounds);
    this.canvas.classList.add('is-dragging');document.body.classList.add('diagram-dragging');
    try{this.canvas.setPointerCapture(event.pointerId)}catch(error){this.cancel();this.status.textContent=String(error)}
  }
  operations(event){
    const point=this.point(event),gesture=this.gesture;
    return JSON.stringify([{node:gesture.node,position:{x:point.x-gesture.offset.x,y:point.y-gesture.offset.y}}]);
  }
  move(event){
    const gesture=this.gesture;if(!gesture||event.pointerId!==gesture.pointer)return;
    event.preventDefault();
    gesture.moved=gesture.moved||Math.hypot(event.clientX-gesture.x,event.clientY-gesture.y)>=4;
    gesture.operations=this.operations(event);
    if(!gesture.frame)gesture.frame=requestAnimationFrame(()=>{
      gesture.frame=0;if(this.gesture!==gesture)return;
      this.preview(gesture);
    });
  }
  preview(gesture){
    try{
      if(gesture.moved&&gesture.applied!==gesture.operations){if(gesture.label){const [op]=JSON.parse(gesture.operations);gesture.preview.update_label(gesture.node,op.position.x,op.position.y)}else gesture.preview.update(gesture.operations);gesture.applied=gesture.operations}
      this.show(gesture.moved?gesture.preview.svg():gesture.markup,gesture.bounds);
      gesture.valid=true;this.status.textContent=gesture.status;this.canvas.classList.remove('invalid-drag');
    }catch(error){gesture.applied=undefined;gesture.valid=false;this.canvas.classList.add('invalid-drag');this.status.textContent=`Cannot place here: ${String(error)}. Move to a clear position or press Escape.`}
  }
  release(restore=true){
    const gesture=this.gesture;if(!gesture)return;
    this.gesture=undefined;cancelAnimationFrame(gesture.frame);gesture.preview.free();
    if(restore){this.view.frame=gesture.savedFrame;this.view.fitted=gesture.fitted}
    this.canvas.classList.remove('is-dragging','invalid-drag');document.body.classList.remove('diagram-dragging');
    if(this.canvas.hasPointerCapture(gesture.pointer))this.canvas.releasePointerCapture(gesture.pointer);
    return gesture;
  }
  finish(event){
    const gesture=this.gesture;if(!gesture||event.pointerId!==gesture.pointer)return;
    event.preventDefault();
    const operations=this.operations(event),moved=gesture.moved||Math.hypot(event.clientX-gesture.x,event.clientY-gesture.y)>=4;
    if(!moved){this.labelClick=gesture.label?{id:gesture.node,time:event.timeStamp}:undefined;this.cancel();return}
    this.labelClick=undefined;
    gesture.operations=operations;gesture.moved=true;
    this.preview(gesture);
    if(!gesture.valid){const message=this.status.textContent;this.cancel();this.status.textContent=`Move canceled. ${message}`;return}
    try{gesture.document.commit_drag(gesture.preview);this.release(false);this.onCommit()}
    catch(error){this.cancel();this.status.textContent=String(error)}
  }

  resetLabel(event){
    const label=event.target.closest('[data-draggable-label="true"]'),id=label?.dataset.label||(this.labelClick&&event.timeStamp-this.labelClick.time<600?this.labelClick.id:undefined);if(!id)return;
    event.preventDefault();this.cancel();if(this.beforeStart()===false)return;
    const doc=this.getDocument();let preview;
    try{preview=doc.begin_drag();preview.reset_label(id);doc.commit_drag(preview);this.onCommit()}
    catch(error){this.status.textContent=String(error)}finally{preview?.free()}
  }

  cancel(){
    const gesture=this.release();if(!gesture)return;
    this.show(gesture.markup);this.status.textContent=gesture.status;
  }
}
