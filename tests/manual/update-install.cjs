// Isolated updater integration: fictional user data and an ephemeral signing key.
// No campus requests, real user data or production signing key are used.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync,spawn}=require('node:child_process');
const assert=require('node:assert/strict');
const project=path.resolve(__dirname,'../..');
fs.mkdirSync(path.join(project,'test-results'),{recursive:true});
const root=fs.mkdtempSync(path.join(project,'test-results/manual-update-'));
const harness=path.join(root,'harness');fs.mkdirSync(path.join(harness,'src'),{recursive:true});
const keys=crypto.generateKeyPairSync('ed25519');
const publicKey=keys.publicKey.export({type:'spki',format:'der'}).subarray(-32).toString('base64');
const rustPath=value=>JSON.stringify(value.replaceAll('\\','/'));
fs.writeFileSync(path.join(harness,'Cargo.toml'),`[package]
name="ruc-update-fixture"
version="1.2.0"
edition="2021"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="1"
sha2="0.10"
chrono={version="0.4",default-features=false,features=["clock","std"]}
semver="1"
base64="0.22"
ed25519-dalek="2"
windows-sys={version="0.61",features=["Win32_Foundation","Win32_Storage_FileSystem","Win32_System_Threading"]}
`);
const manifestSource=fs.readFileSync(path.join(project,'src-tauri/src/update_manifest.rs'),'utf8')
 .replace('include_str!("../update-public-key.txt")',JSON.stringify(publicKey));
fs.writeFileSync(path.join(harness,'src/update_manifest.rs'),manifestSource);
fs.writeFileSync(path.join(harness,'src/main.rs'),`#![windows_subsystem="windows"]
mod update_manifest;
#[path=${rustPath(path.join(project,'src-tauri/src/update_install.rs'))}] mod update_install;
mod storage { pub fn directory()->Result<std::path::PathBuf,String>{std::env::var_os("RUC_UPDATE_TEST_DATA").map(std::path::PathBuf::from).ok_or("test directory required".into())} }
fn main(){
 if update_install::helper_mode(){return;}
 let dir=storage::directory().unwrap();
 let args:Vec<_>=std::env::args().collect();
 if args.get(1).map(String::as_str)==Some("--update-complete") {
   std::fs::write(dir.join("new-started.txt"),"new").unwrap();
   if std::fs::read_to_string(dir.join("behavior.txt")).unwrap_or_default()=="ack" {
     update_install::acknowledge(&dir,&std::env::current_exe().unwrap());
     std::thread::sleep(std::time::Duration::from_secs(1));
   }
 } else {std::fs::write(dir.join("old-restored.txt"),"old").unwrap();}
}`);
const targetDir=path.join(project,'src-tauri/target');
const compile=spawnSync('cargo',['build','--offline','--manifest-path',path.join(harness,'Cargo.toml'),'--target-dir',targetDir],{cwd:project,encoding:'utf8',windowsHide:true});
if(compile.status!==0)throw Error(compile.stderr||'Fixture compilation failed');
const fixture=path.join(targetDir,'debug/ruc-update-fixture.exe');
const original=fs.readFileSync(fixture);
const digest=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
async function run(mode){
 const directory=path.join(root,mode),program=path.join(directory,'program'),data=path.join(directory,'user');
 fs.mkdirSync(program,{recursive:true});fs.mkdirSync(data,{recursive:true});
 const area=path.join(data,'.updates');fs.mkdirSync(area);
 const target=path.join(program,'人大校园网助手.exe');fs.writeFileSync(target,original);
 const preserved=['account.enc','settings.json','devices.json','device-names.json'];
 for(const name of preserved)fs.writeFileSync(path.join(data,name),'fictional-local-test-data:'+name);
 fs.writeFileSync(path.join(data,'behavior.txt'),mode==='success'?'ack':'no-ack');
 let bytes=mode==='launch-failure'?Buffer.concat([Buffer.from('MZ'),Buffer.alloc(1022)]):Buffer.concat([original,Buffer.from('\nupdated-fixture')]);
 const manifest={schema:1,project:'ruc-web-helper',version:'9.0.0',tag:'v9.0.0',minUpdaterVersion:'1.1.0',windowsX86_64:{url:'https://github.com/FengNKT/ruc-web-helper/releases/download/v9.0.0/ruc-web-helper_9.0.0_windows-x86_64.exe',size:bytes.length,sha256:digest(bytes)}};
 const raw=Buffer.from(JSON.stringify(manifest)+'\n');
 fs.writeFileSync(path.join(area,'update.json'),raw);
 fs.writeFileSync(path.join(area,'update.json.sig'),crypto.sign(null,raw,keys.privateKey).toString('base64'));
 if(mode==='tampered')bytes[100]^=1;
 fs.writeFileSync(path.join(area,'new.exe'),bytes);
 fs.copyFileSync(fixture,path.join(area,'helper.exe'));
 fs.writeFileSync(path.join(area,'task.json'),JSON.stringify({target,version:'9.0.0',parent:0,nonce:'local-integration-test'}));
 await new Promise((resolve,reject)=>{
   const child=spawn(path.join(area,'helper.exe'),['--apply-update'],{cwd:program,env:{...process.env,RUC_UPDATE_TEST_DATA:data},windowsHide:true,stdio:'ignore'});
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
 for(const name of preserved)assert.equal(fs.readFileSync(path.join(data,name),'utf8'),'fictional-local-test-data:'+name);
 assert.deepEqual(fs.readdirSync(program),['人大校园网助手.exe'],'No configuration or staging files remain beside the EXE');
 for(const name of ['update.json','update.json.sig','new.exe'])if(fs.existsSync(path.join(area,name)))fs.unlinkSync(path.join(area,name));
 console.log('PASS updater integration: '+mode+'; user data preserved; program folder contains only EXE');
}
(async()=>{for(const mode of ['success','launch-failure','no-ack','tampered'])await run(mode);})().catch(error=>{console.error(error);process.exitCode=1;});
