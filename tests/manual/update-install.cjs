// Local updater integration test. All executables and data stay in a new test directory.
// Requires a local signing key; never publishes signed test programs.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync,spawn}=require('node:child_process');
const assert=require('node:assert/strict');
const project=path.resolve(__dirname,'../..');
const helper=path.join(project,'src-tauri/target/x86_64-pc-windows-msvc/release/ruc-web-helper.exe');
const key=crypto.createPrivateKey(fs.readFileSync(path.join(project,'.local/update-signing.key')));
fs.mkdirSync(path.join(project,'test-results'),{recursive:true});
const root=fs.mkdtempSync(path.join(project,'test-results/manual-update-'));
const source=path.join(root,'fixture.rs'),fixture=path.join(root,'fixture.exe');
fs.writeFileSync(source,`#![windows_subsystem="windows"]
fn main(){
 let dir=std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
 let args:Vec<_>=std::env::args().collect();
 if args.get(1).map(String::as_str)==Some("--update-complete") {
   std::fs::write(dir.join("new-started.txt"),"new").unwrap();
   if std::fs::read_to_string(dir.join("behavior.txt")).unwrap_or_default()=="ack" {
     std::fs::write(dir.join(".updates/ack.txt"),args.get(2).unwrap()).unwrap();
     std::thread::sleep(std::time::Duration::from_secs(1));
   }
 } else {std::fs::write(dir.join("old-restored.txt"),"old").unwrap();}
}`);
const compile=spawnSync('rustc',['--edition','2021','-O',source,'-o',fixture],{cwd:project,encoding:'utf8',windowsHide:true});
if(compile.status!==0)throw Error(compile.stderr||'Fixture compilation failed');
const original=fs.readFileSync(fixture);
const digest=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
async function run(mode){
  const directory=path.join(root,mode);fs.mkdirSync(directory);
  const area=path.join(directory,'.updates');fs.mkdirSync(area);
  const target=path.join(directory,'人大校园网助手.exe');fs.writeFileSync(target,original);
  const preserved=['account.enc','settings.json','devices.json','device-names.json'];
  for(const name of preserved)fs.writeFileSync(path.join(directory,name),'fictional-local-test-data:'+name);
  fs.writeFileSync(path.join(directory,'behavior.txt'),mode==='success'?'ack':'no-ack');
  let bytes=mode==='launch-failure'?Buffer.concat([Buffer.from('MZ'),Buffer.alloc(1022)]):Buffer.concat([original,Buffer.from('\nupdated-fixture')]);
  const manifest={schema:1,project:'ruc-web-helper',version:'9.0.0',tag:'v9.0.0',minUpdaterVersion:'1.1.0',windowsX86_64:{url:'https://github.com/FengNKT/ruc-web-helper/releases/download/v9.0.0/ruc-web-helper_9.0.0_windows-x86_64.exe',size:bytes.length,sha256:digest(bytes)}};
  const raw=Buffer.from(JSON.stringify(manifest)+'\n');
  fs.writeFileSync(path.join(area,'update.json'),raw);
  fs.writeFileSync(path.join(area,'update.json.sig'),crypto.sign(null,raw,key).toString('base64'));
  if(mode==='tampered')bytes[100]^=1;
  fs.writeFileSync(path.join(area,'new.exe'),bytes);
  fs.copyFileSync(helper,path.join(area,'helper.exe'));
  fs.writeFileSync(path.join(area,'task.json'),JSON.stringify({target:path.basename(target),version:'9.0.0',parent:0,nonce:'local-integration-test'}));
  await new Promise((resolve,reject)=>{
    const child=spawn(path.join(area,'helper.exe'),['--apply-update'],{cwd:directory,windowsHide:true,stdio:'ignore'});
    const timer=setTimeout(()=>{child.kill();reject(Error('Updater test timed out'));},90000);
    child.on('error',error=>{clearTimeout(timer);reject(error);});
    child.on('exit',()=>{clearTimeout(timer);resolve();});
  });
  const result=fs.readFileSync(path.join(area,'result.txt'),'utf8');
  if(mode==='success'){
    assert.equal(result,'更新成功');assert.equal(digest(fs.readFileSync(target)),digest(bytes));
    assert.equal(fs.existsSync(path.join(area,'previous.exe')),false);
  }else{
    assert.equal(digest(fs.readFileSync(target)),digest(original));
    assert.match(result,mode==='tampered'?/校验失败/:/已恢复原版本/);
  }
  for(const name of preserved)assert.equal(fs.readFileSync(path.join(directory,name),'utf8'),'fictional-local-test-data:'+name);
  // Remove locally signed test payloads immediately; no private key or test release is distributed.
  for(const name of ['update.json','update.json.sig','new.exe'])if(fs.existsSync(path.join(area,name)))fs.unlinkSync(path.join(area,name));
  console.log('PASS updater integration: '+mode+'; local configuration preserved');
}
(async()=>{for(const mode of ['success','launch-failure','no-ack','tampered'])await run(mode);})().catch(error=>{console.error(error);process.exitCode=1;});
