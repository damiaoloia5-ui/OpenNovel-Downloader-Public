// Vue component event tests on an in-memory host, not a browser or native UI.
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile,writeFile,mkdir,mkdtemp} from 'node:fs/promises';
import {resolve,join,basename} from 'node:path';
import {pathToFileURL} from 'node:url';
import {build,transformWithOxc} from 'vite';
import {parse,compileScript} from '@vue/compiler-sfc';
import {createRenderer,h,reactive,nextTick} from 'vue';
import {readGuidedCatalog} from '../apps/desktop/src/guidedCatalog.ts';
import {saveRecovery} from '../apps/desktop/src/workflow.ts';

const preview={token:'original-preview',kind:'catalog',chapter:null,catalog:{source:'qidian-session',remote_id:'123',title:'原创向导样书',author:'原创作者',description:'',chapters:[{id:'1',title:'第一章 原创',restricted:false},{id:'2',title:'第二章 原创',restricted:false}]}};
const stored={id:'original-book',source:'qidian-session',remote_id:'123',title:'原创向导样书',author:'原创作者',saved_chapters:0,total_chapters:2};
const link='https://www.qidian.com/book/123/';
const transient='完整目录或官网总章数未识别；请展开目录并等待全部加载后重试';
test('catalog polling is bounded and only retries identified transient failures',async()=>{
  let probes=0,waits=0;
  assert.deepEqual(await readGuidedCatalog(link,async()=>{if(++probes<3)throw transient;return preview;},()=>true,async()=>{waits++;}),preview);
  assert.equal(probes,3);assert.equal(waits,3);probes=0;
  await assert.rejects(readGuidedCatalog(link,async()=>{probes++;throw new Error(transient);},()=>true,async()=>{}));assert.equal(probes,6);
  for(const message of ['官网要求验证码，请本人处理后重新读取','页面包含订阅或试读提示，本次未保存','官网会话已变化，请重新读取','页面读取结果超过 2 MiB，未保存']){
    probes=0;await assert.rejects(readGuidedCatalog(link,async()=>{probes++;throw new Error(message);},()=>true,async()=>{}),{message});assert.equal(probes,1);
  }
});
test('late catalog responses cannot cross books or abandoned views',async()=>{
  let current=true,probes=0;
  const result=await readGuidedCatalog(link,async()=>{probes++;current=false;return preview;},()=>current,async()=>{});
  assert.equal(result,null);assert.equal(probes,1);
  await assert.rejects(readGuidedCatalog(link,async()=>({...preview,catalog:{...preview.catalog,remote_id:'999'}}),()=>true,async()=>{}),/不是这本书/);
  probes=0;assert.equal(await readGuidedCatalog(link,async()=>{probes++;return preview;},()=>false,async()=>{}),null);assert.equal(probes,0);
});
test('stopped work gives a concrete next action without granting access',()=>{
  assert.match(saveRecovery('官网要求重新登录'),/重新登录/);assert.match(saveRecovery('官网要求验证码'),/完成验证/);
  assert.match(saveRecovery('订阅或保护内容'),/核对这一章/);assert.match(saveRecovery('页面读取超时'),/检查网络/);
});

