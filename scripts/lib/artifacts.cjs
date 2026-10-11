const {mkdirSync}=require('node:fs');
const {resolve,basename}=require('node:path');
const root=resolve(__dirname,'../../test-results');
module.exports=function artifact(name,group='e2e'){
  if(!['e2e','manual','icons'].includes(group)||basename(name)!==name)throw Error('Invalid artifact path');
  const directory=resolve(root,group);
  mkdirSync(directory,{recursive:true});
  return resolve(directory,name);
};
