// WebGPU silhouette fills with the core SVG renderer for semantic ink and text.
export class StackRenderer {
  constructor(container,onStage) { this.container = container;this.onStage=onStage; this.mode = 'SVG';this.vertexBuffers=new Map();this.retiredBuffers=[]; }
  async initialize() {
    this.gpu=navigator.gpu;if (!this.gpu) return;
    this.adapter = await this.gpu.requestAdapter() || await this.gpu.requestAdapter({forceFallbackAdapter:true}); if (!this.adapter) return;
    this.device = await this.adapter.requestDevice();
    const shader = this.device.createShaderModule({ code: `
struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f }
@vertex fn vs(@location(0) position: vec2f, @location(1) color: vec4f) -> Out { var out: Out; out.position=vec4f(position,0,1); out.color=color; return out; }
@fragment fn fs(in: Out) -> @location(0) vec4f { return in.color; }` });
    const info=await shader.getCompilationInfo();const error=info.messages.find(m=>m.type==='error');if(error)throw new Error(error.message);
    const format = this.gpu.getPreferredCanvasFormat();
    const descriptor = topology => ({ layout: 'auto', vertex: { module: shader, entryPoint: 'vs', buffers: [{ arrayStride: 24, attributes: [{ shaderLocation: 0, offset: 0, format: 'float32x2' }, { shaderLocation: 1, offset: 8, format: 'float32x4' }] }] }, fragment: { module: shader, entryPoint: 'fs', targets: [{ format, blend: { color: { srcFactor: 'src-alpha', dstFactor: 'one-minus-src-alpha' }, alpha: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' } } }] }, primitive: { topology } });
    this.fill = await this.device.createRenderPipelineAsync(descriptor('triangle-list'));
    this.line = await this.device.createRenderPipelineAsync(descriptor('line-list'));
    // Some drivers expose an adapter and compile shaders, but reject the first
    // submission. Probe the queue before replacing the working SVG fallback.
    const probe=this.device.createTexture({size:[1,1],format,usage:GPUTextureUsage.RENDER_ATTACHMENT});
    const encoder=this.device.createCommandEncoder();
    encoder.beginRenderPass({colorAttachments:[{view:probe.createView(),clearValue:{r:1,g:1,b:1,a:1},loadOp:'clear',storeOp:'store'}]}).end();
    this.device.queue.submit([encoder.finish()]);
    await this.device.queue.onSubmittedWorkDone();probe.destroy();
    this.format = format; this.mode = 'WebGPU';
  }
  async draw(stack) {
    if (!stack.svg) throw new Error('Stack presentation is missing; rebuild the runtime bundle.');
    const svg = new DOMParser().parseFromString(stack.svg, 'image/svg+xml').documentElement;
    if (svg.localName !== 'svg' || svg.querySelector('parsererror')) throw new Error('Invalid stack SVG');
    const [left,top,width,height] = svg.getAttribute('viewBox').split(/\s+/).map(Number);
    this.container.replaceChildren();
    const stage=document.createElement('div');stage.className='stack-stage';stage.style.cssText=`position:relative;aspect-ratio:${width}/${height}`;
    svg.style.cssText='position:absolute;inset:0;width:100%;height:100%';
    // Prefix paint/description IDs when the native SVG shares a page with a diff.
    const names=new Map([...svg.querySelectorAll('[id]')].map(el=>[el.id,`stack-${el.id}`]));
    for(const el of [svg,...svg.querySelectorAll('*')])for(const attr of [...el.attributes]){
      let value=attr.value;
      if(attr.name==='id')value=names.get(value)||value;
      else if(attr.name==='aria-labelledby')value=value.split(' ').map(id=>names.get(id)||id).join(' ');
      else for(const [id,next]of names)value=value.replaceAll(`url(#${id})`,`url(#${next})`);
      el.setAttribute(attr.name,value);
    }
    stage.append(svg);this.container.append(stage);this.onStage?.(stage,[left,top,width,height]);
    if(this.device){
      // Native silhouettes supply both SVG fallback ink and GPU fill geometry.
      // Directions, captions, and relation dashes remain the same SVG in either mode.
      const triangles=[];
      const vertex=(x,y,color)=>triangles.push((x-left)/width*2-1,1-(y-top)/height*2,...color);
      for(const shape of svg.querySelectorAll('[data-plane-background],[data-silhouette],[data-zone]')){
        const style=getComputedStyle(shape), rgba=style.fill.match(/[\d.]+/g)?.map(Number);
        if(!rgba || style.fill==='none')continue;
        const color=[rgba[0]/255,rgba[1]/255,rgba[2]/255,(rgba[3]??1)*Number(style.fillOpacity)];
        const box=shape.getBBox(),cx=box.x+box.width/2,cy=box.y+box.height/2;
        const length=shape.getTotalLength(),count=96;
        for(let i=0;i<count;i++){const a=shape.getPointAtLength(length*i/count),b=shape.getPointAtLength(length*(i+1)/count);vertex(cx,cy,color);vertex(a.x,a.y,color);vertex(b.x,b.y,color);}
        shape.style.fill='none';
      }
      const canvas=this.canvas||(this.canvas=document.createElement('canvas'));canvas.style.cssText='width:100%;height:100%;position:absolute;inset:0';
      canvas.width=Math.max(1,Math.ceil(stage.clientWidth*devicePixelRatio));canvas.height=Math.max(1,Math.ceil(canvas.width*height/width));stage.prepend(canvas);
      const context=this.context||(this.context=canvas.getContext('webgpu'));context.configure({device:this.device,format:this.format,alphaMode:'premultiplied'});
      const encoder=this.device.createCommandEncoder();const pass=encoder.beginRenderPass({colorAttachments:[{view:context.getCurrentTexture().createView(),clearValue:{r:1,g:1,b:1,a:1},loadOp:'clear',storeOp:'store'}]});
      if(triangles.length){const data=new Float32Array(triangles);let allocation=this.vertexBuffers.get('fill');if(!allocation||allocation.size<data.byteLength){if(allocation)this.retiredBuffers.push(allocation.buffer);const size=2**Math.ceil(Math.log2(Math.max(4,data.byteLength)));allocation={size,buffer:this.device.createBuffer({size,usage:GPUBufferUsage.VERTEX|GPUBufferUsage.COPY_DST})};this.vertexBuffers.set('fill',allocation);}this.device.queue.writeBuffer(allocation.buffer,0,data);pass.setPipeline(this.fill);pass.setVertexBuffer(0,allocation.buffer);pass.draw(triangles.length/6);}
      pass.end();this.device.queue.submit([encoder.finish()]);
      try{await this.device.queue.onSubmittedWorkDone();}catch(error){console.warn('WebGPU submission unavailable; using SVG',String(error));this.device=undefined;this.mode='SVG';return this.draw(stack);}
    }
    const legend=document.createElement('p');legend.className='stack-legend';legend.textContent='Solid arrows connect services within a stratum. Dashed arrows cross strata; their labels name the relationship.';this.container.append(legend);
  }
}
