const artifact=require('../../scripts/lib/artifacts.cjs');
// Read-only live validation. Never invoke login, logout, credential or settings mutations.
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
let browser;
(async()=>{
  for(let i=0;i<30;i++){
    try{browser=await chromium.connectOverCDP('http://127.0.0.1:9227');break;}
    catch{await new Promise(resolve=>setTimeout(resolve,500));}
  }
  assert.ok(browser,'WebView2 must expose a local debugging endpoint');
  let page;
  for(let i=0;i<40;i++){
    page=browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri\.localhost|tauri:\/\//.test(p.url()));
    if(page)break;
    await new Promise(resolve=>setTimeout(resolve,250));
  }
  assert.ok(page,'Application window exists');
  assert.equal(await page.title(),'人大校园网助手');
  assert.equal(await page.locator('meta[name="description"]').getAttribute('content'),'RUC Web Helper');
  await page.locator('[data-status-title]').waitFor({state:'attached'});
  await page.waitForFunction(()=>!document.querySelector('[data-account]').disabled,{},{timeout:30000});
  await page.locator('[data-page="overview"]').click();
  assert.equal(await page.locator('.ruc-titlebar').count(),0);
  assert.equal(await page.locator('.ruc-brand').count(),0);
  assert.equal(await page.locator('.ruc-heading p').count(),0);
  assert.equal((await page.locator('body').textContent()).includes('模拟'),false);
  const result=await page.evaluate(async()=>{
    const snapshot=await window.__TAURI_INTERNALS__.invoke('get_snapshot',{});
    const preferences=await window.__TAURI_INTERNALS__.invoke('get_preferences',{});
    return {authenticated:snapshot.authenticated,hasIp:!!snapshot.ip,hasTraffic:typeof snapshot.usedBytes==='number',devices:snapshot.devices.length,visibleDevices:snapshot.devices.filter(d=>!d.ignored).length,warnings:snapshot.warnings,preferencesHavePassword:'password' in preferences};
  });
  assert.equal(result.hasIp,true);assert.equal(result.preferencesHavePassword,false);
  if(result.authenticated){assert.equal(result.hasTraffic,true);assert.deepEqual(result.warnings,[]);}
  await page.locator('[data-refresh]').first().click();
  await page.waitForFunction(()=>document.querySelector('.ruc-toast').textContent.includes('数据已更新')||document.querySelector('.ruc-toast').textContent.includes('状态已更新'),{},{timeout:30000});
  await page.locator('[data-page="devices"]').click();
  assert.ok(await page.locator('[data-all-devices] .ruc-device-preview').count()>=result.visibleDevices);
  await page.locator('[data-page="records"]').click();
  await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden,{},{timeout:30000});
  if(result.authenticated)assert.ok((await page.locator('[data-record-info]').textContent()).includes('共 '),'History API must succeed');
  if(result.authenticated){
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden,{},{timeout:60000});
    assert.equal(await page.locator('.ruc-traffic-svg').count(),1);
    assert.ok((await page.locator('[data-traffic-total]').textContent()).includes('GB'));
    assert.ok(await page.locator('[data-traffic-ranking] button').count()>0);
    await page.locator('[data-record-tab="day"]').click();
    await page.selectOption('[data-traffic-day]','10');
    assert.ok((await page.locator('[data-traffic-day-detail]').textContent()).includes('估算'));
    const all=await page.locator('[data-traffic-total]').textContent();
    const option=await page.locator('[data-traffic-device] option').nth(1).getAttribute('value');
    await page.selectOption('[data-traffic-device]',option);
    assert.notEqual(await page.locator('[data-traffic-total]').textContent(),all);
    await page.selectOption('[data-traffic-device]','all');
    await page.locator('[data-record-tab="history"]').click();
    assert.equal(await page.locator('#history-panel').isVisible(),true);
    await page.locator('[data-record-tab="traffic"]').click();
    for(const width of [1040,736,480,360,320]){
      await page.setViewportSize({width,height:1100});await page.waitForTimeout(60);
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    }
    await page.setViewportSize({width:1040,height:1100});
    await page.screenshot({path:artifact('traffic-statistics-desktop.png','manual'),fullPage:true});
  }
  await page.locator('[data-page="settings"]').click();
  const credentialsAvailable=await page.evaluate(async()=>(await window.__TAURI_INTERNALS__.invoke('get_preferences')).hasCredentials);
  assert.equal(await page.locator('[data-setting="auto"]').isDisabled(),!credentialsAvailable);
  assert.equal(await page.locator('[data-app-version]').textContent(),await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:app|version')));
  await page.locator('[data-account]').click();
  assert.equal(await page.locator('input[name="password"]').inputValue(),'');
  if(credentialsAvailable)assert.equal(await page.locator('input[name="password"]').getAttribute('placeholder'),'已配置密码（留空保留）');
  await page.locator('[data-account-cancel]').click();
  await page.locator('[data-page="overview"]').click();
  await page.screenshot({path:artifact('real-api-desktop.png','manual'),fullPage:true});
  const fs=require('node:fs'),path=require('node:path');
  const directory=process.env.RUC_DELIVERY_DIR;
  if(directory){
    const settings=JSON.parse(fs.readFileSync(path.join(directory,'settings.json'),'utf8'));
    assert.deepEqual(Object.keys(settings).sort(),['auto','notice','startup','theme','wake']);
    assert.ok(fs.statSync(path.join(directory,'webview-data')).isDirectory());
    const account=path.join(directory,'account.enc');
    if(fs.existsSync(account))assert.ok(fs.readFileSync(account).subarray(0,10).equals(Buffer.from('RUC-DPAPI'+String.fromCharCode(1))));
  }
  console.log('PASS: clean headings; native HTTPS queries; password-free preferences; local configuration and WebView storage');
  console.log(JSON.stringify({authenticated:result.authenticated,deviceCount:result.devices}));
  await browser.close();
})().catch(async error=>{console.error(error);if(browser)await browser.close();process.exitCode=1;});
