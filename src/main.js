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

const root = document.getElementById('ruc-prototype');
const q = selector => root.querySelector(selector);
const qa = selector => [...root.querySelectorAll(selector)];
q('[data-app-version]').textContent=packageInfo.version;
if(isTauri())getVersion().then(version=>{q('[data-app-version]').textContent=version;}).catch(()=>{});
const state = {page:'overview', snapshot:null, preferences:null, error:'', busy:false};
let toastTimer, pending, previousFocus, snapshotRequest=0;
let accountOperation=false;
let namingDevice=null,namingBusy=false,namePreviousFocus;
let deviceFocus='';
const traffic=setupTraffic(root,call,openDeviceName,showDevice,acceptTrafficDevices);
setupUpdates(root,call,listen,changePage);

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
    const row=node('div','ruc-device-preview');
    row.dataset.mac=normalizeMac(device.mac);if(deviceFocus===row.dataset.mac)row.classList.add('ruc-device-focused');
    const icon=node('div','ruc-device-icon');const symbol=node('i');symbol.dataset.lucide=/ios|android|iphone/i.test(device.os||'')?'smartphone':'laptop';symbol.setAttribute('aria-hidden','true');icon.append(symbol);
    const info=node('div','ruc-device-info');const title=node('strong','',device.name||device.os||'未命名设备');
    if(device.current)title.append(node('span','ruc-current','本机'));
    info.append(title,node('small','',[device.os,device.ip||device.ip6].filter(Boolean).join(' · ')||'—'));
    if(editable)info.append(node('small','ruc-device-mac',`MAC · ${formatMac(device.mac)}`));
    if(editable&&device.since)info.append(node('small','',`${formatDate(device.since)} 接入`));
    if(editable&&device.lastRecord)info.append(node('small','',`最近结束会话 · ${formatDate(device.lastRecord)}`));
    const actions=node('div','ruc-device-actions');const primaryActions=node('div','ruc-device-primary-actions'),secondaryActions=node('div','ruc-device-secondary-actions');const online=isOnline(device.online);
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
  q('[data-online-badge]').textContent=state.error?'未连接':snapshot?(snapshot.authenticated?'在线':'待登录'):'检查中';
  q('[data-online-badge]').style.color=snapshot?.authenticated?'var(--ruc-green)':'var(--ruc-muted)';
  q('[data-status-detail]').replaceChildren(node('span','',state.error||snapshot?.message||'正在检查连接状态'));
  if(snapshot?.ip)q('[data-status-detail]').append(node('span','',`本机 IP · ${snapshot.ip}`));
  qa('[data-connect]').forEach(button=>{button.textContent=state.busy?'处理中…':snapshot?.authenticated?'断开连接':'连接网络';button.disabled=state.busy;button.classList.toggle('ruc-primary',!snapshot?.authenticated);});
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
  const autoText=prefs?.automaticError?`自动登录已暂停：${prefs.automaticError}`:prefs?.paused?'自动登录已暂停，点击连接可恢复':prefs?.settings.auto?(prefs.hasCredentials?'自动登录已开启':'请保存账号以启用自动登录'):'自动登录尚未开启';
  q('[data-auto-note]').textContent=autoText;
  qa('[data-setting]').forEach(input=>{input.checked=Boolean(prefs?.settings[input.dataset.setting]);input.disabled=!prefs||state.busy;});
  const auto=q('[data-setting="auto"]');
  auto.disabled=auto.disabled||!prefs?.hasCredentials;
  auto.checked=Boolean(prefs?.hasCredentials&&prefs?.settings.auto);
  q('[data-auto-description]').textContent=prefs?.hasCredentials?'接入校园网后，使用保存的账号登录':'请先在“管理账号”中保存账号和密码';
  q('[data-theme]').value=prefs?.settings.theme||'system';
  q('[data-theme]').disabled=!prefs||state.busy;
  if(prefs)applyTheme(prefs.settings.theme);
  q('[data-profile-account]').textContent=prefs?.username||snapshot?.username||'尚未保存账号';
  q('[data-account-label]').textContent=prefs?.hasCredentials?`${prefs.username} · 密码已加密保存在本地`:'尚未保存账号';
  q('[data-account]').disabled=!prefs||state.busy;
}

async function preferences(){state.preferences=await call('get_preferences');render();}
async function refresh({silent=false}={}){
  if(state.busy)return;
  const request=++snapshotRequest;
  try{
    const snapshot=await call('get_snapshot');if(request!==snapshotRequest)return;
    changedAccount(snapshot.username);
    state.snapshot=snapshot;state.error='';
    if(!silent)toast(snapshot.warnings?.length?'状态已更新，设备查询暂不可用':'数据已更新');
  }catch(error){if(request!==snapshotRequest)return;if(state.snapshot)state.snapshot={...state.snapshot,authenticated:false,usedBytes:null,remainBytes:null,packageRemainBytes:null,balance:null,warnings:[message(error)],devices:state.snapshot.devices.map(d=>({...d,online:false,current:false,deviceStatus:'unknown'}))};traffic.invalidate();state.error=message(error);if(!silent)toast(state.error);}
  render();
}

async function operate(command,args={}){
  if(state.busy)return;
  state.busy=true;++snapshotRequest;render();
  try{
    const snapshot=await call(command,args);changedAccount(snapshot.username);state.snapshot=snapshot;state.error='';
    await preferences();toast(command==='connect_network'?'校园网登录成功':command==='disconnect_network'?'已断开，自动登录已暂停':'设备下线请求已完成');
  }catch(error){toast(message(error));try{await preferences();}catch{} }
  finally{state.busy=false;render();}
}

