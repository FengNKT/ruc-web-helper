import {formatBytes} from './format.js';

export function setupUpdates(root,call,listen,navigate){
  const q=selector=>root.querySelector(selector);
  let state=null,pending=false;
  const banner=document.createElement('button');
  banner.type='button';banner.className='ruc-update-banner';banner.hidden=true;
  root.querySelector('.ruc-main').prepend(banner);
  banner.addEventListener('click',()=>{navigate('settings');q('[data-update-card]').scrollIntoView({block:'center',behavior:'smooth'});});
  const errorText=error=>typeof error==='string'?error:error?.message||'更新操作失败，请重试';
  function render(){
    if(!state)return;
    const {status,release,preferences}=state;
    const working=pending||['checking','downloading','verifying','installing'].includes(status);
    q('[data-update-status]').textContent=state.message;
    q('[data-update-check]').disabled=working||status==='ready';
    q('[data-update-check]').textContent=status==='checking'?'检查中…':'检查更新';
    q('[data-update-automatic]').checked=preferences.automatic;
    q('[data-update-automatic]').disabled=working;
    q('[data-update-network]').value=preferences.network;
    q('[data-update-network]').disabled=working;
    q('[data-update-checked]').textContent=state.checked?`上次检查：${new Date(state.checked*1000).toLocaleString('zh-CN')}`:'尚未检查更新';
    q('[data-update-info]').hidden=!release;
    if(release){
      const date=new Date(release.publishedAt);
      q('[data-update-info]').textContent=`版本 ${release.version}${Number.isNaN(date.getTime())?'':` · ${date.toLocaleDateString('zh-CN')}`}${release.size?` · ${formatBytes(release.size)}`:''}${release.signed?'':' · 需从发布页面手动下载'}`;
    }
    const downloading=['downloading','verifying'].includes(status);
    q('[data-update-details]').hidden=!release&&!downloading;
    q('[data-update-progress]').hidden=!downloading;
    q('[data-update-progress-text]').hidden=!downloading;
    q('[data-update-progress]').value=release?.size?Math.min(100,state.downloaded/release.size*100):0;
    q('[data-update-progress-text]').textContent=`${formatBytes(state.downloaded)} / ${formatBytes(release?.size||0)}`;
    q('[data-update-notes-box]').hidden=!release?.notes;
    q('[data-update-notes]').textContent=release?.notes||'';
    q('[data-update-download]').hidden=!release?.signed||!['available','error'].includes(status);
    q('[data-update-download]').disabled=working;
    q('[data-update-install]').hidden=status!=='ready';
    q('[data-update-install]').disabled=working;
    q('[data-update-cancel]').hidden=status!=='downloading';
    q('[data-update-release]').hidden=!release;
    q('[data-update-skip]').hidden=!release||state.skipped||working;
    if(status!=='ready')q('[data-update-confirm]').hidden=true;
    banner.hidden=!release||state.skipped||!['available','ready'].includes(status);
    banner.textContent=status==='ready'?`版本 ${release?.version} 已下载，点击前往更新`:`发现新版本 ${release?.version}，点击查看`;
  }
  async function run(command,args={}){
    if(pending)return;
    pending=true;render();
    try{const result=await call(command,args);if(result)state=result;}
    catch(error){if(state)state={...state,status:'error',message:errorText(error)};else q('[data-update-status]').textContent=errorText(error);}
    finally{pending=false;render();}
  }
  q('[data-update-check]').addEventListener('click',()=>run('check_update'));
  q('[data-update-download]').addEventListener('click',()=>run('download_update'));
  q('[data-update-cancel]').addEventListener('click',async()=>{q('[data-update-cancel]').disabled=true;try{await call('cancel_update');}catch(error){q('[data-update-status]').textContent=errorText(error);}finally{q('[data-update-cancel]').disabled=false;}});
  q('[data-update-release]').addEventListener('click',async()=>{try{await call('open_update_release');}catch(error){q('[data-update-status]').textContent=errorText(error);}});
  q('[data-update-skip]').addEventListener('click',()=>run('skip_update'));
  q('[data-update-install]').addEventListener('click',()=>{q('[data-update-confirm]').hidden=false;q('[data-update-confirm-install]').focus();});
  q('[data-update-later]').addEventListener('click',()=>{q('[data-update-confirm]').hidden=true;});
  q('[data-update-confirm-install]').addEventListener('click',()=>{q('[data-update-confirm]').hidden=true;run('install_update');});
  for(const name of ['automatic','network'])q(`[data-update-${name}]`).addEventListener('change',event=>{
    if(!state)return;
    const value=name==='automatic'?event.target.checked:event.target.value;
    run('set_update_preferences',{preferences:{...state.preferences,[name]:value}});
  });
  call('get_update_state').then(result=>{state=result;render();}).catch(error=>{q('[data-update-status]').textContent=errorText(error);q('[data-update-check]').disabled=true;});
  listen('update-state',event=>{state=event.payload;render();}).catch(()=>{});
}
