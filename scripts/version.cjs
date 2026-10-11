const fs=require('node:fs');
const path=require('node:path');
const root=path.resolve(__dirname,'..');
const read=file=>fs.readFileSync(path.join(root,file),'utf8');
const json=file=>JSON.parse(read(file));
const stable=/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const packageBlock=/(\[package\][\s\S]*?\nversion = ")([^"]+)(")/;
const lockBlock=/(\[\[package\]\]\r?\nname = "ruc-web-helper"\r?\nversion = ")([^"]+)(")/;
const files=['package.json','package-lock.json','src-tauri/tauri.conf.json'];
const requested=process.argv[2];
if(requested&&requested!=='--check'){
  if(!stable.test(requested))throw Error('Use a stable version such as 1.0.0.');
  // Read and validate all manifests before writing any of them.
  const documents=files.map(file=>[file,json(file)]);
  const toml=read('src-tauri/Cargo.toml'),lock=read('src-tauri/Cargo.lock');
  if(!packageBlock.test(toml)||!lockBlock.test(lock))throw Error('Root Cargo package not found.');
  for(const [file,data] of documents){
    data.version=requested;
    if(file==='package-lock.json')data.packages[''].version=requested;
    fs.writeFileSync(path.join(root,file),JSON.stringify(data,null,2)+'\n');
  }
  fs.writeFileSync(path.join(root,'src-tauri/Cargo.toml'),toml.replace(packageBlock,(_,before,old,after)=>before+requested+after));
  fs.writeFileSync(path.join(root,'src-tauri/Cargo.lock'),lock.replace(lockBlock,(_,before,old,after)=>before+requested+after));
}
const pkg=json('package.json'),lock=json('package-lock.json');
const versions=[pkg.version,lock.version,lock.packages[''].version,json('src-tauri/tauri.conf.json').version,read('src-tauri/Cargo.toml').match(packageBlock)?.[2],read('src-tauri/Cargo.lock').match(lockBlock)?.[2]];
if(!stable.test(pkg.version)||versions.some(version=>version!==pkg.version))throw Error('Project versions do not match: '+versions.join(', '));
const tag=process.env.RELEASE_TAG;
if(tag&&tag!=='v'+pkg.version)throw Error(`Release tag ${tag} does not match v${pkg.version}.`);
if(tag&&!fs.existsSync(path.join(root,'docs/releases',pkg.version+'.md')))throw Error('Missing release notes.');
console.log(`All 6 version fields match ${pkg.version}${tag?' ('+tag+')':''}.`);