// Compile actual SFC scripts + client templates, replacing only IPC and peripheral
// panels. Native navigation, layout and account entitlement are outside this test.
await mkdir('.artifacts',{recursive:true});
const output=await mkdtemp(resolve('.artifacts/simple-flow-tests-'));
const apiSource=await readFile('apps/desktop/src/api.ts','utf8');
const names=[...apiSource.matchAll(/export function (\w+)/g)].map(m=>m[1]);
const mockFile=join(output,'api.mjs');
await writeFile(mockFile,`export const desktopAvailable=true;\n${names.map(name=>`export const ${name}=(...args)=>globalThis.__simpleApi.${name}(...args);`).join('\n')}`);
const windowFile=join(output,'window.mjs');
await writeFile(windowFile,'export const getCurrentWindow=()=>({isMinimized:async()=>false,onResized:async()=>()=>{}});');
const entry=join(output,'entry.mjs');
await writeFile(entry,`export {default as Account} from ${JSON.stringify(resolve('apps/desktop/src/AccountPanel.vue'))};\nexport {default as AutoSave} from ${JSON.stringify(resolve('apps/desktop/src/AutoSavePanel.vue'))};\nexport {default as App} from ${JSON.stringify(resolve('apps/desktop/src/App.vue'))};`);
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as Official} from '+JSON.stringify(resolve('apps/desktop/src/OfficialReaderPanel.vue'))+';\n');
const real=new Set(['AccountPanel.vue','AutoSavePanel.vue','App.vue','OfficialReaderPanel.vue']);
await build({configFile:false,logLevel:'error',plugins:[{
  name:'controlled-vue-host',enforce:'pre',
  resolveId(id){if(id==='./api')return mockFile;if(id==='@tauri-apps/api/window')return windowFile;},
  async transform(source,id){if(!id.endsWith('.vue'))return;
    if(!real.has(basename(id)))return `import {h} from 'vue';export default {props:['bookId'],render(){return h('section',{'data-stub':${JSON.stringify(basename(id))}},this.bookId||'');}};`;
    const {descriptor}=parse(source,{filename:id});const compiled=compileScript(descriptor,{id:basename(id),inlineTemplate:true,templateOptions:{compilerOptions:{hoistStatic:false}}});
    return transformWithOxc(compiled.content,`${id}.ts`,{lang:'ts'});
  },
}],build:{ssr:entry,outDir:output,emptyOutDir:false,rollupOptions:{output:{entryFileNames:'components.mjs'}}}});
const {Account,AutoSave,App,Official}=await import(pathToFileURL(join(output,'components.mjs')).href);
function element(tag,text=''){
  return {tag,tagName:tag.toUpperCase(),children:[],parent:null,props:{},text,listeners:{},value:'',checked:false,selected:false,
    get textContent(){return this.text+this.children.map(n=>n.textContent).join('');},
    get options(){return this.children.filter(c=>c.tag==='option');},
    getRootNode(){return globalThis.document;},
    addEventListener(type,fn){(this.listeners[type]??=[]).push(fn);},removeEventListener(){},
    focus(){globalThis.document.activeElement=this;},
    dispatch(type){for(const fn of this.listeners[type]||[])fn({target:this});},
  };
}
const renderer=createRenderer({
  createElement:element,createText:text=>element('#text',text),createComment:()=>element('#comment'),
  insert(child,parent,anchor=null){if(child.parent){const index=child.parent.children.indexOf(child);if(index>=0)child.parent.children.splice(index,1);}child.parent=parent;const at=anchor?parent.children.indexOf(anchor):-1;parent.children.splice(at<0?parent.children.length:at,0,child);},
  remove(child){if(child.parent){child.parent.children.splice(child.parent.children.indexOf(child),1);child.parent=null;}},
  setText(node,text){node.text=text;},setElementText(node,text){node.text=text;node.children=[];},
  parentNode:node=>node.parent,nextSibling:node=>node.parent?.children[node.parent.children.indexOf(node)+1]||null,
  patchProp(node,key,old,value){node.props[key]=value;if(['value','checked','selected','type','multiple'].includes(key))node[key]=value;if(key==='value')node._value=value;},
  insertStaticContent(text,parent,anchor){const node=element('#static',text);node.parent=parent;const at=anchor?parent.children.indexOf(anchor):-1;parent.children.splice(at<0?parent.children.length:at,0,node);return [node,node];},
});
function walk(node,visible=false){return [node,...node.children.filter(c=>!visible||node.tag!=='details'||node.props.open||c.tag==='summary').flatMap(c=>walk(c,visible))];}
function fixture(){
  const state={accounts:[],books:[],jobs:[],automatic:[],calls:[],probe:async()=>structuredClone(preview),startError:null,resumeError:null};
  const api={
    accountList:async()=>structuredClone(state.accounts),snapshot:async()=>structuredClone({books:state.books,jobs:state.jobs}),
    createAccount:async()=>{const account={id:'original-account',platform:'qidian',label:'我的起点账户'};state.accounts.push({account,session:'closed',blocked_navigation:false});return account;},
    openAccount:async(id,url,login)=>{state.calls.push(['open',id,url,login]);state.accounts.find(a=>a.account.id===id).session='open_unverified';},
    previewAccountPage:async()=>{state.calls.push(['preview']);return state.probe();},
    confirmAccountPage:async(...args)=>{state.calls.push(['confirm',...args]);state.books=[structuredClone(stored)];return {kind:'catalog',book_id:stored.id,save:null};},
    startAccountDownload:async(...args)=>{state.calls.push(['start',...args]);if(state.startError)throw state.startError;state.jobs=[{id:'original-job',book_id:stored.id,source:'qidian-session',title:stored.title,state:'running',saved:0,total:2,blocked:0,failed:0,error:null}];return 'original-job';},
    accountDownloadJobs:async()=>structuredClone(state.jobs),pauseJob:async()=>{state.jobs[0].state='paused';},resumeAccountDownload:async(...args)=>{state.calls.push(['resume',...args]);if(state.resumeError)throw state.resumeError;state.jobs[0].state='running';},
    focusAccount:async(id)=>{state.calls.push(['focus',id]);},
    cancelJob:async()=>{},preferences:async()=>[],automaticTxtSnapshot:async()=>structuredClone(state.automatic),sourceCapabilities:async()=>[{id:'qimao-web',can_download:true}],
    inspectBook:async()=>{state.calls.push(['inspect']);return {...preview.catalog,source:'qimao-web'};},
    downloadBook:async(...args)=>{state.calls.push(['download',...args]);return 'public-job';},jobDetails:async()=>({job:{book_id:stored.id},items:[]}),
    revealExport:async(id)=>{state.calls.push(['reveal',id]);},
    openOfficialReader:async(...args)=>{state.calls.push(['official-open',...args]);},
    inspectOfficialReader:async(p)=>{state.calls.push(['official-inspect',p]);return {token:'official-token',page:state.officialPage||{platform:p,status:'text',book_title:'原创旅行',chapter_title:'原创第一章',paragraphs:['原创页面内容。'],declared_words:null}};},
    saveOfficialReader:async(...args)=>{state.calls.push(['official-save',...args]);return {id:'official-export',path:'D:/original-visible.txt'};},
    revealOfficialReader:async(id)=>{state.calls.push(['official-reveal',id]);},
    closeOfficialReader:async(p)=>{state.calls.push(['official-close',p]);},
  };
  return {state,api};
}
const flush=async()=>{await new Promise(resolve=>setImmediate(resolve));await nextTick();};
async function mount(component,props={},setup=()=>{}){
  const f=fixture();setup(f);globalThis.__simpleApi=f.api;
  globalThis.Document=class {};globalThis.ShadowRoot=class {};
  globalThis.document=Object.assign(new globalThis.Document(),{hidden:false,activeElement:null,addEventListener(){},removeEventListener(){},getElementById:()=>null});
  globalThis.window={addEventListener(){},removeEventListener(){}};
  const values=reactive(props);const root=element('root');const app=renderer.createApp({render:()=>h(component,values)});app.mount(root);await flush();await flush();
  return {...f,root,props:values,close:()=>app.unmount(),button:(label)=>walk(root,true).find(n=>n.tag==='button'&&n.textContent===label)};
}
async function click(button){assert.ok(button,'button exists');assert.ok(!button.props.disabled,'button is enabled');button.props.onClick?.({preventDefault(){}});await flush();}
async function waitFor(predicate){const deadline=Date.now()+5000;while(!predicate()){if(Date.now()>deadline)throw Error('controlled component wait timed out');await new Promise(resolve=>setTimeout(resolve,20));await flush();}}
async function loginAndRead(f){await click(f.button('打开起点，去登录'));await click(f.button('我已登录，下一步'));await waitFor(()=>f.button('是这本，开始保存为 TXT'));}
async function acknowledge(f){const input=walk(f.root,true).find(n=>n.tag==='input'&&n.type==='checkbox');assert.ok(input);input.checked=true;input.dispatch('change');await flush();}

