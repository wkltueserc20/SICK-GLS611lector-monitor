'use strict';
function refreshTargets(){
  const select=document.querySelector('#demoTarget'),previous=select.value;select.replaceChildren();
  window.demoControls.cameras().forEach(c=>{const option=document.createElement('option');option.value=c.id;option.textContent=c.label;select.append(option);});
  if([...select.options].some(o=>o.value===previous))select.value=previous;
  document.querySelectorAll('[data-result]').forEach(b=>b.disabled=!select.options.length);
}
document.querySelectorAll('[data-preset]').forEach(b=>b.onclick=()=>{setPaused(false);window.demoControls.preset(Number(b.dataset.preset));layoutKey='';poll();toast('已切換為 '+b.dataset.preset+' 台示範設備');});
document.querySelectorAll('[data-result]').forEach(b=>b.onclick=()=>{window.demoControls.receive(b.dataset.result,document.querySelector('#demoTarget').value);poll();if(paused)toast('模擬結果已產生；請繼續畫面更新以查看。');});
document.querySelector('#demoAuto').onclick=()=>{window.demoRunning=!window.demoRunning;const b=document.querySelector('#demoAuto');b.setAttribute('aria-pressed',String(window.demoRunning));b.textContent=window.demoRunning?'自動模擬：開':'自動模擬：關';};
window.addEventListener('democonfig',refreshTargets);refreshTargets();
