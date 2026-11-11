// WebGPU rectangles/routes with an SVG text layer: glyphs remain flat and undistorted.
export class StackRenderer {
  constructor(container) { this.container = container; this.mode = 'SVG';this.vertexBuffers=new Map();this.retiredBuffers=[]; }
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
    const nodes = stack.planes.flatMap(plane => Object.entries(plane.layout.nodes).map(([id, r]) => ({ id, ...r, layer: plane.index })));
    if (!nodes.length) { this.container.textContent = 'Empty stack'; return; }
    const left = Math.min(...nodes.map(r => r.x-r.w/2))-32, top = Math.min(...nodes.map(r => r.y-r.h/2))-32;
    const right = Math.max(...nodes.map(r => r.x+r.w/2))+32, bottom = Math.max(...nodes.map(r => r.y+r.h/2))+32;
    const width=right-left, height=bottom-top;
    this.container.replaceChildren(); this.container.style.position='relative'; this.container.style.aspectRatio=`${width}/${height}`;
    const svg=document.createElementNS('http://www.w3.org/2000/svg','svg'); svg.setAttribute('viewBox',`${left} ${top} ${width} ${height}`); svg.style.cssText='position:absolute;inset:0;width:100%;height:100%;pointer-events:none';
    const triangles=[], lines=[]; const colors=[[.78,.87,.98,1],[.85,.93,.85,1],[.95,.88,.76,1]];
    const vertex=(out,x,y,color)=>out.push((x-left)/width*2-1,1-(y-top)/height*2,...color);
    const line=(a,b,color=[.35,.43,.55,1])=>{vertex(lines,a.x,a.y,color);vertex(lines,b.x,b.y,color);};
    for(const plane of stack.planes) for(const route of Object.values(plane.layout.routes)) for(let i=1;i<route.points.length;i++) line(route.points[i-1],route.points[i]);
    for(const via of stack.vias) line(via.source,via.target,[.6,.35,.65,1]);
    for(const r of nodes){const x=r.x-r.w/2,y=r.y-r.h/2,c=colors[r.layer%colors.length];for(const [dx,dy] of [[0,0],[r.w,0],[r.w,r.h],[0,0],[r.w,r.h],[0,r.h]])vertex(triangles,x+dx,y+dy,c);
      if(!this.device){const rect=document.createElementNS(svg.namespaceURI,'rect');for(const [k,v] of Object.entries({x,y,width:r.w,height:r.h,rx:8,fill:`rgb(${c.slice(0,3).map(v=>v*255).join(' ')})`,stroke:'#9db2c7'}))rect.setAttribute(k,String(v));svg.append(rect);}
      const text=document.createElementNS(svg.namespaceURI,'text');text.setAttribute('x',r.x);text.setAttribute('y',r.y+4);text.setAttribute('text-anchor','middle');text.setAttribute('font-size','14');text.textContent=r.id;svg.append(text);
    }
    if(this.device){const canvas=this.canvas||(this.canvas=document.createElement('canvas'));canvas.style.cssText='width:100%;height:100%;position:absolute;inset:0';canvas.width=Math.max(1,Math.ceil(this.container.clientWidth*devicePixelRatio));canvas.height=Math.max(1,Math.ceil(canvas.width*height/width));this.container.append(canvas);const context=this.context||(this.context=canvas.getContext('webgpu'));context.configure({device:this.device,format:this.format,alphaMode:'premultiplied'});const encoder=this.device.createCommandEncoder();const pass=encoder.beginRenderPass({colorAttachments:[{view:context.getCurrentTexture().createView(),clearValue:{r:1,g:1,b:1,a:1},loadOp:'clear',storeOp:'store'}]});for(const [kind,values,pipeline] of [['fill',triangles,this.fill],['line',lines,this.line]]){if(!values.length)continue;const data=new Float32Array(values);let allocation=this.vertexBuffers.get(kind);if(!allocation||allocation.size<data.byteLength){if(allocation)this.retiredBuffers.push(allocation.buffer);const size=2**Math.ceil(Math.log2(Math.max(4,data.byteLength)));allocation={size,buffer:this.device.createBuffer({size,usage:GPUBufferUsage.VERTEX|GPUBufferUsage.COPY_DST})};this.vertexBuffers.set(kind,allocation);}const buffer=allocation.buffer;this.device.queue.writeBuffer(buffer,0,data);pass.setPipeline(pipeline);pass.setVertexBuffer(0,buffer);pass.draw(values.length/6);}pass.end();this.device.queue.submit([encoder.finish()]);}
    else {for(let i=0;i<lines.length;i+=12){const p=document.createElementNS(svg.namespaceURI,'line');p.setAttribute('x1',left+(lines[i]+1)/2*width);p.setAttribute('y1',top+(1-lines[i+1])/2*height);p.setAttribute('x2',left+(lines[i+6]+1)/2*width);p.setAttribute('y2',top+(1-lines[i+7])/2*height);p.setAttribute('stroke','#8b65a0');svg.prepend(p);}}
    this.container.append(svg);
    if(this.device){try{await this.device.queue.onSubmittedWorkDone();}catch(error){console.warn('WebGPU submission unavailable; using SVG',String(error));this.device=undefined;this.mode='SVG';return this.draw(stack);}}
  }
}
