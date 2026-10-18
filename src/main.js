import {createIcons, icons} from 'lucide';
import {invoke, isTauri} from '@tauri-apps/api/core';
import {getVersion} from '@tauri-apps/api/app';
import {calculateAllowance,formatAllowance} from './allowance.js';
import packageInfo from '../package.json';
import {listen} from '@tauri-apps/api/event';
import {formatBytes, formatDate, formatDuration, isOnline, numericId, formatMac} from './format.js';
import './style.css';
import {applyTheme} from './theme.js';
import {setupTraffic} from './traffic-view.js';
import {normalizeMac} from './traffic.js';
import {setupUpdates} from './update-view.js';
import {setupAccounts} from './accounts.js';
import {setupPrivacy} from './privacy-view.js';

const root = document.getElementById('ruc-prototype');
const q = selector => root.querySelector(selector);
const qa = selector => [...root.querySelectorAll(selector)];
q('[data-app-version]').textContent=packageInfo.version;
if(isTauri())getVersion().then(version=>{q('[data-app-version]').textContent=version;}).catch(()=>{});
const state = {page:'overview', snapshot:null, preferences:null, error:'', busy:false};
let toastTimer, pending, previousFocus, snapshotRequest=0, preferencesRequest=0;
let namingDevice=null,namingBusy=false,namePreviousFocus;
let deviceFocus='',clearingData=false;
const traffic=setupTraffic(root,call,openDeviceName,showDevice,acceptTrafficDevices);
setupUpdates(root,call,listen,changePage);
const accounts=setupAccounts(root,{state,call,render,toast,operate,changePage,refresh});
const privacy=setupPrivacy(root,call,toast);

function acceptTrafficDevices(result){
  if(!state.snapshot||state.snapshot.username!==result.username)return;
  state.snapshot.devices=result.devices;state.snapshot.deviceMonths=result.deviceMonths||[];render();
}
function showDevice(mac){deviceFocus=normalizeMac(mac);q('[data-device-filter]').value='all';changePage(state.snapshot?.devices.some(d=>normalizeMac(d.mac)===deviceFocus&&d.ignored)?'ignored-devices':'devices');}
function showDeviceRecords(mac){traffic.selectDevice(normalizeMac(mac));changePage('records');}
function clearDeviceFocus(){deviceFocus='';qa('.ruc-device-focused').forEach(row=>row.classList.remove('ruc-device-focused'));}
async function setDeviceIgnored(device,ignored){
  if(state.busy)return;
  const username=state.snapshot?.username||'';
  state.busy=true;++snapshotRequest;traffic.invalidate(true);render();
  try{
    const snapshot=await call('set_device_ignored',{mac:device.mac,ignored,username});
    ++snapshotRequest;changedAccount(snapshot.username);state.snapshot=snapshot;state.error='';
    toast(ignored?'已忽略设备，可在“查看已忽略设备”中恢复':'设备已恢复显示');
  }catch(error){toast(message(error));}
  finally{state.busy=false;render();}
}
function changedAccount(username){
  if(state.snapshot?.username!==username){traffic.invalidate();deviceFocus='';}
}

async function call(command,args={}) {
  if(!isTauri()) throw new Error('请使用桌面应用运行，此页面无法直接访问校园网接口');
  return invoke(command,args);
}
function message(error){ return typeof error==='string' ? error : error?.message || '操作失败，请稍后重试'; }
function toast(text){ q('.ruc-toast').textContent=text;q('.ruc-toast').hidden=false;clearTimeout(toastTimer);toastTimer=setTimeout(()=>q('.ruc-toast').hidden=true,5000); }
function node(tag, className, text){const element=document.createElement(tag);if(className)element.className=className;if(text!==undefined)element.textContent=text;return element;}

