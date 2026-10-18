export function setupAccounts(root,{state,call,render,toast,operate,changePage,refresh}) {
  const q=s=>root.querySelector(s),qa=s=>[...root.querySelectorAll(s)];
  const form=q('.ruc-account-form');
  let editing=null,passwordEditing=false,confirmation=null,working=false,introduced=false,previousFocus;
  const selected=()=>state.preferences?.accounts?.find(a=>a.id===state.preferences.selectedId);
  const message=e=>typeof e==='string'?e:e.message||'操作失败，请重试';
  const el=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!==undefined)e.textContent=text;return e;};
  function close(layer){if(working)return;layer.hidden=true;form.elements.password.value='';previousFocus?.focus();}
  function show(layer){previousFocus=document.activeElement;layer.hidden=false;layer.querySelector('input,button')?.focus();}
  function settings(){changePage('settings');q('.ruc-main').scrollTop=0;}
  function updatePasswordField(){
    const saved=state.preferences?.accounts?.find(a=>a.id===editing);
    const same=saved&&form.elements.username.value.trim()===saved.username;
    const unchanged=same&&!passwordEditing;
    form.elements.password.required=!unchanged;
    form.elements.password.placeholder=unchanged?'••••••':saved?'':'请输入数智人大密码';
    form.elements.password.classList.toggle('ruc-password-saved',Boolean(unchanged));
  }
  function open(id=null,username=''){
    if(state.busy||!state.preferences)return;
    editing=id;passwordEditing=false;
    const account=state.preferences.accounts?.find(a=>a.id===id);
    q('#account-title').textContent=account?'编辑校园账号':'添加校园账号';
    form.elements.accountName.value=account?.name||'';
    form.elements.username.value=account?.username||username;
    form.elements.password.value='';
    q('[data-forget]').hidden=!account;
    q('[data-account-error]').textContent='';updatePasswordField();show(q('[data-account-dialog]'));
  }
  function confirm(kind,account){
    if(state.busy)return;
    confirmation={kind,account};
    q('#account-confirm-title').textContent=kind==='reset'?'备份原文件并重新配置？':kind==='delete'?'删除此账号的本机凭据？':'切换并登录校园网？';
    q('[data-account-confirm-copy]').textContent=kind==='reset'?'会先将原账号文件完整备份为 account.enc.recovery-*，再清空助手的凭据配置。当前校园网连接不会改变；你需要重新添加账号密码。':kind==='delete'
      ?`将删除「${account.name||account.username}」保存的账号密码，不会注销当前网络连接。${account.id===state.preferences.automaticId?'删除后将使用剩余的第一个账号自动登录；没有剩余账号时不执行自动登录。':''}`
      :`将使用「${account.name||account.username}」(${account.username}) 登录。${state.snapshot?.authenticated?`本机当前在线账号 ${state.snapshot.username||'未知'} 将先下线，期间网络会短暂断开。`:''}`;
    q('[data-account-confirm-error]').textContent='';q('[data-account-confirm-submit]').textContent=kind==='reset'?'备份并重新配置':kind==='delete'?'删除本机凭据':'确认切换并登录';
    if(!q('[data-account-dialog]').hidden)close(q('[data-account-dialog]'));
    show(q('[data-account-confirm]'));
  }
  function renderAccounts(){
    const prefs=state.preferences,account=selected(),list=q('[data-account-list]');
    const online=!state.error&&Boolean(state.snapshot?.authenticated);
    const username=online?state.snapshot.username||'':'';
    const registered=prefs?.accounts?.find(a=>a.username===username);
    q('[data-account-label]').textContent=prefs?.accounts?.length?`${prefs.accounts.length} 个已保存账号 · 密码加密保存在本机`:'添加校园账号后，才能自动登录';
    q('[data-account]').textContent='添加账号';
    q('[data-account]').disabled=!prefs||state.busy||Boolean(prefs.credentialError);
    const automatic=q('[data-automatic-account]');
    q('[data-automatic-account-row]').hidden=!prefs?.accounts?.length;
    automatic.replaceChildren(...(prefs?.accounts||[]).map(saved=>{
      const option=el('option','',saved.name&&saved.name!==saved.username?`${saved.name} · ${saved.username}`:saved.username);
      option.value=saved.id;return option;
    }));
    automatic.value=prefs?.automaticId||'';automatic.disabled=!prefs||state.busy;
    const credentialError=q('[data-credential-error]');credentialError.hidden=!prefs?.credentialError;
    credentialError.textContent=prefs?.credentialError?`${prefs.credentialError}。原凭据已保留，请先备份用户数据目录中的 account.enc 并检查 Windows 用户或恢复备份。`:'';
    q('[data-account-recover]').hidden=!prefs?.credentialError;q('[data-account-recover]').disabled=state.busy;
    list.replaceChildren();
    for(const saved of prefs?.accounts||[]){
      const row=el('div','ruc-account-row');row.dataset.accountId=saved.id;
      const info=el('div','ruc-account-info'),title=el('div','ruc-account-row-title');
      title.append(el('strong','',saved.name||saved.username));
      if(online?saved.username===username:saved.id===prefs.selectedId)title.append(el('span','ruc-account-tag',online?'当前账号':'待登录'));
      if(!state.error&&state.snapshot?.authenticated&&state.snapshot.username===saved.username)title.append(el('span','ruc-account-tag ruc-account-online','网络在线'));
      const detail=el('small','',saved.username);
      if(saved.passwordError)detail.append(el('span','ruc-account-password-error','数智人大密码错误'));
      info.append(title,detail);
      const actions=el('div','ruc-account-actions'),edit=el('button','ruc-text-button','编辑');edit.type='button';edit.dataset.editAccount=saved.id;edit.disabled=state.busy;edit.addEventListener('click',()=>open(saved.id));actions.append(edit);
      if(!online||saved.username!==username){
        const connect=el('button','ruc-button','登录');connect.type='button';connect.dataset.switchAccount=saved.id;connect.disabled=state.busy;connect.addEventListener('click',()=>confirm('switch',saved));actions.append(connect);
      }else actions.append(el('span','ruc-account-using','正在使用'));
      row.append(info,actions);list.append(row);
    }
    if(prefs&&!prefs.accounts?.length&&!prefs.credentialError)list.append(el('p','ruc-account-empty','尚未配置校园账号'));
    q('[data-account-caption]').textContent=!state.error&&state.snapshot&&!online&&account?'待登录账号':'当前账号';
    q('[data-network-account]').textContent=state.error?'暂时无法确认':!state.snapshot?'正在检查':online?username||'在线身份未知':account?.username||'未登录';
    const name=q('[data-network-name]');name.hidden=!registered?.name||registered.name===username;name.textContent=registered?.name||'';
    const ip=q('[data-network-ip]');ip.hidden=Boolean(state.error)||!state.snapshot?.ip;ip.textContent=state.snapshot?.ip?`IP ${state.snapshot.ip}`:'';
    const notice=q('[data-account-notice]');let text='';
    if(prefs?.credentialError)text='助手无法读取本机凭据。网络状态与凭据读取相互独立，请在设置中查看原因。';
    else if(prefs&&!prefs.accounts?.length)text=online?'网络已登录，助手尚未配置账号。记录当前账号和密码后，可使用自动登录。':'尚未配置账号，请设置账号和密码以使用登录功能。';
    else if(prefs&&online&&username&&!registered)text='当前在线账号尚未登记，可记录账号和密码以便自动登录。';
    else if(prefs&&!online&&state.snapshot&&!state.error&&!account)text='请在设置中选择一个已登记账号登录。';
    notice.hidden=!text;notice.textContent=text;
    const button=q('[data-account-settings]');const record=online&&username&&!registered&&!prefs?.credentialError;
    button.textContent=prefs?.credentialError?'账号设置':record?'记录此账号':!prefs?.accounts?.length?'设置账号':!online&&!account?'选择账号':'管理账号';button.disabled=!prefs||state.busy;
    button.className=record||!prefs?.accounts?.length?'ruc-button ruc-primary cursor-interaction':'ruc-text-button cursor-interaction';
  }
  async function dismissIntro(){
    if(working)return false;working=true;qa('[data-onboarding] button').forEach(b=>b.disabled=true);
    try{state.preferences=await call('set_preferences',{settings:{...state.preferences.settings,onboarding_done:true}});working=false;close(q('[data-onboarding]'));render();return true;}
    catch(error){q('[data-onboarding-error]').textContent=message(error);return false;}
    finally{working=false;qa('[data-onboarding] button').forEach(b=>b.disabled=false);}
  }
  function afterPreferences(){
    if(introduced||!state.preferences)return;introduced=true;
    if(!state.preferences.settings.onboarding_done&&!state.preferences.accounts?.length&&!state.preferences.credentialError)show(q('[data-onboarding]'));
  }
  q('[data-automatic-account]').addEventListener('change',async event=>{
    if(state.busy||!state.preferences)return;
    const id=event.target.value;state.busy=true;render();
    try{state.preferences=await call('set_automatic_account',{id});toast('默认自动登录账号已保存');}
    catch(error){toast(message(error));}
    finally{state.busy=false;render();}
  });
  q('[data-account]').addEventListener('click',()=>open());
  q('[data-account-recover]').addEventListener('click',()=>confirm('reset',null));
  q('[data-account-settings]').addEventListener('click',()=>{
    if(state.preferences?.credentialError){settings();return;}
    const username=!state.error&&state.snapshot?.authenticated?state.snapshot.username||'':'';
    if(!state.preferences?.accounts?.length||username&&!state.preferences.accounts.some(a=>a.username===username))open(null,username);
    else settings();
  });
  q('[data-account-cancel]').addEventListener('click',()=>close(q('[data-account-dialog]')));
  form.elements.username.addEventListener('input',updatePasswordField);
  form.elements.password.addEventListener('focus',()=>{
    if(editing&&!passwordEditing){passwordEditing=true;form.elements.password.value='';updatePasswordField();}
  });
  form.elements.password.addEventListener('input',()=>{passwordEditing=true;updatePasswordField();});
  form.addEventListener('submit',async event=>{
    event.preventDefault();if(working||state.busy)return;
    working=true;state.busy=true;qa('.ruc-account-form button,.ruc-account-form input').forEach(b=>b.disabled=true);q('[data-account-error]').textContent='';render();
    try{state.preferences=await call('save_account',{id:editing,name:form.elements.accountName.value,username:form.elements.username.value.trim(),password:form.elements.password.value});working=false;close(q('[data-account-dialog]'));toast('账号已安全保存，网络连接未更改。可手动登录或开启启动时自动登录。');}
    catch(error){q('[data-account-error]').textContent=message(error);}
    finally{working=false;state.busy=false;form.elements.password.value='';qa('.ruc-account-form button,.ruc-account-form input').forEach(b=>b.disabled=false);render();}
    if(q('[data-account-dialog]').hidden)await refresh({silent:true});
  });
  q('[data-forget]').addEventListener('click',()=>{const account=state.preferences.accounts.find(a=>a.id===editing);if(account)confirm('delete',account);});
  q('[data-account-confirm-cancel]').addEventListener('click',()=>close(q('[data-account-confirm]')));
  q('[data-account-confirm-submit]').addEventListener('click',async()=>{
    if(working||state.busy||!confirmation)return;const {kind,account}=confirmation;
    if(kind==='switch'){close(q('[data-account-confirm]'));await operate('switch_account',{id:account.id});return;}
    working=true;state.busy=true;qa('[data-account-confirm] button').forEach(b=>b.disabled=true);render();
    try{state.preferences=await call(kind==='reset'?'reset_accounts':'forget_account',kind==='reset'?{}:{id:account.id});working=false;close(q('[data-account-confirm]'));toast(kind==='reset'?'原文件已备份，可重新设置账号':'本机凭据已删除，网络连接保持不变');}
    catch(error){q('[data-account-confirm-error]').textContent=message(error);}
    finally{working=false;state.busy=false;qa('[data-account-confirm] button').forEach(b=>b.disabled=false);render();}
  });
  q('[data-onboarding-skip]').addEventListener('click',dismissIntro);
  q('[data-onboarding-start]').addEventListener('click',async()=>{if(await dismissIntro()){settings();open();}});
  return {render:renderAccounts,afterPreferences,closeActive(){
    if(!q('[data-onboarding]').hidden){dismissIntro();return;}
    for(const selector of ['[data-account-confirm]','[data-account-dialog]'])if(!q(selector).hidden){close(q(selector));return;}
  }};
}
