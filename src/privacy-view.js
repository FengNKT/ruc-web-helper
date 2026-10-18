import privacyText from '../docs/privacy.md?raw';

// Render the document's Markdown as text nodes, never executable HTML.
function inline(parent,text){
  const tokens=/(`[^`]+`|\*\*.+?\*\*|\[[^\]]+\]\(https:\/\/[^)]+\))/g;
  let previous=0;
  for(const match of text.matchAll(tokens)){
    parent.append(document.createTextNode(text.slice(previous,match.index)));
    const value=match[0];let element;
    if(value.startsWith('`')){element=document.createElement('code');element.textContent=value.slice(1,-1);}
    else if(value.startsWith('**')){element=document.createElement('strong');element.textContent=value.slice(2,-2);}
    else {
      const link=value.match(/^\[([^\]]+)\]\((.+)\)$/);
      element=document.createElement('a');element.textContent=link[1];element.href=link[2];
    }
    parent.append(element);previous=match.index+value.length;
  }
  parent.append(document.createTextNode(text.slice(previous)));
}

function renderDocument(article){
  const lines=privacyText.replace(/\r/g,'').split('\n');
  const cells=line=>line.trim().replace(/^\||\|$/g,'').split('|').map(value=>value.trim());
  for(let i=0;i<lines.length;){
    const line=lines[i].trim();
    if(!line||line==='# 使用与隐私说明'){i++;continue;}
    const heading=line.match(/^(#{1,6})\s+(.+)/);
    if(heading){const element=document.createElement(`h${Math.min(6,heading[1].length+1)}`);inline(element,heading[2]);article.append(element);i++;continue;}
    if(line.startsWith('|')&&/^\|(?:\s*:?-+:?\s*\|)+\s*$/.test(lines[i+1]||'')){
      const table=document.createElement('table'),head=document.createElement('thead'),body=document.createElement('tbody');
      const row=(values,header)=>{const tr=document.createElement('tr');for(const value of values){const cell=document.createElement(header?'th':'td');if(header)cell.scope='col';inline(cell,value);tr.append(cell);}return tr;};
      head.append(row(cells(line),true));i+=2;
      while(i<lines.length&&lines[i].trim().startsWith('|')){body.append(row(cells(lines[i]),false));i++;}
      table.append(head,body);article.append(table);continue;
    }
    const list=line.match(/^(-|\d+\.)\s+(.+)/);
    if(list){
      const ordered=list[1]!=='-',element=document.createElement(ordered?'ol':'ul');
      while(i<lines.length){const item=lines[i].trim().match(ordered?/^\d+\.\s+(.+)/:/^-\s+(.+)/);if(!item)break;const li=document.createElement('li');inline(li,item[1]);element.append(li);i++;}
      article.append(element);continue;
    }
    const paragraph=document.createElement('p'),text=[line];i++;
    while(i<lines.length&&lines[i].trim()&&!/^(#|\||- |\d+\. )/.test(lines[i].trim()))text.push(lines[i++].trim());
    inline(paragraph,text.join(' '));article.append(paragraph);
  }
}

export function setupPrivacy(root,call,toast){
  const layer=root.querySelector('[data-privacy-dialog]'),article=layer.querySelector('article');
  const opener=root.querySelector('[data-privacy-open]'),closeButton=layer.querySelector('[data-privacy-close]');
  renderDocument(article);
  function close(){layer.hidden=true;opener.focus();}
  opener.addEventListener('click',()=>{layer.hidden=false;article.scrollTop=0;closeButton.focus();});
  closeButton.addEventListener('click',close);
  article.addEventListener('click',async event=>{
    const link=event.target.closest('a');if(!link)return;event.preventDefault();
    if(link.href==='https://github.com/FengNKT/ruc-web-helper/issues'){
      try{await call('open_privacy_feedback');}catch(error){toast(typeof error==='string'?error:error.message||'无法打开反馈页面');}
    }
  });
  return {close};
}