function renderDevices(container,devices,editable,ignoredView=false){
  container.replaceChildren();
  if(!devices.length){container.append(node('p','ruc-faint-note',state.snapshot?(ignoredView?'暂无已忽略设备':'暂无可展示的设备'):'尚未读取设备数据'));return;}
  devices.forEach(device=>{
    const online=isOnline(device.online);
    const row=node('div','ruc-device-preview');
    row.dataset.mac=normalizeMac(device.mac);if(deviceFocus===row.dataset.mac)row.classList.add('ruc-device-focused');
    const icon=node('div','ruc-device-icon');const symbol=node('i');symbol.dataset.lucide=/ios|android|iphone/i.test(device.os||'')?'smartphone':'laptop';symbol.setAttribute('aria-hidden','true');icon.append(symbol);
    const info=node('div','ruc-device-info');const title=node('strong','',device.name||device.os||'未命名设备');
    if(device.current)title.append(node('span','ruc-current','本机'));
    info.append(title);
    if(editable){
      const detail=(label,value,monospace=false)=>{
        const line=node('small','ruc-device-detail');line.setAttribute('aria-label',`${label}：${value}`);
        const separator=node('span','ruc-device-detail-separator','·');separator.setAttribute('aria-hidden','true');
        line.append(node('span','ruc-device-detail-label',label),separator,node('span',`ruc-device-detail-value${monospace?' ruc-device-mac':''}`,value));
        return line;
      };
      info.append(detail('IP',device.ip||device.ip6||'—'));
      info.append(detail('MAC',formatMac(device.mac),true));
      if(!online&&device.deviceStatus!=='unknown')info.append(detail('上次在线',Number(device.lastRecord)>0?formatDate(device.lastRecord):'暂无记录'));
    }else info.append(node('small','',[device.os,device.ip||device.ip6].filter(Boolean).join(' · ')||'—'));
    const actions=node('div','ruc-device-actions');const primaryActions=node('div','ruc-device-primary-actions'),secondaryActions=node('div','ruc-device-secondary-actions');
    const status=node('span','ruc-device-status',({online:'在线',offline:'离线',historical:'历史设备',unknown:'状态未更新'})[device.deviceStatus]||(online?'在线':'离线'));if(!online)status.style.color='var(--ruc-muted)';actions.append(status);
    if(device.deviceStatus==='historical')status.title='有历史记录或本地档案，当前设备列表中未找到';
    if(editable&&formatMac(device.mac)!=='—'){
      const rename=node('button','ruc-text-button','命名');rename.type='button';rename.dataset.renameDevice='';rename.disabled=state.busy;
      rename.addEventListener('click',()=>openDeviceName({...device,username:state.snapshot?.username}));primaryActions.append(rename);
      const records=node('button','ruc-text-button','查看统计');records.type='button';records.addEventListener('click',()=>showDeviceRecords(device.mac));primaryActions.prepend(records);
      const visibility=node('button','ruc-text-button',ignoredView?'恢复':'忽略');visibility.type='button';visibility.dataset.deviceVisibility='';visibility.disabled=state.busy;
      visibility.addEventListener('click',()=>setDeviceIgnored(device,!ignoredView));(ignoredView?primaryActions:secondaryActions).append(visibility);
    }
    if(editable&&online&&!ignoredView){
      const button=node('button','ruc-text-button cursor-interaction',device.current?'断开连接':'下线');button.type='button';button.disabled=state.busy;
      const id=numericId(device.id);
      if(!device.current&&id===null)button.disabled=true;
      button.addEventListener('click',()=>device.current?confirmAction('disconnect'):confirmAction('drop',device));secondaryActions.append(button);
    }
    actions.append(primaryActions,secondaryActions);
    row.append(icon,info,actions);container.append(row);
  });
  createIcons({icons});
}

