const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'..');
function signUpdate(directory,privateKey){
  const version=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8')).version;
  if(process.env.RELEASE_TAG&&process.env.RELEASE_TAG!=='v'+version)throw Error('Release tag does not match version.');
  const name=`ruc-web-helper_${version}_windows-x86_64.exe`;
  const exe=fs.readFileSync(path.join(directory,name));
  if(exe.length<1024||exe.length>200*1024*1024||exe.subarray(0,2).toString()!=='MZ')throw Error('Invalid Windows executable.');
  const key=crypto.createPrivateKey(privateKey);
  if(key.asymmetricKeyType!=='ed25519')throw Error('An Ed25519 signing key is required.');
  const publicKey=crypto.createPublicKey(key);
  const publicBytes=publicKey.export({type:'spki',format:'der'}).subarray(-32).toString('base64');
  if(publicBytes!==fs.readFileSync(path.join(root,'src-tauri/update-public-key.txt'),'utf8').trim())throw Error('Signing key does not match embedded client key.');
  const manifest={schema:1,project:'ruc-web-helper',version,tag:'v'+version,minUpdaterVersion:'1.1.0',windowsX86_64:{url:`https://github.com/FengNKT/ruc-web-helper/releases/download/v${version}/${name}`,size:exe.length,sha256:crypto.createHash('sha256').update(exe).digest('hex')}};
  const bytes=Buffer.from(JSON.stringify(manifest,null,2)+'\n');
  const signature=crypto.sign(null,bytes,key);
  if(!crypto.verify(null,bytes,publicKey,signature))throw Error('Signature verification failed.');
  fs.writeFileSync(path.join(directory,'update.json'),bytes);
  fs.writeFileSync(path.join(directory,'update.json.sig'),signature.toString('base64')+'\n');
  const assets=[name,`ruc-web-helper_${version}_windows-x86_64_portable.zip`,'update.json','update.json.sig'];
  fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),assets.map(asset=>crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,asset))).digest('hex')+'  '+asset).join('\n')+'\n');
}
if(require.main===module){
  const directory=process.argv[2];
  const key=process.env.UPDATE_SIGNING_KEY||(process.argv[3]&&fs.readFileSync(process.argv[3],'utf8'));
  if(!directory||!key)throw Error('Provide an asset directory and UPDATE_SIGNING_KEY (or a local key file).');
  signUpdate(path.resolve(directory),key);
  console.log('Signed update manifest and refreshed release checksums.');
}
module.exports={signUpdate};
