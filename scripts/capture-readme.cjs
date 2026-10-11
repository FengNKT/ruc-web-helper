// Capture the real interface with entirely fictional data; never contact the campus network.
const {chromium}=require('playwright');
const http=require('node:http');
const fs=require('node:fs');
const path=require('node:path');

(async()=>{
  const root=path.resolve(__dirname,'../dist');
  const output=path.resolve(__dirname,'../docs/images');
  fs.mkdirSync(output,{recursive:true});
  const server=http.createServer((req,res)=>{
    const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname);
    if(file!==root&&!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}
    try{
      const target=file===root?path.join(root,'index.html'):file;
      res.writeHead(200,{'Content-Type':{'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png'}[path.extname(target)]||'application/octet-stream'});
      res.end(fs.readFileSync(target));
    }catch{res.writeHead(404).end();}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  let browser;
  try{
    browser=await chromium.launch({channel:'msedge',headless:true});
    const page=await browser.newPage({viewport:{width:1040,height:820},deviceScaleFactor:1});
    const errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    // No remote requests, including portal requests, are permitted during capture.
    await page.route('**/*',route=>new URL(route.request().url()).hostname==='127.0.0.1'?route.continue():route.abort());
    await page.addInitScript(({version})=>{
      window.isTauri=true;
      const preferences={settings:{startup:true,auto:true,wake:true,notice:true,theme:'light'},username:'demo2026',hasCredentials:true,paused:false,automaticError:''};
      const devices=[
        {id:1,name:'我的电脑',customName:'我的电脑',os:'Windows',mac:'02:00:00:00:00:01',ip:'192.0.2.10',online:true,current:true},
        {id:2,name:'手机',customName:'手机',os:'Android',mac:'02:00:00:00:00:02',ip:'192.0.2.11',online:true,current:false},
        {id:3,name:'平板',customName:'平板',os:'iOS',mac:'02:00:00:00:00:03',online:false,deviceStatus:'historical'}
      ];
      const deviceNames=Object.fromEntries(devices.map(d=>[d.mac.replaceAll(':',''),d.name]));
      const snapshot={authenticated:true,username:'demo2026',ip:'192.0.2.10',status:'E0000',message:'已登录校园网',usedBytes:36.8e9,remainBytes:87.2e9,balance:12.34,checkoutDate:Date.UTC(2026,10,1)/1000,warnings:[],devices};
      window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
        if(cmd.startsWith('plugin:event|'))return 1;
        if(cmd==='plugin:app|version')return version;
        if(cmd==='get_update_state')return {preferences:{automatic:true,network:'system'},status:'current',message:'当前已是最新版本',release:null,checked:1791690000,downloaded:0,skipped:false};
        if(cmd==='get_preferences')return structuredClone(preferences);
        if(cmd==='get_snapshot')return structuredClone(snapshot);
        if(cmd==='get_traffic_data'){
          const start=Date.UTC(args.year,args.month-1,1)/1000-8*3600;
          const usage=[[1.2,2.8,.8,2.5,1.6,2.2,3.5,1.9,3.1,2.4],[.4,.6,.3,.8,.4,.5,.6,.7,.5,.6],[.5,.7,.6,1.2,.8,.9,1.4,.6,1.1,1.6]];
          const items=usage.flatMap((values,device)=>values.map((gb,day)=>{
            const add=start+day*86400+[8,12,20][device]*3600;
            return {detail_id:device*10+day+1,add_time:device===0&&day===3?start+day*86400-3600:add,drop_time:device===0&&day===3?start+day*86400+7200:add+[6,2,3][device]*3600,total_bytes:Math.round(gb*1e9),user_mac:devices[device].mac,user_ip:devices[device].ip||'192.0.2.12'};
          }));
          const monthLabel=`${args.year}-${String(args.month).padStart(2,'0')}`;
          return {...structuredClone(snapshot),items,deviceNames,queryFrom:`${monthLabel}-01`,queryUntil:`${monthLabel}-${new Date(Date.UTC(args.year,args.month,0)).getUTCDate()}`};
        }
        throw Error('Unexpected documentation capture command: '+cmd);
      }};
    },{version:require('../package.json').version});
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.locator('[data-status-title]').filter({hasText:'已连接校园网'}).waitFor();
    const capture=async name=>{
      await page.mouse.move(5,5);
      await page.evaluate(()=>document.fonts.ready);
      await page.waitForTimeout(250);
      await page.screenshot({path:path.join(output,name+'.png')});
    };
    await capture('overview');
    await page.locator('[data-page="settings"]').click();
    await capture('settings');
    await page.locator('[data-update-card]').scrollIntoViewIfNeeded();
    await capture('updates');
    await page.setViewportSize({width:1040,height:1450});
    await page.locator('[data-page="records"]').click();
    await page.locator('[data-traffic-total]').filter({hasText:'36.80 GB'}).waitFor();
    await capture('monthly-statistics');
    await page.setViewportSize({width:1040,height:900});
    await page.locator('[data-record-tab="day"]').click();
    await page.selectOption('[data-traffic-day]','7');
    await capture('daily-statistics');
    if(errors.length)throw Error(errors.join('\n'));
    process.stdout.write('Saved 5 documentation screenshots with fictional data.\n');
  }finally{
    if(browser)await browser.close();
    await new Promise(resolve=>server.close(resolve));
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
