if(process.env.RUC_ALLOW_LOCAL_TEST_MUTATIONS!=='1')throw Error('This test changes local settings. Set RUC_ALLOW_LOCAL_TEST_MUTATIONS=1 to opt in.');
const artifact=require('../../scripts/lib/artifacts.cjs');
// Only change the appearance preference; restore it and preserve connection settings.
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
(async()=>{
  const directory=process.env.RUC_DATA_DIR;
  assert.ok(directory,'RUC_DATA_DIR is required');
  const settingsFile=path.join(directory,'settings.json');
  const read=()=>JSON.parse(fs.readFileSync(settingsFile,'utf8'));
  const original=read(),savedTheme=original.theme||'system';
  const account=path.join(directory,'account.enc');
  const accountHash=()=>fs.existsSync(account)?crypto.createHash('sha256').update(fs.readFileSync(account)).digest('hex'):null;
  const beforeAccount=accountHash();
  const browser=await chromium.connectOverCDP('http://127.0.0.1:9227');
  let theme;
  try{
    const page=browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri\.localhost|tauri:\/\//.test(p.url()));
    assert.ok(page);await page.locator('[data-page="settings"]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-theme]').disabled);
    theme=page.locator('[data-theme]');
    for(const value of ['light','dark','system']){
      await theme.selectOption(value);
      await page.waitForFunction(()=>!document.querySelector('[data-theme]').disabled);
      assert.equal(await theme.inputValue(),value);
      assert.deepEqual(read(),{...original,theme:value});
      if(value==='dark'){
        assert.equal(await page.locator('#ruc-prototype').evaluate(e=>getComputedStyle(e).backgroundColor),'rgb(24, 27, 32)');
        await page.mouse.move(1030,810);await page.waitForTimeout(200);
        await page.screenshot({path:artifact('dark-mode-native.png','manual')});
      }
    }
  }finally{
    if(theme){await theme.selectOption(savedTheme);await theme.page().waitForFunction(()=>!document.querySelector('[data-theme]').disabled);}
    await browser.close();
  }
  assert.deepEqual(read(),{...original,theme:savedTheme});
  assert.equal(accountHash(),beforeAccount);
  console.log('PASS: three native themes persist; original appearance restored; account and connection settings preserved');
})().catch(error=>{console.error(error);process.exitCode=1;});