test('new-book guide logs in, reads automatically, and starts once only after explicit confirmation',async()=>{
  const f=await mount(Account,{initialUrl:link});try{
    await loginAndRead(f);assert.ok(f.button('是这本，开始保存为 TXT').props.disabled);assert.equal(f.state.calls.filter(c=>c[0]==='start').length,0);
    await acknowledge(f);const button=f.button('是这本，开始保存为 TXT');button.props.onClick();button.props.onClick();await flush();await flush();
    const starts=f.state.calls.filter(c=>c[0]==='start');assert.equal(starts.length,1);assert.deepEqual(starts[0].slice(1),['original-account','original-book','',true]);assert.equal(f.state.calls.filter(c=>c[0]==='confirm').length,1);
    assert.ok(f.button('暂停保存'));assert.equal(f.button('是这本，开始保存为 TXT'),undefined);
  }finally{f.close();}
});
test('verification interrupts catalog preparation and a manual retry never starts saving by itself',async()=>{
  const f=await mount(Account,{initialUrl:link},({state})=>{state.probe=async()=>{throw '官网要求验证码，请本人处理后重新读取';};});try{
    await click(f.button('打开起点，去登录'));await click(f.button('我已登录，下一步'));await waitFor(()=>f.button('我已处理，重新读取'));
    assert.equal(f.state.calls.filter(c=>c[0]==='preview').length,1);assert.equal(f.state.calls.filter(c=>c[0]==='start').length,0);
    f.state.probe=async()=>structuredClone(preview);await click(f.button('我已处理，重新读取'));await waitFor(()=>f.button('是这本，开始保存为 TXT'));assert.ok(f.button('是这本，开始保存为 TXT').props.disabled);
  }finally{f.close();}
});
test('a failed start retains confirmed book and offers a direct retry without re-confirming the catalog',async()=>{
  const f=await mount(Account,{initialUrl:link},({state})=>{state.startError='网络连接失败';});try{
    await loginAndRead(f);await acknowledge(f);await click(f.button('是这本，开始保存为 TXT'));await flush();await flush();
    assert.match(f.root.textContent,/网络连接失败/);assert.ok(f.button('开始保存为 TXT'));assert.equal(f.state.books.length,1);
    f.state.startError=null;await acknowledge(f);await click(f.button('开始保存为 TXT'));assert.equal(f.state.calls.filter(c=>c[0]==='confirm').length,1);assert.equal(f.state.jobs.length,1);
  }finally{f.close();}
});
test('paused saving resumes the original task and complete books offer no duplicate start',async()=>{
  const setup=({state})=>{state.books=[structuredClone(stored)];state.jobs=[{id:'original-job',book_id:stored.id,state:'paused',title:stored.title,saved:1,total:2}];};
  const f=await mount(AutoSave,{accountId:'original-account',preferredBook:stored.id,opened:true,revision:0,guided:true,hideExport:true},setup);try{
    await acknowledge(f);await click(f.button('继续保存'));assert.equal(f.state.calls.filter(c=>c[0]==='resume').length,1);assert.equal(f.state.calls.filter(c=>c[0]==='start').length,0);
  }finally{f.close();}
  const done=await mount(AutoSave,{accountId:'original-account',preferredBook:stored.id,opened:true,revision:0,guided:true,hideExport:true},({state})=>{state.books=[{...stored,saved_chapters:2}];});try{assert.match(done.root.textContent,/不需要再下载/);assert.equal(done.button('开始保存为 TXT'),undefined);}finally{done.close();}
});
test('beginner home keeps advanced entry points collapsed and reuses an existing Qidian book',async()=>{
  const f=await mount(App,{},({state})=>{state.books=[structuredClone(stored)];});try{
    const visible=walk(f.root,true);assert.equal(visible.filter(n=>n.tag==='button'&&n.textContent==='平台账户').length,0);
    assert.ok(f.button('开始保存为 TXT').props.disabled);
    const input=visible.find(n=>n.props.id==='novel-link');input.value=link;input.dispatch('input');await flush();
    const form=walk(f.root,true).find(n=>n.tag==='form');form.props.onSubmit({preventDefault(){}});await flush();
    assert.ok(walk(f.root).find(n=>n.props['data-stub']==='BookWorkspace.vue'&&n.textContent===stored.id));assert.equal(f.state.calls.length,0);
  }finally{f.close();}
});
test('changing the book during a pending catalog read cannot confirm or start the old book',async()=>{
  let finish;const f=await mount(Account,{initialUrl:link},({state})=>{state.probe=()=>new Promise(resolve=>{finish=resolve;});});try{
    await click(f.button('打开起点，去登录'));await click(f.button('我已登录，下一步'));await waitFor(()=>finish);
    f.props.initialUrl='https://www.qidian.com/book/999/';await flush();finish(structuredClone(preview));await flush();await flush();
    assert.equal(f.button('是这本，开始保存为 TXT'),undefined);assert.equal(f.state.calls.filter(c=>['confirm','start'].includes(c[0])).length,0);
  }finally{f.close();}
});
test('public links save directly and an existing public book is reused without a second task',async()=>{
  for(const existing of [false,true]){
    const f=await mount(App,{},({state})=>{if(existing)state.books=[{...stored,source:'qimao-web'}];});try{
      const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://www.qimao.com/shuku/123/';input.dispatch('input');await flush();
      walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
      assert.equal(f.state.calls.filter(c=>c[0]==='download').length,existing?0:1);assert.ok(walk(f.root).find(n=>n.props['data-stub']==='BookWorkspace.vue'&&n.textContent===stored.id));
    }finally{f.close();}
  }
});
test('bookshelf opens the actual automatic TXT record without another export step',async()=>{
  const f=await mount(App,{},({state})=>{state.books=[{...stored,saved_chapters:2}];state.automatic=[{book_id:stored.id,state:'ready',export_id:'original-txt',receipt:{path:'D:/original.txt'}}];});try{
    await click(walk(f.root,true).find(n=>n.tag==='button'&&n.textContent.includes('我的书库')));await click(f.button('打开 TXT 文件夹'));
    assert.deepEqual(f.state.calls,[['reveal','original-txt']]);
  }finally{f.close();}
});
test('official login flow requires readable preview and acknowledgement before a current-page export',async()=>{
  const f=await mount(Official,{initialUrl:'https://b.faloo.com/123.html'});
  try{
    assert.ok(f.button('我已登录，识别当前页').props.disabled);
    await click(f.button('打开飞卢，去登录'));
    await click(f.button('我已登录，识别当前页'));
    assert.ok(f.button('确认，保存当前页为 TXT').props.disabled);
    await acknowledge(f);await click(f.button('确认，保存当前页为 TXT'));
    assert.deepEqual(f.state.calls.find(c=>c[0]==='official-save'),['official-save','faloo','official-token',true]);
    await click(f.button('打开 TXT 所在文件夹'));
    assert.ok(f.state.calls.some(c=>c[0]==='official-reveal'&&c[1]==='official-export'));
    assert.equal(f.state.calls.filter(c=>c[0]==='start'||c[0]==='download').length,0);
  }finally{f.close();}
});
test('protected official pages never expose a save action or start an automatic job',async()=>{
  const f=await mount(Official,{initialUrl:'https://www.ciweimao.com/book/123'},({state})=>{state.officialPage={status:'protected',book_title:'原创图片页',paragraphs:[]};});
  try{await click(f.button('打开刺猬猫，去登录'));await click(f.button('我已登录，识别当前页'));assert.match(f.root.textContent,/图片、画布/);assert.equal(f.button('确认，保存当前页为 TXT'),undefined);assert.equal(f.state.calls.filter(c=>c[0]==='official-save').length,0);}finally{f.close();}
});
test('new official links route to their account reader without public HTTP download',async()=>{
  for(const url of ['https://www.ciweimao.com/book/123','https://b.faloo.com/123.html','https://www.cmread.com/yuekan/novel/123.html']){
    const f=await mount(App);
    try{const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value=url;input.dispatch('input');await flush();walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();assert.match(f.root.textContent,/登录官网，识别当前页/);assert.equal(f.state.calls.filter(c=>c[0]==='inspect'||c[0]==='download').length,0);}finally{f.close();}
  }
});
test('verification recovery preserves the page and job until manual completion then resumes the same task',async()=>{
  const f=await mount(AutoSave,{accountId:'original-account',preferredBook:stored.id,opened:true,revision:0,guided:true,hideExport:true},({state})=>{
    state.books=[{...stored,saved_chapters:1}];state.jobs=[{id:'original-job',book_id:stored.id,state:'partial',title:stored.title,saved:1,total:2,failed:1,blocked:0,error:'读取期间页面导航发生变化，自动保存已停止'}];state.resumeError='官网验证仍未完成，请本人完成后再继续';
  });try{
    await click(f.button('打开验证窗口'));assert.deepEqual(f.state.calls,[['focus','original-account']]);
    await acknowledge(f);await click(f.button('我已完成验证，继续保存'));assert.equal(f.state.jobs[0].state,'partial');assert.equal(f.state.jobs[0].saved,1);assert.match(f.root.textContent,/验证仍未完成/);
    f.state.resumeError=null;await click(f.button('我已完成验证，继续保存'));assert.equal(f.state.jobs[0].state,'running');assert.equal(f.state.jobs[0].saved,1);
    assert.equal(f.state.calls.filter(c=>c[0]==='start'||c[0]==='open').length,0);const resumed=f.state.calls.filter(c=>c[0]==='resume');assert.ok(resumed.every(c=>c[2]==='original-job'&&c[4]===3000));
  }finally{f.close();}
});
