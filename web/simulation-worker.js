// Compile the optional runtime Wasm bundle using scripts/build-wasm.sh runtime.
import init, { WasmSimulation } from './pkg-runtime/smarter_diagrams.js';
let simulation, timer;
let commands=Promise.resolve();
self.onmessage = ({ data }) => {commands=commands.then(async()=>{
  try {
    if (data.type === 'open') {
      await init(); clearInterval(timer); if(simulation)simulation.free();simulation = new WasmSimulation(data.source);
      for(const edge of data.cuts||[])simulation.sever(edge,true);
      timer = setInterval(() => { simulation.tick(); self.postMessage({ type: 'frame', state: JSON.parse(simulation.state()) }); }, 1000 / 60);
    } else if (data.type === 'rate') simulation.set_rate(data.node, data.rate);
    else if (data.type === 'sever') simulation.sever(data.edge, data.severed);
    else if (data.type === 'stop') { clearInterval(timer); timer = undefined; }
  } catch (error) { self.postMessage({ type: 'error', message: String(error) }); }
});};
