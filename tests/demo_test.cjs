// Test the browser-only simulator without network, devices or frontend packages.
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const file=path.join(__dirname,'../demo/simulator.js'),storage=new Map();
const context={window:{dispatchEvent(){}},localStorage:{getItem:k=>storage.get(k),setItem:(k,v)=>storage.set(k,v)},Response,DOMException,Event,crypto:require('node:crypto').webcrypto,document:{hidden:false},setInterval(fn){context.tick=fn;}};
vm.runInNewContext(fs.readFileSync(file,'utf8'),context);
const api=async(route='/api/status',body)=>{
 const response=await context.window.demoAPI(route,body?{method:'POST',body:JSON.stringify(body)}:{});
 return {status:response.status,data:await response.json()};
};
(async()=>{
 let state=(await api()).data;assert.equal(state.cameras.length,4);
 const first=state.cameras[0].id;
 context.window.demoControls.receive('ng',first);const ng=(await api()).data.cameras[0].ng;
 context.window.demoControls.receive('ok',first);state=(await api()).data;
 assert.equal(state.cameras[0].latest.result,'ok');assert.deepEqual(state.cameras[0].ng,ng);
 for(const n of [1,10,4]){context.window.demoControls.preset(n);assert.equal((await api()).data.cameras.length,n);}
 let cfg=(await api('/api/config')).data;
 cfg.cameras[0].label='<img src=x onerror=alert(1)>';cfg.cameras[0].ip='192.0.2.99';cfg.cameras[0].ftp_port=2222;
 assert.equal((await api('/api/config',cfg)).status,200);
 assert.equal((await api()).data.cameras[0].ftp_port,2222);
 cfg.cameras[1].ip=cfg.cameras[0].ip;assert.equal((await api('/api/config',cfg)).status,400);
 cfg=(await api('/api/config')).data;cfg.cameras[0].ftp_port=30000;assert.equal((await api('/api/config',cfg)).status,400);
 cfg=(await api('/api/config')).data;cfg.cameras.forEach(c=>c.enabled=false);assert.equal((await api('/api/config',cfg)).status,200);assert.equal((await api()).data.cameras.length,0);
 assert.equal((await api('/api/config',{...cfg,cameras:[]})).status,400);
 context.window.demoControls.preset(1);let count=(await api()).data.cameras[0].count;context.tick();assert.equal((await api()).data.cameras[0].count,count+1);
 context.window.demoRunning=false;context.tick();assert.equal((await api()).data.cameras[0].count,count+1);
 const another={...context,window:{dispatchEvent(){}},setInterval(){}};vm.runInNewContext(fs.readFileSync(file,'utf8'),another);assert.equal((await (await another.window.demoAPI('/api/config')).json()).cameras.length,1);
 const html=fs.readFileSync(path.join(__dirname,'../demo/index.html'),'utf8');
 for(const match of html.matchAll(/(?:src|href)="(\.\/[^"?#]+)"/g))assert.ok(fs.existsSync(path.join(__dirname,'../demo',match[1])));
 for(const f of ['app.js','settings.js','simulator.js','controls.js'])assert.ok(!/\bfetch\s*\(|XMLHttpRequest|WebSocket/.test(fs.readFileSync(path.join(__dirname,'../demo',f),'utf8')),'No real networking: '+f);
 console.log('PASS: synthetic OK/NG history; 1/4/10; config validation; per-device port; disabled devices; browser-local persistence; auto/manual simulation; relative assets; no real networking');
})().catch(e=>{console.error(e);process.exitCode=1;});