function render(){
  qa('[data-panel]').forEach(el=>el.hidden=el.dataset.panel!==state.page);
  qa('[data-page]').forEach(el=>{if(el.dataset.page===(state.page==='ignored-devices'?'devices':state.page))el.setAttribute('aria-current','page');else el.removeAttribute('aria-current');});
  const snapshot=state.snapshot, prefs=state.preferences;
  const title=state.error?'无法读取校园网状态':snapshot?(snapshot.authenticated?'已连接校园网':'尚未登录校园网'):'正在检查校园网';
  q('[data-status-title]').textContent=title;
  q('[data-online-badge]').textContent=state.error?'未知':snapshot?(snapshot.authenticated?'在线':'待登录'):'检查中';
  q('[data-online-badge]').style.color=snapshot?.authenticated?'var(--ruc-green)':'var(--ruc-muted)';
  q('[data-status-detail]').replaceChildren(node('span','',state.error||snapshot?.message||'正在检查连接状态'));
  q('[data-status-detail]').hidden=!state.error&&(!snapshot||snapshot.authenticated||snapshot.status==='offline');
  qa('[data-connect]').forEach(button=>{button.textContent=state.busy?'处理中…':snapshot?.authenticated?'断开连接':'连接网络';button.disabled=state.busy||!prefs||(!snapshot?.authenticated&&!prefs?.hasCredentials);button.hidden=Boolean(prefs&&!prefs.hasCredentials&&!snapshot?.authenticated);button.classList.toggle('ruc-primary',!snapshot?.authenticated);});
  qa('[data-refresh]').forEach(button=>button.disabled=state.busy);
  const allowance=calculateAllowance(snapshot?.usedBytes,snapshot?.balance);
  q('[data-remain]').textContent=formatAllowance(allowance.remaining);
  q('[data-used]').textContent=formatAllowance(allowance.used);
  q('[data-free]').textContent=formatAllowance(allowance.free);
  q('[data-paid]').textContent=formatAllowance(allowance.paid);
  q('[data-balance]').textContent=typeof snapshot?.balance==='number'?`¥ ${snapshot.balance.toFixed(2)}`:'—';
  q('[data-billing-cycle]').textContent=snapshot?.checkoutDate?`结算日 ${formatDate(snapshot.checkoutDate,true)}`:'—';
  qa('[data-quota-segment]').forEach((segment,index)=>{segment.style.width=`${allowance.shares?.[index]||0}%`;});
  q('.ruc-meter').setAttribute('aria-label',allowance.shares?`已用流量 ${formatAllowance(allowance.used)}，剩余免费流量 ${formatAllowance(allowance.free)}，剩余收费流量 ${formatAllowance(allowance.paid)}`:'流量数据暂不可用');
  q('.ruc-meter').hidden=!allowance.shares;
  const allDevices=snapshot?.devices||[],devices=allDevices.filter(d=>!d.ignored),ignored=allDevices.filter(d=>d.ignored),online=devices.filter(d=>isOnline(d.online));
  q('[data-device-number]').textContent=snapshot&&!snapshot.warnings?.length?online.length:'—';q('.ruc-count').textContent=snapshot&&!snapshot.warnings?.length?`在线 ${online.length}`:'—';
  const filter=q('[data-device-filter]').value;
  renderDevices(q('[data-overview-devices]'),online,false);renderDevices(q('[data-all-devices]'),devices.filter(d=>filter==='all'||(filter==='online'?isOnline(d.online):d.deviceStatus==='historical')),true);
  q('[data-device-summary]').textContent=`已发现 ${devices.length} 个设备 · ${snapshot?.warnings?.length?'在线状态未更新':`在线 ${online.length} 个`}`;
  q('[data-view-ignored]').textContent=`查看已忽略设备（${ignored.length}）`;
  renderDevices(q('[data-ignored-devices]'),ignored,true,true);
  const warning=snapshot?.warnings?.join('；')||'';
  q('[data-device-error]').textContent=warning;
  if(warning)q('[data-overview-devices]').replaceChildren(node('p','ruc-faint-note',warning));
  qa('[data-setting]').forEach(input=>{input.checked=Boolean(prefs?.settings[input.dataset.setting]);input.disabled=!prefs||state.busy;});
  q('[data-theme]').value=prefs?.settings.theme||'system';
  q('[data-theme]').disabled=!prefs||state.busy;
  if(prefs)applyTheme(prefs.settings.theme);
  accounts.render();
}

async function preferences(){const request=++preferencesRequest;const prefs=await call('get_preferences');if(request!==preferencesRequest)return;state.preferences=prefs;render();accounts.afterPreferences();}
async function refresh({silent=false}={}){
  if(state.busy)return;
  const request=++snapshotRequest;
  try{
    const snapshot=await call('get_snapshot');if(request!==snapshotRequest)return;
    changedAccount(snapshot.username);
    state.snapshot=snapshot;state.error='';
    render();await preferences();if(request!==snapshotRequest)return;
    if(!silent)toast(snapshot.warnings?.length?'状态已更新，设备查询暂不可用':'数据已更新');
  }catch(error){if(request!==snapshotRequest)return;if(state.snapshot)state.snapshot={...state.snapshot,authenticated:false,usedBytes:null,remainBytes:null,packageRemainBytes:null,balance:null,warnings:[message(error)],devices:state.snapshot.devices.map(d=>({...d,online:false,current:false,deviceStatus:'unknown'}))};traffic.invalidate();state.error=message(error);if(!silent)toast(state.error);}
  render();
}

