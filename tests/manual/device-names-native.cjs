if(process.env.RUC_ALLOW_LOCAL_TEST_MUTATIONS!=='1')throw Error('This test changes local settings. Set RUC_ALLOW_LOCAL_TEST_MUTATIONS=1 to opt in.');
const artifact=require('../../scripts/lib/artifacts.cjs');
// Temporarily changes only a local alias, then restores its original value.
// Never logs in, logs out, renames school-side devices, or changes startup settings.
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
let browser,page,original,changed=false;
(async()=>{
  for(let i=0;i<30;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9227');break}catch{await new Promise(r=>setTimeout(r,300))}}
  assert.ok(browser);
  for(let i=0;i<30;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri\.localhost|tauri:\/\//.test(p.url()));if(page)break;await new Promise(r=>setTimeout(r,200))}
  assert.ok(page);
  try{
    await page.waitForFunction(()=>!document.querySelector('[data-account]').disabled,{},{timeout:30000});
    const snapshot=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('get_snapshot',{}));
    const device=snapshot.devices.find(d=>/^[0-9a-f:-]{12,17}$/i.test(d.mac||''));assert.ok(device);
    original={mac:device.mac,name:device.customName||'',username:snapshot.username};
    await page.locator('[data-page="devices"]').click();
    const mac=device.mac.replace(/[:-]/g,'').toUpperCase().match(/../g).join(':');
    const row=page.locator('[data-all-devices] .ruc-device-preview').filter({hasText:mac});
    assert.equal(await row.count(),1);
    await row.locator('[data-rename-device]').click();
    await page.locator('input[name="deviceName"]').fill('命名功能验证（临时）');
    changed=true;
    await page.locator('.ruc-device-name-form button[type="submit"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-device-name-dialog]').hidden,{},{timeout:30000});
    assert.ok((await row.textContent()).includes('命名功能验证（临时）'));
    await page.locator('[data-page="records"]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden,{},{timeout:60000});
    assert.ok((await page.locator('[data-traffic-legend]').textContent()).includes('命名功能验证（临时）'));
    assert.ok((await page.locator('[data-traffic-ranking]').textContent()).includes('命名功能验证（临时）'));
    await page.selectOption('[data-traffic-device]',device.mac.replace(/[:-]/g,'').toLowerCase());
    assert.equal(await page.locator('[data-traffic-device] option:checked').textContent(),'命名功能验证（临时）');
    await page.reload();
    await page.waitForFunction(()=>!document.querySelector('[data-account]').disabled,{},{timeout:30000});
    await page.locator('[data-page="devices"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-all-devices]').textContent.includes('命名功能验证（临时）'));
    console.log('PASS: live MAC display, local alias save, statistics labels, device filter and persistence');
  }finally{
    if(changed&&original&&page){
      await page.evaluate(args=>window.__TAURI_INTERNALS__.invoke('set_device_name',args),original);
      const restored=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('get_snapshot',{}));
      const d=restored.devices.find(d=>d.mac===original.mac);assert.equal(d?.customName||'',original.name);
    }
    if(browser)await browser.close();
  }
})().catch(async error=>{console.error(error);if(browser)await browser.close().catch(()=>{});process.exitCode=1});