function changePage(page){state.page=page;render();if(page==='devices'||page==='ignored-devices'){q('.ruc-main').scrollTop=0;q(`[data-panel="${page}"] .ruc-device-focused`)?.scrollIntoView({block:'nearest'});}if(page==='records'){traffic.show();traffic.load();}}
function confirmAction(kind,device){
  pending={kind,device};previousFocus=document.activeElement;q('[data-dialog]').hidden=false;
  q('#ruc-dialog-title').textContent=kind==='disconnect'?'断开校园网？':`将“${device.name||device.os||'此设备'}”下线？`;
  q('[data-dialog-copy]').textContent=kind==='disconnect'?'断开后暂停自动登录。再次点击连接，或重新开启自动登录后恢复。':'该设备的校园网连接将中断。请确认这是你要下线的设备。';
  q('[data-confirm]').textContent=kind==='disconnect'?'确认断开':'确认下线';q('[data-cancel]').focus();
}
function closeDialog(){q('[data-dialog]').hidden=true;pending=null;previousFocus?.focus();}
function accountPasswordHint(){
  const form=q('.ruc-account-form');
  const saved=state.preferences?.hasCredentials;
  const sameAccount=saved&&form.elements.username.value.trim()===state.preferences.username;
  form.elements.password.required=!sameAccount;
  form.elements.password.placeholder=sameAccount?'已配置密码（留空保留）':'请输入密码';
  q('[data-password-hint]').textContent=sameAccount?'已配置密码。留空保留原密码；输入新密码并保存将覆盖原密码。':saved?'学工号已更改，请输入该账号的密码。':'尚未配置密码，请输入密码后保存。';
}
function openAccount(){previousFocus=document.activeElement;q('[data-account-dialog]').hidden=false;const form=q('.ruc-account-form');form.elements.username.value=state.preferences?.username||'';form.elements.password.value='';accountPasswordHint();q('[data-account-error]').textContent='';q('[data-forget]').hidden=!state.preferences?.hasCredentials;form.elements.username.focus();}
function closeAccount(){if(accountOperation)return;q('[data-account-dialog]').hidden=true;q('.ruc-account-form').elements.password.value='';previousFocus?.focus();}

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
  if(state.snapshot?.authenticated)confirmAction('disconnect');else if(!state.preferences?.hasCredentials)openAccount();else operate('connect_network');
}));
q('[data-cancel]').addEventListener('click',closeDialog);
q('[data-confirm]').addEventListener('click',()=>{const action=pending;closeDialog();if(action?.kind==='disconnect')operate('disconnect_network');else if(action?.kind==='drop')operate('drop_device',{id:numericId(action.device.id)});});
q('[data-account]').addEventListener('click',openAccount);q('[data-account-cancel]').addEventListener('click',closeAccount);
q('.ruc-account-form').elements.username.addEventListener('input',accountPasswordHint);
q('.ruc-account-form').addEventListener('submit',async event=>{
  event.preventDefault();if(accountOperation)return;accountOperation=true;
  const form=event.currentTarget;const username=form.elements.username.value.trim();const password=form.elements.password.value;
  qa('.ruc-account-form button').forEach(button=>button.disabled=true);q('[data-account-error]').textContent='';
  try{state.preferences=await call('save_account',{username,password});accountOperation=false;closeAccount();render();toast('账号已安全保存。现在可以连接网络或开启自动登录。');}
  catch(error){q('[data-account-error]').textContent=message(error);}
  finally{accountOperation=false;form.elements.password.value='';qa('.ruc-account-form button').forEach(button=>button.disabled=false);}
});
q('[data-forget]').addEventListener('click',async()=>{
  if(accountOperation)return;accountOperation=true;qa('.ruc-account-form button').forEach(button=>button.disabled=true);
  try{state.preferences=await call('forget_account');accountOperation=false;closeAccount();render();toast('已删除本机凭据并关闭自动登录');}catch(error){q('[data-account-error]').textContent=message(error);}finally{accountOperation=false;qa('.ruc-account-form button').forEach(button=>button.disabled=false);}
});
qa('[data-setting]').forEach(input=>input.addEventListener('change',async()=>{
  if(!state.preferences||state.busy)return;
  if(input.dataset.setting==='auto'&&input.checked&&!state.preferences.hasCredentials){render();toast('请先在“管理账号”中保存账号和密码');return;}
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
document.addEventListener('keydown',event=>{
  const modal=!q('[data-device-name-dialog]').hidden?q('[data-device-name-dialog]'):!q('[data-account-dialog]').hidden?q('[data-account-dialog]'):!q('[data-dialog]').hidden?q('[data-dialog]'):null;
  if(!modal){if(event.key==='Escape')clearDeviceFocus();return;}
  if(event.key==='Escape'){event.preventDefault();modal===q('[data-device-name-dialog]')?closeDeviceName():modal===q('[data-dialog]')?closeDialog():closeAccount();}
  if(event.key==='Tab'){const controls=[...modal.querySelectorAll('button,input')].filter(el=>!el.disabled&&!el.hidden);const first=controls[0],last=controls.at(-1);if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}}
});

for(let index=0;index<12;index++){const date=new Date();date.setDate(1);date.setMonth(date.getMonth()-index);const option=node('option','',`${date.getFullYear()} 年 ${date.getMonth()+1} 月`);option.value=`${date.getFullYear()}-${date.getMonth()+1}`;q('[data-month]').append(option);}
createIcons({icons});render();
if(isTauri()){
  listen('network-snapshot',event=>{if(state.busy||namingBusy)return;++snapshotRequest;changedAccount(event.payload.username);state.snapshot=event.payload;state.error='';render();});
  listen('network-attention',async event=>{toast(event.payload);try{await preferences();}catch{}});
}
preferences().catch(error=>{state.error=message(error);render();});refresh({silent:true});
setInterval(()=>{if(!document.hidden)refresh({silent:true});},60000);
