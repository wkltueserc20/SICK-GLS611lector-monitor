/* Static, browser-only simulator. No network or device API requests. */
'use strict';
(()=>{
  const KEY='chingtech-monitor-public-demo-v1';
  const clone=value=>JSON.parse(JSON.stringify(value));
  const labels=['左上','右上','左下','右下'];
  const defaults=n=>({ftp_host:'0.0.0.0',ftp_port:2121,web_port:8080,max_photos:1000,stale_seconds:300,cameras:Array.from({length:n},(_,i)=>({id:'demo-'+(i+1),ip:'192.0.2.'+(17+i),label:n===4?labels[i]:'設備 '+(i+1),ftp_port:null,enabled:true}))});
  function validate(input){
    const cfg=clone(input),integer=(v,min,max)=>Number.isInteger(v)&&v>=min&&v<=max;
    if(!Array.isArray(cfg.cameras)||cfg.cameras.length<1||cfg.cameras.length>10)throw Error('設備數量必須為 1～10 台（含停用設備）');
    for(const key of ['ftp_port','web_port'])if(!integer(cfg[key],1,65535))throw Error('連接埠必須為 1～65535');
    if(!integer(cfg.max_photos,1,100000))throw Error('保留照片數需為 1～100000');
    if(!integer(cfg.stale_seconds,0,604800))throw Error('提醒秒數需為 0～604800');
    const ipv4=v=>typeof v==='string'&&/^\d{1,3}(\.\d{1,3}){3}$/.test(v)&&v.split('.').every(p=>String(Number(p))===p&&Number(p)<=255);
    if(!ipv4(cfg.ftp_host))throw Error('FTP 綁定位址需為有效 IPv4');
    const ips=new Set(),ids=new Set();
    for(const c of cfg.cameras){
      c.ip=c.ip?.trim();c.label=c.label?.trim();
      if(!ipv4(c.ip))throw Error('設備 IP 格式錯誤');
      if(ips.has(c.ip))throw Error('設備 IP 重複');ips.add(c.ip);
      if(!c.label||[...c.label].length>40)throw Error('設備名稱需為 1～40 字');
      c.id=c.id||crypto.randomUUID();if(typeof c.id!=='string'||!/^[-a-zA-Z0-9.]{1,64}$/.test(c.id)||ids.has(c.id.toLowerCase()))throw Error('設備識別碼無效或重複');ids.add(c.id.toLowerCase());
      c.enabled=c.enabled!==false;c.ftp_port=c.ftp_port??null;
      const port=c.ftp_port??cfg.ftp_port;
      if(!integer(port,1,65535)||port===cfg.web_port||(port>=30000&&port<=30010))throw Error('FTP 埠不可為網頁埠或 30000～30010');
    }
    return cfg;
  }
  let config=defaults(4);
  try{const saved=localStorage.getItem(KEY);if(saved)config=validate(JSON.parse(saved));}catch(_){/* Storage can be unavailable in private mode. */}
  const cameras=new Map();let serial=1067,ticks=0;
  window.demoRunning=true;
  const stamp=(index,result,offset=0)=>({file:`sample-${result}-${index%4}.svg`,name:`DEMO-${serial}.jpg`,time:Date.now()/1000-offset,result,codes:result==='ok'?['DEMO'+String(serial).padStart(6,'0')]:[]});
  function sync(){
    config.cameras.forEach((c,i)=>{
      let camera=cameras.get(c.id);
      if(!camera){camera={id:c.id,count:120+i*27,retained:120+i*27,latest:stamp(i,i===2?'ng':'ok',i*2),ng:i<3?stamp(i,'ng',90+i*30):null};cameras.set(c.id,camera);}
      Object.assign(camera,{label:c.label,ip:c.ip,ftp_port:c.ftp_port??config.ftp_port});camera.retained=Math.min(camera.retained,config.max_photos);
    });
    window.dispatchEvent(new Event('democonfig'));
  }
  function receive(result,id){
    const enabled=config.cameras.filter(c=>c.enabled);if(!enabled.length)return;
    const c=enabled.find(c=>c.id===id)||enabled[ticks%enabled.length],index=config.cameras.indexOf(c),camera=cameras.get(c.id);
    serial++;camera.latest=stamp(index,result);if(result==='ng')camera.ng=clone(camera.latest);camera.count++;camera.retained=Math.min(config.max_photos,camera.retained+1);ticks++;
    window.dispatchEvent(new Event('demoframe'));
  }
  window.demoAPI=async(route,options={})=>{
    if(options.signal?.aborted)throw new DOMException('Aborted','AbortError');
    if(route==='/api/status')return Response.json({cameras:config.cameras.filter(c=>c.enabled).map(c=>clone(cameras.get(c.id))),engine:'browser-demo',max_photos:config.max_photos,stale_seconds:config.stale_seconds,data_path:'展示模式：所有照片皆為合成示意圖，未寫入磁碟',restart_required:false});
    if(route==='/api/config'&&options.method==='POST'){
      try{const next=validate(JSON.parse(options.body));config=next;sync();let saved=true;try{localStorage.setItem(KEY,JSON.stringify(config));}catch(_){saved=false;}return Response.json({config:clone(config),restart_required:false,local_saved:saved});}
      catch(e){return Response.json({error:e.message},{status:400});}
    }
    if(route==='/api/config')return Response.json(clone(config));
    return Response.json({error:'展示版未提供此操作'},{status:404});
  };
  window.demoControls={
    receive,
    preset(n){config=defaults(n);cameras.clear();try{localStorage.setItem(KEY,JSON.stringify(config));}catch(_){}sync();},
    cameras(){return config.cameras.filter(c=>c.enabled).map(c=>({id:c.id,label:c.label}));}
  };
  sync();
  setInterval(()=>{if(window.demoRunning&&!document.hidden)receive(ticks%5===3?'ng':'ok');},2500);
})();