async function operate(command,args={}){
  if(state.busy)return;
  state.busy=true;++snapshotRequest;
  if(command==='switch_account'){traffic.invalidate();deviceFocus='';state.snapshot=null;state.error='';}
  render();
  try{
    const snapshot=await call(command,args);changedAccount(snapshot.username);state.snapshot=snapshot;state.error='';
    await preferences();toast(command==='switch_account'?'账号已切换，已确认网络在线身份':command==='connect_network'?'校园网登录成功':command==='disconnect_network'?'已下线，将保持离线':'设备下线请求已完成');
  }catch(error){toast(message(error));
    try{const actual=await call('get_snapshot');changedAccount(actual.username);state.snapshot=actual;state.error='';}catch{state.snapshot=null;state.error='暂时无法确认网络状态，请刷新后重试';}
    try{await preferences();}catch{}
  }
  finally{state.busy=false;render();}
}

function changePage(page){state.page=page;render();if(page==='devices'||page==='ignored-devices'){q('.ruc-main').scrollTop=0;q(`[data-panel="${page}"] .ruc-device-focused`)?.scrollIntoView({block:'nearest'});}if(page==='records'){traffic.show();traffic.load();}}
function confirmAction(kind,device){
  pending={kind,device};previousFocus=document.activeElement;q('[data-dialog]').hidden=false;
  q('#ruc-dialog-title').textContent=kind==='disconnect'?'断开校园网？':`将“${device.name||device.os||'此设备'}”下线？`;
  q('[data-dialog-copy]').textContent=kind==='disconnect'?'下线后保持离线。你可以随时手动登录；下次启动助手时，再按启动设置登录。':'该设备的校园网连接将中断。请确认这是你要下线的设备。';
  q('[data-confirm]').textContent=kind==='disconnect'?'确认断开':'确认下线';q('[data-cancel]').focus();
}
function closeDialog(){q('[data-dialog]').hidden=true;pending=null;previousFocus?.focus();}
function openDeviceName(device){
  if(namingBusy)return;
  namingDevice={...device};namePreviousFocus=document.activeElement;
  q('[data-device-name-dialog]').hidden=false;q('[data-device-name-mac]').textContent=`MAC · ${formatMac(device.mac)}`;
  const input=q('.ruc-device-name-form').elements.deviceName;
  input.value=device.customName||device.name||'';q('[data-device-name-error]').textContent='';input.focus();input.select();
}
function closeDeviceName(){if(namingBusy)return;q('[data-device-name-dialog]').hidden=true;namingDevice=null;namePreviousFocus?.focus();}
async function saveDeviceName(name){
  if(namingBusy||!namingDevice)return;
  namingBusy=true;++snapshotRequest;qa('.ruc-device-name-form button').forEach(b=>b.disabled=true);q('[data-device-name-error]').textContent='';
  try{
    const result=await call('set_device_name',{mac:namingDevice.mac,name,username:namingDevice.username||''});
    ++snapshotRequest;state.snapshot=result.snapshot;state.error='';traffic.invalidate(true);render();namingBusy=false;closeDeviceName();
    if(state.page==='records')traffic.load();toast(name.trim()?'设备名称已保存':'已恢复默认名称');
  }catch(error){q('[data-device-name-error]').textContent=message(error);}
  finally{namingBusy=false;qa('.ruc-device-name-form button').forEach(b=>b.disabled=false);}
}
q('.ruc-device-name-form').addEventListener('submit',event=>{event.preventDefault();saveDeviceName(event.currentTarget.elements.deviceName.value)});
q('[data-device-name-cancel]').addEventListener('click',closeDeviceName);
q('[data-device-name-reset]').addEventListener('click',()=>saveDeviceName(''));

