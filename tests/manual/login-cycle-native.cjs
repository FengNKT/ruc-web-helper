if(process.env.RUC_ALLOW_NETWORK_DISCONNECT!=='1')throw Error('This test disconnects the network. Set RUC_ALLOW_NETWORK_DISCONNECT=1 only after explicit authorization.');
const artifact=require('../../scripts/lib/artifacts.cjs');
// Explicit end-to-end repair verification: disconnect this machine once, then restore it.
// Credentials remain inside the app; never print usernames, passwords or request URLs.
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
let browser;
(async()=>{
  for(let i=0;i<40;i++){
    try{browser=await chromium.connectOverCDP('http://127.0.0.1:9227');break;}catch{await new Promise(r=>setTimeout(r,300));}
  }
  assert.ok(browser,'Application test endpoint must be ready');
  try{
    let page;
    for(let i=0;i<40;i++){
      page=browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri\.localhost|tauri:\/\//.test(p.url()));
      if(page)break;await new Promise(r=>setTimeout(r,250));
    }
    assert.ok(page);
    const result=await page.evaluate(async()=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const preferences=await invoke('get_preferences',{});
      if(!preferences.hasCredentials)throw Error('缺少已保存账号，无法自动恢复连接');
      let before=await invoke('get_snapshot',{});
      const initiallyAuthenticated=before.authenticated;
      if(before.username&&before.username!==preferences.username)throw Error('当前在线账号与保存账号不同，未执行断开测试');
      if(!before.authenticated)before=await invoke('connect_network',{});
      if(!before.authenticated)throw Error('初始连接未成功，未执行断开测试');
      let restore=false,disconnectConfirmed=false,reconnectConfirmed=false;
      try{
        // Restore even if the disconnect response fails after the operation took effect.
        restore=true;
        const offline=await invoke('disconnect_network',{});
        disconnectConfirmed=!offline.authenticated&&offline.status==='offline';
        if(!disconnectConfirmed)throw Error('学校未确认本机已经断开');
      }finally{
        if(restore){
          const online=await invoke('connect_network',{});
          reconnectConfirmed=online.authenticated&&online.username===preferences.username;
          if(!reconnectConfirmed)throw Error('自动恢复连接未成功，请使用官网恢复网络');
        }
      }
      return {initiallyAuthenticated,disconnectConfirmed,reconnectConfirmed};
    });
    assert.ok(result.disconnectConfirmed);assert.ok(result.reconnectConfirmed);
    console.log('PASS: real disconnect, offline confirmation, password authentication and connection restoration');
    console.log(JSON.stringify(result));
  }finally{if(browser)await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
