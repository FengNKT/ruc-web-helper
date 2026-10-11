const artifact=require('../../scripts/lib/artifacts.cjs');
// Read-only school queries; local device discovery only. No connection or alias mutations.
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
let browser;
(async()=>{
  for(let i=0;i<40;i++){
    try{browser=await chromium.connectOverCDP('http://127.0.0.1:9227');break;}catch{await new Promise(r=>setTimeout(r,300));}
  }
  assert.ok(browser,'Local WebView test endpoint must be ready');
  try{
    let page;
    for(let i=0;i<40;i++){
      page=browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri\.localhost|tauri:\/\//.test(p.url()));
      if(page)break;await new Promise(r=>setTimeout(r,250));
    }
    assert.ok(page);await page.waitForFunction(()=>!document.querySelector('[data-account]').disabled,{},{timeout:30000});
    const result=await page.evaluate(async()=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const before=await invoke('get_snapshot',{});
      if(!before.username)throw Error('An existing authenticated network session is required');
      const now=new Date(new Date().toLocaleString('en-US',{timeZone:'Asia/Shanghai'}));
      const year=now.getFullYear(),month=now.getMonth()+1;
      const traffic=await invoke('get_traffic_data',{year,month});
      const merged=await invoke('get_snapshot',{});
      const normalize=m=>String(m||'').replace(/[:-]/g,'').toLowerCase();
      const known=new Map(merged.devices.map(d=>[normalize(d.mac),d]));
      const macs=[...new Set(traffic.items.map(r=>normalize(r.user_mac)).filter(m=>/^[0-9a-f]{12}$/.test(m)&&m!=='000000000000'))];
      const historical=merged.devices.find(d=>d.deviceStatus==='historical'&&!d.ignored);
      return {
        before:before.devices.length,merged:merged.devices.length,visible:merged.devices.filter(d=>!d.ignored).length,historyMacs:macs.length,
        covered:macs.every(m=>known.has(m)),
        safe:merged.devices.filter(d=>d.deviceStatus==='historical').every(d=>!d.online&&d.id==null),
        namesMatch:traffic.devices.every(d=>known.get(normalize(d.mac))?.name===d.name),
        target:historical?{mac:normalize(historical.mac),name:historical.name}:null,
        month:`${year}-${String(month).padStart(2,'0')}`,months:merged.deviceMonths
      };
    });
    assert.ok(result.covered);assert.ok(result.safe);assert.ok(result.namesMatch);assert.ok(result.months.includes(result.month));
    await page.reload();await page.waitForFunction(()=>!document.querySelector('[data-account]').disabled,{},{timeout:30000});
    await page.locator('[data-page="devices"]').click();
    assert.ok(await page.locator('[data-all-devices] .ruc-device-preview').count()>=result.visible);
    if(result.target){
      const row=page.locator(`[data-mac="${result.target.mac}"]`);
      assert.ok((await row.textContent()).includes('历史设备'));
      assert.equal(await row.getByRole('button',{name:'下线',exact:true}).count(),0);
      await row.getByRole('button',{name:'查看统计',exact:true}).click();
      await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden,{},{timeout:60000});
      assert.equal(await page.locator('[data-traffic-device]').inputValue(),result.target.mac);
      await page.getByRole('button',{name:'设备详情',exact:true}).click();
      assert.equal(await page.locator('[data-panel="devices"] .ruc-device-focused').getAttribute('data-mac'),result.target.mac);
    }
    await page.screenshot({path:artifact('unified-devices-desktop.png','manual'),fullPage:true});
    console.log('PASS: live registry discovery, history coverage, shared identity, persistence, status and navigation');
    console.log(JSON.stringify({knownDevices:result.merged,recordMacs:result.historyMacs,historicalDeviceAvailable:!!result.target}));
  }finally{if(browser)await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