qa('[data-page]').forEach(button=>button.addEventListener('click',()=>changePage(button.dataset.page)));
qa('[data-goto]').forEach(button=>button.addEventListener('click',()=>changePage(button.dataset.goto)));
qa('[data-refresh]').forEach(button=>button.addEventListener('click',()=>refresh()));
q('[data-device-filter]').addEventListener('change',render);
q('[data-traffic-refresh]').addEventListener('click',()=>traffic.load());
q('[data-view-ignored]').addEventListener('click',()=>changePage('ignored-devices'));
q('[data-return-devices]').addEventListener('click',()=>changePage('devices'));
qa('[data-connect]').forEach(button=>button.addEventListener('click',()=>{
  if(state.snapshot?.authenticated)confirmAction('disconnect');else if(!state.preferences?.hasCredentials)changePage('settings');else operate('connect_network');
}));
q('[data-cancel]').addEventListener('click',closeDialog);
q('[data-confirm]').addEventListener('click',()=>{const action=pending;closeDialog();if(action?.kind==='disconnect')operate('disconnect_network');else if(action?.kind==='drop')operate('drop_device',{id:numericId(action.device.id)});});
qa('[data-setting]').forEach(input=>input.addEventListener('change',async()=>{
  if(!state.preferences||state.busy)return;
  const next={...state.preferences.settings,[input.dataset.setting]:input.checked};state.busy=true;render();
  try{state.preferences=await call('set_preferences',{settings:next});toast('设置已保存');}catch(error){toast(message(error));}finally{state.busy=false;render();}
}));
q('[data-theme]').addEventListener('change',async event=>{
  if(!state.preferences||state.busy)return;
  const theme=event.target.value;
  const next={...state.preferences.settings,theme};
  state.busy=true;render();applyTheme(theme);q('[data-theme]').value=theme;
  try{state.preferences=await call('set_preferences',{settings:next});toast('外观设置已保存');}
  catch(error){toast(message(error));}
  finally{state.busy=false;render();}
});
q('[data-official]').addEventListener('click',async()=>{try{await call('open_official');}catch(error){toast(message(error));}});
q('[data-month]').addEventListener('change',()=>traffic.load());
q('.ruc-main').addEventListener('click',event=>{
  if(!deviceFocus||!['devices','ignored-devices'].includes(state.page))return;
  if(event.target.closest('.ruc-device-preview,button,a,input,select,textarea,label,summary,[role="button"]'))return;
  clearDeviceFocus();
});
function closeClearDialog(){if(clearingData)return;q('[data-clear-dialog]').hidden=true;q('[data-clear-data]').focus();}
q('[data-clear-data]').addEventListener('click',()=>{if(state.busy)return;q('[data-clear-error]').textContent='';q('[data-clear-dialog]').hidden=false;q('[data-clear-cancel]').focus();});
q('[data-clear-cancel]').addEventListener('click',closeClearDialog);
q('[data-clear-confirm]').addEventListener('click',async()=>{
  if(clearingData||state.busy)return;clearingData=true;state.busy=true;render();
  qa('[data-clear-dialog] button').forEach(button=>button.disabled=true);q('[data-clear-confirm]').textContent='正在清除并退出…';
  try{await call('clear_all_data');}
  catch(error){q('[data-clear-error]').textContent=message(error);clearingData=false;state.busy=false;qa('[data-clear-dialog] button').forEach(button=>button.disabled=false);q('[data-clear-confirm]').textContent='清除并退出';render();}
});
document.addEventListener('keydown',event=>{
  const modal=q('.ruc-dialog-layer:not([hidden])');
  if(!modal){if(event.key==='Escape')clearDeviceFocus();return;}
  if(event.key==='Escape'){event.preventDefault();modal===q('[data-clear-dialog]')?closeClearDialog():modal===q('[data-privacy-dialog]')?privacy.close():modal===q('[data-device-name-dialog]')?closeDeviceName():modal===q('[data-dialog]')?closeDialog():accounts.closeActive();}
  if(event.key==='Tab'){if(clearingData){event.preventDefault();return;}const controls=[...modal.querySelectorAll('button,input,select,textarea,a[href],[tabindex="0"]')].filter(el=>!el.disabled&&!el.hidden&&el.getClientRects().length);const first=controls[0],last=controls.at(-1);if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}}
});

for(let index=0;index<12;index++){const date=new Date();date.setDate(1);date.setMonth(date.getMonth()-index);const option=node('option','',`${date.getFullYear()} 年 ${date.getMonth()+1} 月`);option.value=`${date.getFullYear()}-${date.getMonth()+1}`;q('[data-month]').append(option);}
createIcons({icons});render();
if(isTauri()){
  listen('network-snapshot',event=>{if(state.busy||namingBusy)return;++snapshotRequest;changedAccount(event.payload.username);state.snapshot=event.payload;state.error='';render();preferences().catch(()=>{});});
  listen('preferences-changed',()=>preferences().catch(()=>{}));
  listen('network-attention',async event=>{toast(event.payload);try{await preferences();}catch{}});
}
preferences().catch(error=>{state.error=message(error);render();});refresh({silent:true});
setInterval(()=>{if(!document.hidden)refresh({silent:true});},60000);
