const artifact=require('../../scripts/lib/artifacts.cjs');
// Fixture backend is injected by the test runner only, never bundled into the application.
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path');
(async()=>{
  const dist=path.resolve(__dirname,'../../dist');
  const server=http.createServer((req,res)=>{
    const file=path.resolve(dist,'.'+new URL(req.url,'http://localhost').pathname);
    if(file!==dist&&!file.startsWith(dist+path.sep)){res.writeHead(403).end();return;}
    try{const target=file===dist?path.join(dist,'index.html'):file;const bytes=fs.readFileSync(target);res.writeHead(200,{'Content-Type':{'.html':'text/html','.js':'text/javascript','.css':'text/css'}[path.extname(target)]||'application/octet-stream'});res.end(bytes);}catch{res.writeHead(404).end();}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  let browser;
  try{
    browser=await chromium.launch({channel:'msedge',headless:true});
    const page=await browser.newPage({viewport:{width:1040,height:820}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(version=>{
      window.isTauri=true;window.__calls=[];
      const update={preferences:{automatic:true,network:'system'},status:'idle',message:'尚未检查更新',release:null,checked:0,downloaded:0,skipped:false};
      const preferences={settings:{startup:false,auto:false,wake:true,notice:true},username:'',hasCredentials:false,paused:false,automaticError:''};
      const deviceNames={};
      const snapshot={authenticated:true,username:'test-student',ip:'10.0.0.2',status:'E0000',message:'已登录校园网',usedBytes:1000000000,remainBytes:9000000000,packageRemainBytes:9000000000,balance:12.34,checkoutDate:null,warnings:[],devices:[{id:1,name:'Test laptop',os:'Windows',ip:'10.0.0.2',online:1,current:true},{id:2,name:'<img src=x onerror=alert(1)>',os:'iOS',ip:'10.0.0.3',online:'1',current:false}]};
      snapshot.devices[0].mac='aa:bb:cc:dd:ee:ff';snapshot.devices[1].mac='11:22:33:44:55:66';
      window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
        window.__calls.push(cmd);
        if(cmd.startsWith('plugin:event|'))return 1;
        if(cmd==='plugin:app|version')return version;
        if(cmd==='get_update_state')return structuredClone(update);
        if(cmd==='set_update_preferences'){update.preferences=args.preferences;return structuredClone(update);}
        if(cmd==='check_update'){
          if(window.__updateFail)throw '更新网络不可用';
          Object.assign(update,{status:'available',message:'发现新版本 9.0.0',skipped:false,checked:1791690000,release:{version:'9.0.0',notes:'<img src=x onerror=alert(1)>\n更新说明',publishedAt:'2026-10-11T00:00:00Z',size:10000000,signed:true}});
          return structuredClone(update);
        }
        if(cmd==='download_update'){Object.assign(update,{status:'ready',message:'更新已下载，可重启更新',downloaded:10000000});return structuredClone(update);}
        if(cmd==='skip_update'){update.skipped=true;return structuredClone(update);}
        if(cmd==='open_update_release')return null;
        if(cmd==='install_update'){update.status='installing';return structuredClone(update);}
        if(cmd==='get_preferences')return structuredClone(preferences);
        if(cmd==='get_snapshot'){if(window.__fail)throw '网络请求超时';await new Promise(r=>setTimeout(r,500));return structuredClone(snapshot);}
        if(cmd==='set_device_ignored'){
          if(window.__ignoreFail)throw '设备档案保存失败';
          const key=args.mac.replace(/[:-]/g,'').toLowerCase();
          snapshot.devices.find(d=>d.mac.replace(/[:-]/g,'').toLowerCase()===key).ignored=args.ignored;
          return structuredClone(snapshot);
        }
        if(cmd==='disconnect_network'){snapshot.authenticated=false;preferences.paused=true;return structuredClone(snapshot);}
        if(cmd==='connect_network'){snapshot.authenticated=true;preferences.paused=false;return structuredClone(snapshot);}
        if(cmd==='drop_device'){snapshot.devices[1].online=false;return structuredClone(snapshot);}
        if(cmd==='set_preferences'){if(window.__themeFail)throw '无法保存设置';preferences.settings=args.settings;return structuredClone(preferences);}
        if(cmd==='save_account'){if(window.__saveFail)throw '账号保存失败';window.__lastAccountSave={username:args.username,hasNewPassword:args.password.length>0};preferences.username=args.username;preferences.hasCredentials=true;return structuredClone(preferences);}
        if(cmd==='forget_account'){preferences.username='';preferences.hasCredentials=false;preferences.settings.auto=false;return structuredClone(preferences);}
        if(cmd==='get_records')return {items:[{add_time:1791690000,user_ip:'10.0.0.2',time_long:3665,total_bytes:1000000000}],total:1};
        if(cmd==='set_device_name'){
          const key=args.mac.replace(/[:-]/g,'').toLowerCase();if(args.name.trim())deviceNames[key]=args.name.trim();else delete deviceNames[key];
          const d=snapshot.devices.find(d=>d.mac.replace(/:/g,'')===key);
          if(d){d.customName=deviceNames[key]||'';d.name=d.customName||(d.id===1?'Test laptop':'<img src=x onerror=alert(1)>');}
          return {snapshot:structuredClone(snapshot),deviceNames:structuredClone(deviceNames)};
        }
        if(cmd==='get_traffic_data'){
          if(!snapshot.devices.some(d=>d.mac==='abcdef123456'))snapshot.devices.push({mac:'abcdef123456',name:'历史电脑',online:false,deviceStatus:'historical',lastRecord:1791690000});
          if(!snapshot.devices.some(d=>d.mac==='123456abcdef'))snapshot.devices.push({mac:'123456abcdef',name:'备用设备',online:false,deviceStatus:'offline'});
          if(window.__trafficFail)throw '统计接口暂不可用';
          const midnight=Date.UTC(args.year,args.month-1,1)/1000-8*3600;
          return {username:'test-student',deviceNames:structuredClone(deviceNames),items:[
            {detail_id:1,add_time:midnight+23*3600,drop_time:midnight+27*3600,total_bytes:1000000000,user_mac:'aa:bb:cc:dd:ee:ff'},
            {detail_id:2,add_time:midnight+36*3600,drop_time:midnight+37*3600,total_bytes:2000000000,user_mac:'11:22:33:44:55:66'},
            ...(window.__excludeHistory?[]:[{detail_id:3,add_time:midnight+40*3600,drop_time:midnight+41*3600,total_bytes:500000000,user_mac:'abcdef123456'}])],
            devices:structuredClone(snapshot.devices),deviceMonths:['2026-10'],
            usedBytes:4000000000,remainBytes:6000000000,queryFrom:'2026-09-01',queryUntil:'2026-12-01',warnings:[]};
        }
        throw 'Unexpected command '+cmd;
      }};
    },require('../../package.json').version);
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    assert.equal(await page.title(),'人大校园网助手');
    assert.equal(await page.locator('meta[name="description"]').getAttribute('content'),'RUC Web Helper');
    await page.locator('[data-page="settings"]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-theme]').disabled);
    await page.waitForFunction(()=>!document.querySelector('[data-update-network]').disabled);
    assert.equal(await page.locator('[data-update-network]').inputValue(),'system');
    assert.equal(await page.locator('[data-update-automatic]').isChecked(),true);
    await page.locator('[data-update-automatic]').locator('..').locator('strong').click();
    assert.equal(await page.locator('[data-update-automatic]').isChecked(),true,'Update switch text must not toggle');
    await page.locator('[data-update-automatic]').uncheck();
    await page.waitForFunction(()=>!document.querySelector('[data-update-automatic]').disabled);
    assert.equal(await page.locator('[data-update-automatic]').isChecked(),false);
    await page.locator('[data-update-network]').selectOption('direct');
    await page.waitForFunction(()=>!document.querySelector('[data-update-network]').disabled);
    assert.equal(await page.locator('[data-update-network]').inputValue(),'direct');
    await page.evaluate(()=>window.__updateFail=true);
    await page.locator('[data-update-check]').click();
    await page.waitForFunction(()=>document.querySelector('[data-update-status]').textContent==='更新网络不可用');
    await page.evaluate(()=>window.__updateFail=false);
    await page.locator('[data-update-check]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-update-download]').hidden);
    assert.equal(await page.locator('[data-update-notes] img').count(),0,'Release notes must render as text');
    await page.locator('[data-update-skip]').click();
    await page.waitForFunction(()=>document.querySelector('.ruc-update-banner').hidden);
    await page.locator('[data-update-check]').click();
    await page.locator('[data-update-download]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-update-install]').hidden);
    assert.equal(await page.locator('[data-update-check]').isDisabled(),true);
    await page.locator('[data-update-install]').click();
    assert.equal(await page.locator('[data-update-confirm]').isVisible(),true);
    await page.locator('[data-update-later]').click();
    assert.equal(await page.locator('[data-update-confirm]').isVisible(),false);
    assert.equal(await page.evaluate(()=>window.__calls.includes('install_update')),false,'Downloading or deferring must not install automatically');
    await page.locator('[data-update-install]').click();
    await page.locator('[data-update-confirm-install]').click();
    await page.waitForFunction(()=>window.__calls.includes('install_update'));
    await page.locator('[data-panel="settings"]').evaluate(e=>e.closest('.ruc-main').scrollTop=0);
    const automatic=page.locator('[data-setting="auto"]');
    assert.equal(await automatic.isDisabled(),true,'Online account does not count as saved credentials');
    assert.equal(await automatic.isChecked(),false);
    await page.waitForFunction(()=>document.querySelector('[data-profile-account]').textContent==='test-student');
    assert.equal(await page.locator('[data-profile-account]').textContent(),'test-student');
    assert.ok((await page.locator('[data-auto-description]').textContent()).includes('保存账号和密码'));
    const theme=page.locator('[data-theme]');
    const background=()=>page.locator('#ruc-prototype').evaluate(e=>getComputedStyle(e).backgroundColor);
    const choose=async value=>{await theme.selectOption(value);await page.waitForFunction(()=>!document.querySelector('[data-theme]').disabled);};
    assert.equal(await theme.inputValue(),'system','Old settings default to system');
    await page.emulateMedia({colorScheme:'dark'});
    assert.equal(await background(),'rgb(24, 27, 32)');
    await choose('light');assert.equal(await background(),'rgb(255, 255, 255)');
    await page.emulateMedia({colorScheme:'light'});
    const saveFixtureAccount=async()=>{
      await page.locator('[data-account]').click();
      await page.locator('input[name="username"]').fill('test-student');
      await page.locator('input[name="password"]').fill('fictional-password');
      await page.locator('.ruc-account-form button[type="submit"]').click();
    };
    await page.evaluate(()=>window.__saveFail=true);await saveFixtureAccount();
    await page.waitForFunction(()=>document.querySelector('[data-account-error]').textContent==='账号保存失败');
    assert.equal(await automatic.isDisabled(),true,'Failed save does not unlock automatic login');
    await page.locator('[data-account-cancel]').click();
    await page.evaluate(()=>window.__saveFail=false);await saveFixtureAccount();
    await page.waitForFunction(()=>!document.querySelector('[data-setting="auto"]').disabled);
    assert.equal(await automatic.isChecked(),false,'Saving credentials does not enable automatic login');
    assert.equal(await page.locator('[data-profile-account]').textContent(),'test-student');
    assert.equal(await page.locator('[data-account-label]').textContent(),'test-student · 密码已加密保存在本地');
    for(const setting of ['startup','auto','wake','notice']){
      const control=page.locator(`[data-setting="${setting}"]`),row=control.locator('..');
      const before=await control.isChecked();
      await row.locator('strong').click();await row.locator('small').click();
      assert.equal(await control.isChecked(),before,'Setting text must not toggle '+setting);
    }
    await automatic.check();await page.waitForFunction(()=>!document.querySelector('[data-theme]').disabled);
    assert.equal(await automatic.isChecked(),true);
    await page.locator('[data-account]').click();await page.locator('[data-forget]').click();
    await page.waitForFunction(()=>document.querySelector('[data-account-dialog]').hidden);
    assert.equal(await automatic.isDisabled(),true);assert.equal(await automatic.isChecked(),false);
    await saveFixtureAccount();await page.waitForFunction(()=>!document.querySelector('[data-setting="auto"]').disabled);
    await choose('dark');assert.equal(await background(),'rgb(24, 27, 32)');
    assert.equal(await page.evaluate(()=>localStorage.getItem('ruc-theme')),'dark');
    await page.locator('[data-account]').click();
    assert.equal(await page.locator('.ruc-account-form').evaluate(e=>getComputedStyle(e).backgroundColor),'rgb(24, 27, 32)');
    const password=page.locator('input[name="password"]');
    assert.equal(await password.inputValue(),'');
    assert.equal(await password.getAttribute('placeholder'),'已配置密码（留空保留）');
    assert.ok((await page.locator('[data-password-hint]').textContent()).includes('覆盖原密码'));
    assert.equal(await password.evaluate(e=>e.required),false);
    await page.locator('input[name="username"]').fill('other-student');
    assert.equal(await password.evaluate(e=>e.required),true);
    await page.locator('input[name="username"]').fill('test-student');
    assert.equal(await password.evaluate(e=>e.required),false);
    await page.locator('.ruc-account-form button[type="submit"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-account-dialog]').hidden);
    assert.deepEqual(await page.evaluate(()=>window.__lastAccountSave),{username:'test-student',hasNewPassword:false});
    await page.locator('[data-account]').click();await password.fill('fictional-replacement');
    await page.locator('.ruc-account-form button[type="submit"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-account-dialog]').hidden);
    assert.deepEqual(await page.evaluate(()=>window.__lastAccountSave),{username:'test-student',hasNewPassword:true});
    await page.mouse.move(1030,810);await page.waitForTimeout(200);
    await page.screenshot({path:artifact('dark-mode-settings.png','e2e')});
    await page.evaluate(()=>window.__themeFail=true);await choose('light');
    assert.equal(await theme.inputValue(),'dark','Save failure rolls back');
    assert.equal(await background(),'rgb(24, 27, 32)');
    await page.evaluate(()=>window.__themeFail=false);await choose('system');
    assert.equal(await background(),'rgb(255, 255, 255)');
    await page.emulateMedia({colorScheme:'dark'});assert.equal(await background(),'rgb(24, 27, 32)');
    await page.emulateMedia({colorScheme:'light'});
    await page.locator('[data-page="devices"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-all-devices]').textContent.includes('Test laptop'));
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),2);
    assert.equal(await page.locator('[data-device-month],[data-device-sync],[data-device-coverage]').count(),0);
    assert.equal(await page.evaluate(()=>window.__calls.includes('get_traffic_data')),false);
    await page.locator('[data-page="records"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-traffic-total]').textContent==='3.50 GB');
    await page.locator('[data-page="overview"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-status-title]').textContent==='已连接校园网');
    assert.equal(await page.locator('.ruc-titlebar,.ruc-brand').count(),0);
    assert.equal(await page.locator('[data-page="records"]').textContent(),'统计');
    assert.equal(await page.locator('[data-panel="records"] h1').textContent(),'数据统计');
    assert.equal(await page.locator('[data-panel="records"] .ruc-heading [data-traffic-refresh]').count(),1);
    assert.equal(await page.locator('.ruc-statistics-filters [data-month],.ruc-statistics-filters [data-traffic-device]').count(),2);
    assert.equal(await page.locator('[data-panel="devices"] .ruc-heading [data-view-ignored]').count(),1);
    assert.equal(await page.locator('[data-remain]').textContent(),'123.00 GB');
    assert.equal(await page.locator('[data-used]').textContent(),'1.00 GB');
    assert.equal(await page.locator('[data-free]').textContent(),'99.00 GB');
    assert.equal(await page.locator('[data-paid]').textContent(),'24.00 GB');
    assert.equal(await page.locator('[data-quota-segment]').count(),3);
    await page.screenshot({path:artifact('overview-allowance-light.png','e2e')});
    await page.emulateMedia({colorScheme:'dark'});await page.waitForTimeout(200);
    await page.screenshot({path:artifact('overview-allowance-dark.png','e2e')});
    await page.emulateMedia({colorScheme:'light'});
    for(const width of [736,480,320]){
      await page.setViewportSize({width,height:900});
      assert.ok(await page.evaluate(()=>{const main=document.querySelector('.ruc-main');return main.scrollWidth<=main.clientWidth&&document.documentElement.scrollWidth<=innerWidth;}),'Overview quota layout at '+width);
    }
    await page.setViewportSize({width:1040,height:820});
    await page.locator('[data-connect]').first().click();await page.locator('[data-cancel]').click();
    assert.equal(await page.evaluate(()=>window.__calls.includes('disconnect_network')),false);
    await page.locator('[data-connect]').first().click();await page.locator('[data-confirm]').click();
    await page.waitForFunction(()=>document.querySelector('[data-status-title]').textContent==='尚未登录校园网');
    await page.locator('[data-connect]').first().click();await page.waitForFunction(()=>document.querySelector('[data-status-title]').textContent==='已连接校园网');
    await page.locator('[data-page="devices"]').click();assert.equal(await page.locator('[data-all-devices] img').count(),0);
    await page.waitForFunction(()=>document.querySelector('[data-all-devices]').textContent.includes('历史电脑'));
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),4);
    const historical=page.locator('[data-mac="abcdef123456"]');
    assert.ok((await historical.textContent()).includes('历史设备'));
    assert.equal(await historical.getByRole('button',{name:'下线',exact:true}).count(),0);
    await page.selectOption('[data-device-filter]','historical');
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),1);
    await historical.getByRole('button',{name:'查看统计',exact:true}).click();
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden);
    assert.equal(await page.locator('[data-traffic-device] option[value="123456abcdef"]').count(),0);
    assert.equal(await page.locator('[data-traffic-device] option').count(),4);
    const filters=await page.locator('.ruc-statistics-filters').boundingBox();
    const tabs=await page.locator('.ruc-record-tabs').boundingBox();
    assert.ok(filters.y+filters.height<=tabs.y);
    await page.selectOption('[data-traffic-device]','abcdef123456');
    assert.equal(await page.locator('[data-traffic-device]').inputValue(),'abcdef123456');
    assert.equal(await page.locator('[data-traffic-total]').textContent(),'500.00 MB');
    await page.getByRole('button',{name:'设备详情',exact:true}).click();
    assert.equal(await page.locator('[data-panel="devices"] .ruc-device-focused').getAttribute('data-mac'),'abcdef123456');
    await page.locator('[data-panel="devices"] h1').click();
    assert.equal(await page.locator('.ruc-device-focused').count(),0);
    await page.selectOption('[data-device-filter]','all');
    assert.equal(await page.locator('.ruc-device-focused').count(),0);
    await historical.getByRole('button',{name:'查看统计',exact:true}).click();
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden);
    await page.getByRole('button',{name:'设备详情',exact:true}).click();
    assert.equal(await page.locator('.ruc-device-focused').count(),1);
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.ruc-device-focused').count(),0);
    await page.evaluate(()=>window.__ignoreFail=true);
    await historical.getByRole('button',{name:'忽略',exact:true}).click();
    await page.waitForFunction(()=>document.querySelector('.ruc-toast').textContent==='设备档案保存失败');
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),4);
    await page.evaluate(()=>window.__ignoreFail=false);
    await historical.getByRole('button',{name:'忽略',exact:true}).click();
    await page.waitForFunction(()=>document.querySelector('[data-all-devices]').children.length===3);
    await page.locator('[data-panel="devices"] [data-refresh]').click();
    await page.waitForFunction(()=>document.querySelector('.ruc-toast').textContent==='数据已更新');
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),3);
    await page.locator('[data-view-ignored]').click();
    assert.equal(await page.locator('[data-panel="ignored-devices"]').isVisible(),true);
    assert.equal(await page.locator('[data-page="devices"]').getAttribute('aria-current'),'page');
    assert.equal(await page.locator('[data-ignored-devices] .ruc-device-preview').count(),1);
    const ignored=page.locator('[data-ignored-devices] [data-mac="abcdef123456"]');
    assert.ok((await ignored.textContent()).includes('AB:CD:EF:12:34:56'));
    await ignored.getByRole('button',{name:'查看统计',exact:true}).click();
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden);
    assert.equal(await page.locator('[data-traffic-total]').textContent(),'500.00 MB');
    await page.getByRole('button',{name:'设备详情',exact:true}).click();
    assert.equal(await page.locator('[data-panel="ignored-devices"]').isVisible(),true);
    await ignored.getByRole('button',{name:'恢复',exact:true}).click();
    await page.waitForFunction(()=>document.querySelector('[data-ignored-devices]').textContent==='暂无已忽略设备');
    await page.locator('[data-return-devices]').click();
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),4);
    assert.equal(await page.evaluate(()=>window.__calls.includes('sync_devices')),false);
    assert.ok((await page.locator('[data-all-devices]').textContent()).includes('AA:BB:CC:DD:EE:FF'));
    await page.locator('[data-rename-device]').first().click();
    await page.locator('input[name="deviceName"]').fill('我的电脑');
    await page.locator('.ruc-device-name-form button[type="submit"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-device-name-dialog]').hidden);
    assert.ok((await page.locator('[data-all-devices]').textContent()).includes('我的电脑'));
    await page.screenshot({path:artifact('device-actions-fixture.png','e2e'),fullPage:true});
    await page.getByRole('button',{name:'下线',exact:true}).click();await page.locator('[data-confirm]').click();
    await page.waitForFunction(()=>document.querySelector('[data-all-devices]').textContent.includes('离线'));
    await page.locator('[data-page="records"]').click();await page.waitForFunction(()=>document.querySelector('[data-record-info]').textContent.includes('共 1 条'));
    await page.selectOption('[data-traffic-device]','all');
    await page.waitForFunction(()=>document.querySelector('[data-traffic-total]').textContent==='3.50 GB');
    assert.ok((await page.locator('[data-traffic-legend]').textContent()).includes('我的电脑'));
    assert.ok((await page.locator('[data-traffic-ranking]').textContent()).includes('我的电脑'));
    assert.equal(await page.locator('[data-traffic-content] img').count(),0);
    assert.equal(await page.locator('.ruc-traffic-svg').count(),1);
    assert.equal(await page.locator('[data-month-pie] path').count(),3);
    await page.emulateMedia({colorScheme:'dark'});
    const darkSlice=await page.locator('[data-month-pie] path').first().evaluate(e=>getComputedStyle(e).fill);
    assert.notEqual(darkSlice,'rgb(0, 0, 0)','Dark chart uses resolved palette');
    await page.mouse.move(1030,810);await page.waitForTimeout(200);
    assert.equal(await page.locator('.ruc-ranking-row').first().evaluate(e=>getComputedStyle(e).color),'rgb(230, 234, 240)');
    await page.screenshot({path:artifact('dark-mode-statistics.png','e2e')});
    await page.emulateMedia({colorScheme:'light'});
    assert.notEqual(await page.locator('[data-month-pie] path').first().evaluate(e=>getComputedStyle(e).fill),darkSlice);
    assert.ok((await page.locator('[data-traffic-ranking]').textContent()).includes('57.1%'));
    assert.equal(await page.locator('#day-panel').isVisible(),false);
    assert.equal(await page.locator('[data-statistics-day-filter]').isVisible(),false);
    await page.locator('[data-record-tab="day"]').click();
    assert.equal(await page.locator('#month-panel').isVisible(),false);
    assert.equal(await page.locator('#day-panel').isVisible(),true);
    assert.equal(await page.locator('[data-statistics-day-filter]').isVisible(),true);
    await page.selectOption('[data-traffic-day]','1');
    assert.ok((await page.locator('[data-traffic-day-detail]').textContent()).includes('250.00 MB'));
    assert.equal(await page.locator('[data-day-pie] circle').count(),1);
    assert.ok((await page.locator('.ruc-day-breakdown').textContent()).includes('100.0%'));
    await page.selectOption('[data-traffic-day]','2');
    assert.equal(await page.locator('[data-day-pie] path').count(),3);
    assert.ok((await page.locator('.ruc-day-breakdown').textContent()).includes('61.5%'));
    assert.ok((await page.locator('.ruc-day-breakdown').textContent()).includes('23.1%'));
    assert.ok((await page.locator('.ruc-day-breakdown').textContent()).includes('15.4%'));
    assert.equal(await page.locator('.ruc-day-heading [data-traffic-day]').count(),1);
    await page.locator('[data-traffic-day-detail]').screenshot({path:artifact('daily-traffic-pie-fixture.png','e2e')});
    await page.selectOption('[data-traffic-day]','3');
    assert.equal(await page.locator('[data-day-pie]').count(),0);
    await page.selectOption('[data-traffic-day]','2');
    await page.selectOption('[data-traffic-device]','aabbccddeeff');
    assert.equal(await page.locator('[data-traffic-total]').textContent(),'1.00 GB');
    assert.equal(await page.locator('[data-day-pie] circle').count(),1);
    assert.ok((await page.locator('.ruc-day-breakdown').textContent()).includes('100.0%'));
    await page.selectOption('[data-traffic-device]','all');
    await page.locator('[data-traffic-refresh]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-refresh]').disabled);
    assert.equal(await page.locator('[data-record-tab="day"]').getAttribute('aria-selected'),'true');
    assert.equal(await page.locator('[data-traffic-day]').inputValue(),'2');
    await page.locator('[data-record-tab="traffic"]').click();
    await page.locator('[data-traffic-ranking]').getByRole('button',{name:'命名我的电脑',exact:true}).click();
    await page.locator('[data-device-name-reset]').click();
    await page.waitForFunction(()=>document.querySelector('[data-device-name-dialog]').hidden&&!document.querySelector('[data-traffic-content]').hidden);
    assert.equal((await page.locator('[data-traffic-legend]').textContent()).includes('我的电脑'),false);
    await page.locator('[data-record-tab="history"]').click();
    assert.equal(await page.locator('#history-panel').isVisible(),true);
    assert.equal(await page.locator('[data-statistics-device-filter]').isVisible(),true);
    assert.equal(await page.locator('[data-month]').isVisible(),true);
    assert.equal(await page.locator('[data-traffic-refresh]').isVisible(),true);
    assert.ok((await page.locator('[data-record-info]').textContent()).includes('共 3 条'));
    await page.selectOption('[data-traffic-device]','aabbccddeeff');
    assert.ok((await page.locator('[data-record-info]').textContent()).includes('共 1 条'));
    assert.equal(await page.locator('[data-record-body] tr').count(),1);
    assert.ok((await page.locator('[data-record-body]').textContent()).includes('1.00 GB'));
    await page.selectOption('[data-traffic-device]','all');
    const recordCalls=await page.evaluate(()=>window.__calls.filter(c=>c==='get_traffic_data').length);
    await page.locator('[data-traffic-refresh]').click();
    await page.waitForFunction(count=>window.__calls.filter(c=>c==='get_traffic_data').length>count,recordCalls);
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-refresh]').disabled);
    assert.equal(await page.locator('#history-panel').isVisible(),true);
    await page.locator('[data-record-tab="traffic"]').click();
    assert.equal(await page.locator('[data-statistics-device-filter]').isVisible(),true);
    await page.evaluate(()=>window.__trafficFail=true);await page.locator('[data-traffic-refresh]').click();
    await page.waitForFunction(()=>document.querySelector('[data-traffic-status]').textContent==='统计接口暂不可用');
    assert.equal(await page.locator('[data-traffic-content]').isVisible(),false);
    await page.evaluate(()=>window.__trafficFail=false);await page.locator('[data-traffic-refresh]').click();
    await page.waitForFunction(()=>document.querySelector('[data-traffic-total]').textContent==='3.50 GB');
    await page.locator('#month-panel').screenshot({path:artifact('monthly-statistics-fixture.png','e2e')});
    const chart=await page.locator('.ruc-traffic-svg').boundingBox();
    await page.locator('.ruc-traffic-svg').click({position:{x:48+(chart.width-60)*1.5/31,y:70}});
    assert.equal(await page.locator('[data-record-tab="day"]').getAttribute('aria-selected'),'true');
    assert.equal(await page.locator('[data-traffic-day]').inputValue(),'2');
    assert.ok((await page.locator('[data-traffic-day-detail]').textContent()).includes('3.25 GB'));
    await page.waitForTimeout(200);
    await page.screenshot({path:artifact('daily-statistics-fixture.png','e2e'),fullPage:true});
    await page.locator('[data-record-tab="traffic"]').click();
    await page.screenshot({path:artifact('traffic-statistics-fixture.png','e2e'),fullPage:true});
    const initialMonth=await page.locator('[data-month]').inputValue();
    await page.selectOption('[data-traffic-device]','abcdef123456');
    await page.evaluate(()=>window.__excludeHistory=true);
    const previousMonth=await page.locator('[data-month] option').nth(1).getAttribute('value');
    await page.selectOption('[data-month]',previousMonth);
    await page.waitForFunction(()=>document.querySelector('[data-traffic-total]').textContent==='3.00 GB');
    assert.equal(await page.locator('[data-traffic-device]').inputValue(),'all');
    assert.equal(await page.locator('[data-traffic-device] option').count(),3);
    assert.equal(await page.locator('[data-traffic-device] option[value="abcdef123456"]').count(),0);
    assert.ok((await page.locator('[data-record-info]').textContent()).includes('共 2 条'));
    await page.evaluate(()=>window.__excludeHistory=false);
    await page.selectOption('[data-month]',initialMonth);
    await page.waitForFunction(()=>document.querySelector('[data-traffic-total]').textContent==='3.50 GB');
    await page.locator('[data-page="settings"]').click();await page.locator('[data-account]').click();
    assert.equal(await page.locator('input[name="password"]').inputValue(),'');await page.locator('[data-account-cancel]').click();
    for(const tab of ['overview','devices','records','settings']){
      await page.locator(`[data-page="${tab}"]`).click();
      if(tab==='records'){await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden);}
      for(const width of [1040,736,480,360,320]){await page.setViewportSize({width,height:900});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,`${tab} at ${width}`);assert.ok(await page.evaluate(()=>{const main=document.querySelector('.ruc-main');return main.scrollWidth<=main.clientWidth&&document.documentElement.scrollHeight<=innerHeight}),`Content stays within its scroll pane: ${tab} at ${width}`);}
    }
    await page.locator('[data-page="records"]').click();
    await page.locator('[data-record-tab="day"]').click();
    await page.selectOption('[data-traffic-day]','2');
    for(const width of [1040,736,480,360,320]){
      await page.setViewportSize({width,height:900});
      assert.ok(await page.evaluate(()=>{const main=document.querySelector('.ruc-main');return main.scrollWidth<=main.clientWidth&&document.documentElement.scrollWidth<=innerWidth}),`Daily layout at ${width}`);
    }
    await page.locator('[data-record-tab="traffic"]').click();
    assert.equal(await page.locator('.ruc-footer,[data-updated],[data-footer-state]').count(),0);
    await page.setViewportSize({width:1040,height:580});
    await page.locator('[data-page="records"]').click();
    await page.waitForFunction(()=>!document.querySelector('[data-traffic-content]').hidden);
    const sidebar=await page.locator('.ruc-sidebar').boundingBox();
    await page.locator('[data-traffic-chart]').hover();await page.mouse.wheel(0,600);
    await page.waitForFunction(()=>document.querySelector('.ruc-main').scrollTop>0);
    assert.equal(await page.evaluate(()=>window.scrollY),0);
    assert.deepEqual(await page.locator('.ruc-sidebar').boundingBox(),sidebar);
    await page.locator('[data-page="settings"]').click();
    assert.deepEqual(await page.locator('.ruc-sidebar').boundingBox(),sidebar);
    await page.locator('[data-page="overview"]').click();await page.evaluate(()=>window.__fail=true);await page.locator('[data-refresh]').first().click();
    await page.waitForFunction(()=>document.querySelector('[data-status-title]').textContent==='无法读取校园网状态');
    assert.equal(await page.locator('[data-remain]').textContent(),'—');
    await page.locator('[data-page="devices"]').click();
    assert.equal(await page.locator('[data-all-devices] .ruc-device-preview').count(),4);
    assert.ok((await page.locator('[data-all-devices]').textContent()).includes('状态未更新'));
    assert.deepEqual(errors,[]);console.log('PASS: real command wiring, safe text rendering, destructive-action confirmation, offline clearing and responsive layouts');
  }finally{if(browser)await browser.close();server.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
