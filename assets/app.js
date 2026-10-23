'use strict';
const $=s=>document.querySelector(s);
let paused=false,latestData=null,selected='all',mode='both',inflight=false,timer,rotation=0,toastTimer,frozenCanvas=null;
const cards=[],seen=new Map();
let layoutKey='';
const stamp=d=>d?d.file+'|'+d.time:'';
const formatTime=t=>new Date(t*1000).toLocaleString('zh-TW',{hour12:false});
const timeOnly=t=>new Date(t*1000).toLocaleTimeString('zh-TW',{hour12:false});
const num=n=>Number(n||0).toLocaleString('zh-TW');
function age(t){const seconds=Math.max(0,Math.floor(Date.now()/1000-t));if(seconds<5)return '剛剛收到';if(seconds<60)return seconds+' 秒前';if(seconds<3600)return Math.floor(seconds/60)+' 分鐘前';if(seconds<86400)return Math.floor(seconds/3600)+' 小時前';return Math.floor(seconds/86400)+' 天前';}
function toast(text){$('#toast').textContent=text;$('#toast').hidden=false;clearTimeout(toastTimer);toastTimer=setTimeout(()=>$('#toast').hidden=true,2200);}
async function copy(text){try{await navigator.clipboard.writeText(text);toast('已複製條碼：'+text);}catch(e){toast('目前無法複製，請從照片下方選取條碼內容。');}}
function createCard(index){
  const card=document.createElement('article');card.className='card';card.id='cam'+index;
  card.innerHTML=`<div class="cardhead"><div class="position"><span class="deviceindex" aria-hidden="true">${index+1}</span><div><h2>設備 ${index+1}</h2><div class="ip">等待設定</div></div></div><span class="badge">等待照片</span></div><div class="views">${['latest','ng'].map(type=>`<div class="view" data-type="${type}"><div class="viewhead"><strong>${type==='latest'?'最新讀碼':'最近 NG'}</strong><span>${type==='latest'?'每次讀碼更新':'後續 OK 不覆蓋'}</span></div><button class="imagebutton" disabled><span class="empty">${type==='latest'?'等待讀碼照片':'尚無 NG 紀錄'}<small>${type==='latest'?'設備觸發並傳圖後，自動顯示':'發生 NoRead 後，保留在這裡'}</small></span><img hidden alt=""></button><div class="caption"><time>尚未收到</time><span class="age"></span></div>${type==='latest'?'<div class="resultrow"><label>條碼內容</label><span class="code">—</span><button class="copy" disabled>複製</button></div>':'<div class="ngnote">僅保留最近一次未讀取照片</div>'}</div>`).join('')}</div><div class="reception"></div><div class="cardfoot"><span class="retained">已保留 0 張</span><span class="meter" aria-hidden="true"><i></i></span><span class="total">累計接收 0 張</span></div>`;
  $('#grid').append(card);cards.push(card);
  card.querySelector('.copy').onclick=()=>{const values=card._camera?.latest?.codes||[];if(values.length)copy(values.join('、'));};
  for(const view of card.querySelectorAll('.view')){
    const type=view.dataset.type,button=view.querySelector('.imagebutton');
    button.onclick=()=>openPhoto(index,type,button._snapshot,view.querySelector('img'));
  }
}
function resultLabel(d){if(!d)return '等待照片';if(d.result==='ok')return 'OK · 已讀取';if(d.result==='ng')return 'NG · NoRead';return Date.now()/1000-d.time<8?'等待判定':'待確認';}
function paint(c,index){
  const card=cards[index];if(!card)return;card._camera=c;
  card.querySelector('h2').textContent=c.label;card.querySelector('.ip').textContent=c.ip+' · FTP '+c.ftp_port;
  const reception=card.querySelector('.reception'), stale=latestData.stale_seconds>0&&c.latest&&Date.now()/1000-c.latest.time>latestData.stale_seconds;
  reception.classList.toggle('stale',Boolean(stale));reception.textContent=c.latest?(stale?'長時間未收到 · ':'最近接收 · ')+age(c.latest.time):'尚未收到照片 · 請確認設備觸發與 FTP 設定';
  $('#positions').querySelector(`[data-slot="${index}"]`).textContent=c.label;
  const badge=card.querySelector('.badge');badge.className='badge '+(c.latest?.result||'');badge.textContent=resultLabel(c.latest);card.classList.toggle('is-ng',c.latest?.result==='ng');
  for(const type of ['latest','ng']){
    const d=c[type],view=card.querySelector(`[data-type="${type}"]`);if(!d)continue;
    const img=view.querySelector('img'),button=view.querySelector('.imagebutton'),empty=view.querySelector('.empty'),key=index+type;
    if(seen.get(key)!==stamp(d)){
      seen.set(key,stamp(d));button.disabled=true;
      const snapshot={...d,codes:[...(d.codes||[])],label:c.label,ip:c.ip};
      img.onload=()=>{img.hidden=false;empty.hidden=true;button.disabled=false;button._snapshot=snapshot;};
      img.onerror=()=>{img.hidden=true;empty.hidden=false;empty.textContent='照片載入失敗，正在重試';button.disabled=true;seen.delete(key);};
      img.alt=c.label+' '+(type==='latest'?'最新讀碼':'最近 NG')+'，'+formatTime(d.time);
      img.src='/'+encodeURIComponent(d.file)+'?t='+encodeURIComponent(d.time);
    }else if(button._snapshot){button._snapshot={...button._snapshot,...d,label:c.label,ip:c.ip};}
    view.querySelector('time').textContent=formatTime(d.time);view.querySelector('time').dateTime=new Date(d.time*1000).toISOString();view.querySelector('.age').textContent=age(d.time);
  }
  const codes=c.latest?.codes||[];
  card.querySelector('.code').textContent=codes.length?codes.join('、'):c.latest?.result==='ng'?'NoRead':c.latest?.result==='ok'?'已讀取':c.latest?'等待結果':'—';
  card.querySelector('.copy').disabled=!codes.length;
  const limit=latestData?.max_photos||1000;
  card.querySelector('.retained').textContent='已保留 '+num(c.retained)+' / '+num(limit)+' 張';
  card.querySelector('.meter i').style.width=Math.min(100,(c.retained||0)/limit*100)+'%';
  card.querySelector('.total').textContent='累計接收 '+num(c.count)+' 張';
}
function render(){if(!latestData||paused)return;const cams=latestData.cameras;
  const key=JSON.stringify(cams.map(c=>[c.id,c.ip,c.label]));
  if(key!==layoutKey){layoutKey=key;selected='all';cards.length=0;seen.clear();$('#grid').replaceChildren();$('#positions').replaceChildren();
    const all=document.createElement('button');all.dataset.slot='all';all.textContent='全部設備';all.setAttribute('aria-pressed','true');$('#positions').append(all);
    cams.forEach((c,i)=>{createCard(i);const b=document.createElement('button');b.dataset.slot=String(i);b.textContent=c.label;b.setAttribute('aria-pressed','false');$('#positions').append(b);});
    $('#grid').classList.remove('focused');$('#grid').classList.toggle('single',cams.length===1);$('#grid').classList.toggle('many',cams.length>4);
  }
  $('#emptyDevices').hidden=cams.length!==0;cams.forEach(paint);
  $('#received').textContent=cams.filter(c=>c.latest).length+' / '+cams.length;
  $('#currentNg').textContent=cams.filter(c=>c.latest?.result==='ng').length;
  $('#ngs').textContent=cams.filter(c=>c.ng).length;
  const timestamps=cams.filter(c=>c.latest).map(c=>c.latest.time);const last=timestamps.length?Math.max(...timestamps):null;
  $('#lastReceived').textContent=last?timeOnly(last):'—';$('#lastReceived').title=last?formatTime(last):'尚未收到照片';
}
function setPaused(value){paused=value;$('#pause').textContent=paused?'繼續更新':'暫停畫面';$('#pause').classList.toggle('frozen',paused);$('#pause').setAttribute('aria-pressed',String(paused));$('#frozen').hidden=!paused;if(!paused)render();}
$('#pause').onclick=()=>setPaused(!paused);$('#resume').onclick=()=>setPaused(false);
$('#positions').onclick=event=>{const b=event.target.closest('button');if(!b)return;selected=b.dataset.slot;for(const button of $('#positions').querySelectorAll('button'))button.setAttribute('aria-pressed',String(button===b));cards.forEach((card,index)=>card.hidden=selected!=='all'&&String(index)!==selected);$('#grid').classList.toggle('focused',selected!=='all');};
$('#views').onclick=event=>{const b=event.target.closest('button');if(!b)return;mode=b.dataset.view;for(const button of $('#views').querySelectorAll('button'))button.setAttribute('aria-pressed',String(button===b));$('#grid').classList.toggle('view-only-latest',mode==='latest');$('#grid').classList.toggle('view-only-ng',mode==='ng');};
function openPhoto(index,type,snapshot,img){if(!snapshot||!img.naturalWidth)return;
  // Freeze the pixels too: latest.jpg may be overwritten while this dialog is open.
  const canvas=document.createElement('canvas');canvas.width=img.naturalWidth;canvas.height=img.naturalHeight;canvas.getContext('2d').drawImage(img,0,0);
  frozenCanvas=canvas;$('#large').src=canvas.toDataURL('image/png');$('#large').alt=snapshot.label+' '+(type==='ng'?'最近 NG':'最新讀碼');
  $('#viewerTitle').textContent=snapshot.label+' · '+(type==='ng'?'最近 NG':'最新讀碼');
  $('#viewerMeta').textContent=snapshot.ip+' ｜ '+formatTime(snapshot.time);
  $('#viewerCode').textContent='條碼：'+(snapshot.codes?.join('、')||(snapshot.result==='ng'?'NoRead':'等待結果資料'));
  rotation=0;$('#large').style.transform='';$('#zoomstage').classList.remove('zoomed');$('#zoom').textContent='放大 160%';$('#zoom').setAttribute('aria-pressed','false');$('#viewer').showModal();
}
$('#zoom').onclick=()=>{const zoomed=$('#zoomstage').classList.toggle('zoomed');$('#zoom').textContent=zoomed?'適合視窗':'放大 160%';$('#zoom').setAttribute('aria-pressed',String(zoomed));};
$('#rotate').onclick=()=>{if(!frozenCanvas)return;rotation=(rotation+90)%360;const rotated=document.createElement('canvas'),quarter=rotation%180!==0;rotated.width=quarter?frozenCanvas.height:frozenCanvas.width;rotated.height=quarter?frozenCanvas.width:frozenCanvas.height;const ctx=rotated.getContext('2d');ctx.translate(rotated.width/2,rotated.height/2);ctx.rotate(rotation*Math.PI/180);ctx.drawImage(frozenCanvas,-frozenCanvas.width/2,-frozenCanvas.height/2);$('#large').src=rotated.toDataURL('image/png');};
$('#close').onclick=()=>$('#viewer').close();$('#closeHelp').onclick=()=>$('#helpDialog').close();
for(const dialog of document.querySelectorAll('dialog:not(#settingsDialog)'))dialog.addEventListener('click',e=>{if(e.target===dialog){const r=dialog.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)dialog.close();}});
$('#help').onclick=()=>{$('#dataPath').textContent=latestData?.data_path||'與監控程式位於同一個資料夾';$('#limit').textContent=num(latestData?.max_photos||1000);$('#helpDialog').showModal();};
$('#fullscreen').onclick=async()=>{try{if(document.fullscreenElement)await document.exitFullscreen();else await document.documentElement.requestFullscreen();}catch(e){toast('此視窗未提供全螢幕操作，可使用視窗最大化。');}};
document.addEventListener('fullscreenchange',()=>$('#fullscreen').textContent=document.fullscreenElement?'離開全螢幕':'全螢幕');
document.addEventListener('keydown',e=>{if(e.code==='Space'&&!e.repeat&&!document.querySelector('dialog[open]')&&!['BUTTON','INPUT','TEXTAREA','SELECT'].includes(e.target.tagName)){e.preventDefault();setPaused(!paused);}});
async function poll(){if(inflight)return;clearTimeout(timer);inflight=true;const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),3000);
  try{const response=await fetch('/api/status',{signal:controller.signal,cache:'no-store'});if(!response.ok)throw Error();const data=await response.json();if(!Array.isArray(data.cameras)||data.cameras.length>10)throw Error();latestData=data;$('#pendingSettings').hidden=!data.restart_required;$('#service').textContent='接收服務正常';$('#service').classList.remove('off');$('#error').hidden=true;render();}
  catch(e){$('#service').textContent='服務無回應';$('#service').classList.add('off');$('#error').hidden=false;}
  finally{clearTimeout(timeout);inflight=false;timer=setTimeout(poll,1000);}
}
$('#retry').onclick=poll;poll();
