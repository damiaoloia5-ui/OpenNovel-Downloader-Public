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
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as Platform} from '+JSON.stringify(resolve('apps/desktop/src/PlatformSavePanel.vue'))+';\n');
const real=new Set(['AccountPanel.vue','AutoSavePanel.vue','App.vue','OfficialReaderPanel.vue','PlatformSavePanel.vue']);
real.add('LocalEnginePanel.vue');
real.add('SaveLocation.vue');
real.add('SaveOptions.vue');
real.add('ProfilePanel.vue');
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as Profiles} from '+JSON.stringify(resolve('apps/desktop/src/ProfilePanel.vue'))+';\n');
real.add('ExportActions.vue');
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as ExportActions} from '+JSON.stringify(resolve('apps/desktop/src/ExportActions.vue'))+';\n');
real.add('AutomaticTxtPanel.vue');
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as AutoTxt} from '+JSON.stringify(resolve('apps/desktop/src/AutomaticTxtPanel.vue'))+';\n');
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as Engine} from '+JSON.stringify(resolve('apps/desktop/src/LocalEnginePanel.vue'))+';\n');
real.add('TxtSupplementPanel.vue');
await writeFile(entry,(await readFile(entry,'utf8'))+'\nexport {default as Supplement} from '+JSON.stringify(resolve('apps/desktop/src/TxtSupplementPanel.vue'))+';\n');
await build({configFile:false,logLevel:'error',plugins:[{
  name:'controlled-vue-host',enforce:'pre',
  resolveId(id){if(id==='./api')return mockFile;if(id==='@tauri-apps/api/window')return windowFile;},
  async transform(source,id){if(!id.endsWith('.vue'))return;
    if(!real.has(basename(id)))return `import {h} from 'vue';export default {props:['bookId'],render(){return h('section',{'data-stub':${JSON.stringify(basename(id))}},this.bookId||'');}};`;
    const {descriptor}=parse(source,{filename:id});const compiled=compileScript(descriptor,{id:basename(id),inlineTemplate:true,templateOptions:{compilerOptions:{hoistStatic:false}}});
    return transformWithOxc(compiled.content,`${id}.ts`,{lang:'ts'});
  },
}],build:{ssr:entry,outDir:output,emptyOutDir:false,rollupOptions:{output:{entryFileNames:'components.mjs'}}}});
const {Account,AutoSave,App,Official,Supplement,Engine,AutoTxt,Platform,ExportActions,Profiles}=await import(pathToFileURL(join(output,'components.mjs')).href);
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
    localProfiles:async()=>({current_id:'original-user',profiles:[{id:'original-user',name:'原创用户',database:null}],data_directory:'D:/OriginalUser/profiles/original-user'}),
    accountList:async()=>structuredClone(state.accounts),snapshot:async()=>structuredClone({books:state.books,jobs:state.jobs}),
    snapshotIfChanged:async known=>{const snapshot=structuredClone({books:state.books,jobs:state.jobs});const revision=JSON.stringify(snapshot);return known===revision?null:{revision,snapshot};},
    createAccount:async(platform='qidian',label='我的起点账户')=>{const account={id:'original-account',platform,label};state.accounts.push({account,session:'closed',blocked_navigation:false});return account;},
    openAccount:async(id,url,login)=>{state.calls.push(['open',id,url,login]);state.accounts.find(a=>a.account.id===id).session='open_unverified';},
    previewAccountPage:async()=>{state.calls.push(['preview']);return state.probe();},
    confirmAccountPage:async(...args)=>{state.calls.push(['confirm',...args]);state.books=[structuredClone(stored)];return {kind:'catalog',book_id:stored.id,save:null};},
    startAccountDownload:async(...args)=>{state.calls.push(['start',...args]);if(state.startError)throw state.startError;state.jobs=[{id:'original-job',book_id:stored.id,source:'qidian-session',title:stored.title,state:'running',saved:0,total:2,blocked:0,failed:0,error:null}];return 'original-job';},
    accountDownloadJobs:async()=>structuredClone(state.jobs),pauseJob:async()=>{state.jobs[0].state='paused';},resumeAccountDownload:async(...args)=>{state.calls.push(['resume',...args]);if(state.resumeError)throw state.resumeError;state.jobs[0].state='running';},
    focusAccount:async(id)=>{state.calls.push(['focus',id]);},
    cancelJob:async()=>{},preferences:async()=>[],automaticTxtSnapshot:async()=>structuredClone(state.automatic),sourceCapabilities:async()=>[{id:'qimao-web',can_download:true}],
    localEngineStatus:async()=>({enabled:false,ready:false,message:'原创组件未安装'}),
    inspectBook:async()=>{state.calls.push(['inspect']);return {...preview.catalog,source:'qimao-web'};},
    downloadBook:async(...args)=>{state.calls.push(['download',...args]);return 'public-job';},jobDetails:async()=>({job:{book_id:stored.id},items:[]}),
    revealExport:async(id)=>{state.calls.push(['reveal',id]);},
    defaultExportRoot:async()=>'D:/OriginalApp/exports',
    chooseSaveDirectory:async()=>null,
    bookExportDestination:async()=>({directory:'D:/OriginalApp/exports/原创',custom:false,filename:'本地－原创－作者.txt'}),
    setBookExportDirectory:async(...args)=>{state.calls.push(['set-directory',...args]);},
    importBook:async(...args)=>{state.calls.push(['import',...args]);return 'original-job';},
    openOfficialReader:async(...args)=>{state.calls.push(['official-open',...args]);},
    inspectOfficialReader:async(p)=>{state.calls.push(['official-inspect',p]);return {token:'official-token',page:state.officialPage||{platform:p,status:'text',book_title:'原创旅行',chapter_title:'原创第一章',paragraphs:['原创页面内容。'],declared_words:null}};},
    saveOfficialReader:async(...args)=>{state.calls.push(['official-save',...args]);return {id:'official-export',path:'D:/original-visible.txt'};},
    chooseOfficialExportDirectory:async()=>{state.calls.push(['official-folder']);return 'D:/chosen-original';},
    revealOfficialReader:async(id)=>{state.calls.push(['official-reveal',id]);},
    closeOfficialReader:async(p)=>{state.calls.push(['official-close',p]);},
    previewTxtSupplement:async(...args)=>{state.calls.push(['txt-preview',...args]);if(state.txtError)throw state.txtError;return {fingerprint:'original-fingerprint',title:'原创样书',total:3,already_saved:1,first_title:'第一章 原创',last_title:'第三章 原创'};},
    confirmTxtSupplement:async(...args)=>{state.calls.push(['txt-confirm',...args]);return {total:3,already_saved:1};},
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

function openEngineOptions(f){walk(f.root).find(n=>n.tag==='details').props.open=true;}
const txtReady={state:'ready',export_id:'original-txt',receipt:{path:'D:/original.txt',saved_chapters:2,total_chapters:2,partial:false}};
test('missing automatic TXT gives a local regeneration action with honest request feedback',async()=>{
  const f=await mount(AutoTxt,{bookId:'original-book',saved:2,status:txtReady},({state,api})=>{
    api.revealExport=async id=>{state.calls.push(['reveal',id]);throw '导出文件已移动或删除';};
    api.retryAutomaticTxt=async id=>{state.calls.push(['regenerate',id]);};
  });try{
    await click(f.button('打开 TXT 所在文件夹'));assert.match(f.root.textContent,/导出文件已移动或删除/);
    await click(f.button('重新生成 TXT'));assert.match(f.root.textContent,/已请求按原范围与格式重新生成/);
    assert.deepEqual(f.state.calls,[['reveal','original-txt'],['regenerate','original-book']]);
  }finally{f.close();}
});
test('automatic TXT regeneration is blocked while saving and rejects duplicate requests',async()=>{
  let finish;const f=await mount(AutoTxt,{bookId:'original-book',saved:2,active:true,status:txtReady},({state,api})=>{
    api.retryAutomaticTxt=id=>{state.calls.push(['regenerate',id]);return new Promise(resolve=>{finish=resolve;});};
  });try{
    const button=f.button('重新生成 TXT');assert.ok(button.props.disabled);button.props.onClick();await flush();assert.deepEqual(f.state.calls,[]);
    f.props.active=false;await flush();button.props.onClick();button.props.onClick();await flush();
    assert.deepEqual(f.state.calls,[['regenerate','original-book']]);finish();await flush();
  }finally{f.close();}
});
test('late TXT failures and acknowledgements cannot cross the selected book',async()=>{
  for(const failed of [false,true]){
    let finish;const f=await mount(AutoTxt,{bookId:'original-book',saved:2,status:txtReady},({api})=>{
      api.retryAutomaticTxt=()=>new Promise((resolve,reject)=>{finish=()=>failed?reject('旧书原创失败'):resolve();});
    });try{
      await click(f.button('重新生成 TXT'));f.props.bookId='next-book';await flush();finish();await flush();
      assert.doesNotMatch(f.root.textContent,/旧书原创失败|已请求按原范围与格式重新生成/);
    }finally{f.close();}
  }
});

test('remembered enhancement can be forgotten even when its runtime is unavailable',async()=>{
  const f=await mount(Engine,{current:{enabled:false,ready:false,installed:false,remembered:true,message:'原创组件失效',issue:'component_unavailable',next_step:'重新安装完整一体化版，原书库无需删除。'}},({state,api})=>{
    api.setLocalEngine=async enabled=>{state.calls.push(['set-engine',enabled]);return {enabled:false,ready:false,remembered:false,message:'原创组件仍未就绪'};};
  });try{
    openEngineOptions(f);assert.match(f.root.textContent,/下一步：重新安装完整/);
    assert.equal(f.button('重新准备本地增强'),undefined);
    await click(f.button('关闭并忘记选择'));
    assert.deepEqual(f.state.calls,[['set-engine',false]]);
    assert.equal(f.button('关闭并忘记选择'),undefined);
  }finally{f.close();}
});
test('failed preparation retains recovery and forget actions without starting a download',async()=>{
  let attempts=0;const f=await mount(Engine,{current:{enabled:false,ready:false,installed:true,remembered:true,message:'原创服务暂不可用',next_step:'等待 Linux 服务就绪后重新准备。'}},({state,api})=>{
    api.setLocalEngine=async enabled=>{state.calls.push(['set-engine',enabled]);if(++attempts===1)throw '原创导入失败：检查可用空间';return {enabled:true,ready:true,remembered:true,installed:true,message:'原创组件已就绪'};};
  });try{
    openEngineOptions(f);await click(f.button('重新准备本地增强'));assert.match(f.root.textContent,/原创导入失败/);
    assert.ok(f.button('关闭并忘记选择'));await click(f.button('重新准备本地增强'));
    assert.match(f.root.textContent,/已启用并记住/);assert.deepEqual(f.state.calls,[['set-engine',true],['set-engine',true]]);
  }finally{f.close();}
});
test('preparation shows its own busy state, rejects duplicate actions and ignores an abandoned result',async()=>{
  let finish,checked=0;const f=await mount(Engine,{current:{enabled:false,ready:false,installed:true,remembered:false,message:'首次准备'},onChecked:()=>checked++},({state,api})=>{
    api.setLocalEngine=enabled=>{state.calls.push(['set-engine',enabled]);return new Promise(resolve=>{finish=resolve;});};
  });try{
    openEngineOptions(f);const button=f.button('启用并记住选择');button.props.onClick();button.props.onClick();await flush();
    assert.match(f.root.textContent,/正在校验并准备内置引擎/);assert.ok(f.button('重新检查组件').props.disabled);
    assert.deepEqual(f.state.calls,[['set-engine',true]]);f.close();finish({enabled:true,ready:true,remembered:true,message:'已准备'});await flush();
    assert.equal(checked,0);
  }finally{f.close();}
});
test('read-only recheck displays recovery steps without enabling the component',async()=>{
  const f=await mount(Engine,{current:{enabled:false,ready:false,installed:true,remembered:false,message:'首次状态'}},({state,api})=>{
    api.localEngineStatus=async()=>{state.calls.push(['check-engine']);return {enabled:false,ready:false,installed:true,remembered:false,message:'Windows 容器模式',issue:'docker_windows_mode',next_step:'手动切换到 Linux 容器后重新检查。'};};
  });try{
    openEngineOptions(f);await click(f.button('重新检查组件'));assert.match(f.root.textContent,/手动切换到 Linux 容器/);
    assert.deepEqual(f.state.calls,[['check-engine']]);assert.ok(f.button('启用并记住选择'));
  }finally{f.close();}
});

test('TXT supplementation previews before explicit confirmation and retains the selected book identity',async()=>{
  const f=await mount(Supplement,{bookId:'original-book',disabled:false});try{
    const input=walk(f.root).find(n=>n.tag==='input');
    input.props.onChange({target:{files:[{name:'original.txt',size:100,arrayBuffer:async()=>new TextEncoder().encode('原创文件').buffer}]}});await flush();await flush();
    assert.equal(f.state.calls.filter(c=>c[0]==='txt-confirm').length,0);
    assert.match(f.root.textContent,/匹配 3 章/);assert.match(f.root.textContent,/将补齐 2 章/);
    await click(f.button('确认是同一本书，补齐并自动生成 TXT'));
    assert.deepEqual(f.state.calls.find(c=>c[0]==='txt-confirm'),['txt-confirm','original-book','原创文件','original-fingerprint']);
    assert.match(f.root.textContent,/已补齐 2 章/);
  }finally{f.close();}
});

test('invalid TXT and oversized files cannot offer a confirm action',async()=>{
  const f=await mount(Supplement,{bookId:'original-book',disabled:false},({state})=>{state.txtError='TXT 章节缺失';});try{
    const input=walk(f.root).find(n=>n.tag==='input');
    await input.props.onChange({target:{files:[{name:'large.txt',size:17*1024*1024}]}});await flush();
    assert.equal(f.state.calls.length,0);assert.match(f.root.textContent,/16 MiB/);
    await input.props.onChange({target:{files:[{name:'missing.txt',size:10,arrayBuffer:async()=>new TextEncoder().encode('原创文件').buffer}]}});await flush();
    assert.match(f.root.textContent,/TXT 章节缺失/);assert.equal(f.button('确认是同一本书，补齐并自动生成 TXT'),undefined);
  }finally{f.close();}
});

test('new-book guide logs in, reads automatically, and starts once only after explicit confirmation',async()=>{
  const f=await mount(Account,{initialUrl:link});try{
    await loginAndRead(f);assert.ok(f.button('是这本，开始保存为 TXT').props.disabled);assert.equal(f.state.calls.filter(c=>c[0]==='start').length,0);
    await acknowledge(f);const button=f.button('是这本，开始保存为 TXT');button.props.onClick();button.props.onClick();await flush();await flush();
    const starts=f.state.calls.filter(c=>c[0]==='start');assert.equal(starts.length,1);assert.deepEqual(starts[0].slice(1),['original-account','original-book','',true,3000,{directory:null,format:'txt',selection_only:false}]);assert.equal(f.state.calls.filter(c=>c[0]==='confirm').length,1);
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

test('unchanged refresh skips preferences, keeps automatic status live and retries a failed changed refresh',async()=>{
  let reads=0,fail=false;
  const f=await mount(App,{},({api})=>{api.preferences=async()=>{reads++;if(fail)throw '原创暂时不可读';return [];};});
  try {
    assert.equal(reads,1);
    await click(f.button('保存进度'));
    await click(f.button('刷新状态'));assert.equal(reads,1);
    f.state.books=[{...stored,saved_chapters:2}];fail=true;
    await click(f.button('刷新状态'));assert.equal(reads,2);assert.match(f.root.textContent,/原创暂时不可读/);
    fail=false;await click(f.button('刷新状态'));assert.equal(reads,3);
    f.state.automatic=[{book_id:stored.id,...txtReady}];
    await click(f.button('刷新状态'));assert.equal(reads,3);
    await click(walk(f.root,true).find(n=>n.tag==='button'&&n.textContent.startsWith('我的书库')));
    await click(f.button('打开文件夹'));
    assert.deepEqual(f.state.calls,[['reveal','original-txt']]);
  } finally {f.close();}
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

test('choose destination before saving; cancelling the picker preserves it and returning to default is explicit',async()=>{
  const f=await mount(App,{},({api})=>{let picks=0;api.chooseSaveDirectory=async()=>++picks===1?'D:/我的小说':null;});
  try{
    assert.match(f.root.textContent,/D:\/OriginalApp\/exports/);
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://www.qimao.com/shuku/123/';input.dispatch('input');await flush();
    await click(f.button('选择文件夹'));await click(f.button('选择文件夹'));
    assert.match(f.root.textContent,/D:\/我的小说/);assert.equal(f.state.calls.length,0);
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
    assert.deepEqual(f.state.calls.find(c=>c[0]==='download').at(-1),{directory:'D:/我的小说',format:'txt',selection_only:false});
  }finally{f.close();}
  const g=await mount(App,{},({api})=>{api.chooseSaveDirectory=async()=>'D:/custom';});
  try{await click(g.button('选择文件夹'));await click(g.button('使用默认位置'));assert.doesNotMatch(g.root.textContent,/D:\/custom/);assert.match(g.root.textContent,/D:\/OriginalApp\/exports/);}finally{g.close();}
});

test('an open folder picker prevents starting a download and choosing a folder alone creates no task',async()=>{
  let finish;const f=await mount(App,{},({api})=>{api.chooseSaveDirectory=()=>new Promise(resolve=>{finish=resolve;});});
  try{
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://www.qimao.com/shuku/123/';input.dispatch('input');await flush();
    await click(f.button('选择文件夹'));assert.ok(f.button('开始保存为 TXT').props.disabled);
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();assert.equal(f.state.calls.length,0);
    finish('D:/chosen');await flush();assert.equal(f.state.calls.length,0);assert.equal(f.button('开始保存为 TXT').props.disabled,false);
  }finally{f.close();}
});

test('JSON import and account confirmation carry the folder into the start command',async()=>{
  const f=await mount(App,{},({api})=>{api.chooseSaveDirectory=async()=>'D:/JSON小说';});
  try{await click(f.button('选择文件夹'));const input=walk(f.root).find(n=>n.tag==='input'&&n.props.accept==='.json,application/json');await input.props.onChange({target:{files:[{size:10,text:async()=>'original-json'}]}});await flush();assert.deepEqual(f.state.calls.find(c=>c[0]==='import'),['import','original-json',{directory:'D:/JSON小说',format:'txt'}]);}finally{f.close();}
  const g=await mount(Account,{initialUrl:link,initialDirectory:'D:/账户小说'});
  try{await loginAndRead(g);assert.match(g.root.textContent,/D:\/账户小说/);await acknowledge(g);await click(g.button('是这本，开始保存为 TXT'));assert.deepEqual(g.state.calls.find(c=>c[0]==='start').at(-1),{directory:'D:/账户小说',format:'txt',selection_only:false});}finally{g.close();}
});

test('resume preserves existing destination unless explicitly changed and does not resume if updating it fails',async()=>{
  const f=await mount(AutoSave,{accountId:'original-account',preferredBook:stored.id,opened:true,revision:0,guided:true},({state,api})=>{
    state.books=[structuredClone(stored)];state.jobs=[{id:'original-job',book_id:stored.id,state:'paused',title:stored.title,saved:1,total:2}];
    api.bookExportDestination=async()=>({directory:'D:/原来的目录',custom:true,filename:'原创.txt'});
    api.chooseSaveDirectory=async()=>'D:/新目录';api.setBookExportDirectory=async()=>{throw '原创写入权限不足';};
  });
  try{assert.match(f.root.textContent,/D:\/原来的目录/);await click(f.button('选择文件夹'));await acknowledge(f);await click(f.button('继续保存'));assert.match(f.root.textContent,/原创写入权限不足/);assert.equal(f.state.calls.filter(c=>c[0]==='resume').length,0);assert.equal(f.state.jobs[0].saved,1);}finally{f.close();}
});

test('starting an existing account book refreshes a folder changed by another panel instead of overwriting it',async()=>{
  const f=await mount(AutoSave,{accountId:'original-account',preferredBook:stored.id,opened:true,revision:0,guided:true},({state,api})=>{
    state.books=[structuredClone(stored)];let calls=0;
    api.bookExportDestination=async()=>({directory:++calls===1?'D:/旧位置':'D:/工作台刚选的位置',custom:true,filename:'原创.txt'});
  });
  try{assert.match(f.root.textContent,/D:\/旧位置/);await acknowledge(f);await click(f.button('开始保存为 TXT'));assert.deepEqual(f.state.calls.find(c=>c[0]==='start').at(-1),{directory:'D:/工作台刚选的位置',format:'txt',selection_only:false});}finally{f.close();}
});
test('a ready local component requires explicit activation before saving App-only free chapters',async()=>{
  const f=await mount(App,{},({state,api})=>{
    state.engineEnabled=false;
    api.localEngineStatus=async()=>({enabled:state.engineEnabled,ready:true,message:'原创组件就绪'});
    api.setLocalEngine=async(enabled)=>{state.calls.push(['enable-engine',enabled]);state.engineEnabled=enabled;return api.localEngineStatus();};
    api.sourceCapabilities=async()=>[{id:'fanqie-web',can_download:true},{id:'fanqie-app-local',can_download:state.engineEnabled}];
    api.inspectBook=async()=>{state.calls.push(['inspect',state.engineEnabled]);return {...preview.catalog,source:state.engineEnabled?'fanqie-app-local':'fanqie-web',chapters:preview.catalog.chapters.map((c,i)=>({...c,restricted:!state.engineEnabled&&i===1}))};};
  });try{
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://fanqienovel.com/page/10000000001';input.dispatch('input');await flush();
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
    assert.equal(f.state.calls.filter(c=>c[0]==='download').length,0);
    await click(f.button('启用本地组件，保存更多免费章节'));await flush();await flush();
    assert.equal(f.state.calls.filter(c=>c[0]==='enable-engine').length,1);
    const downloads=f.state.calls.filter(c=>c[0]==='download');assert.equal(downloads.length,1);assert.match(downloads[0][4],/2/);
    assert.deepEqual(f.state.calls.filter(c=>c[0]==='inspect'),[['inspect',false],['inspect',true]]);
  }finally{f.close();}
});
test('unavailable components leave the ordinary web save usable',async()=>{
  const f=await mount(App,{},({api})=>{
    api.sourceCapabilities=async()=>[{id:'fanqie-web',can_download:true}];
    api.inspectBook=async()=>({...preview.catalog,source:'fanqie-web',chapters:preview.catalog.chapters.map((c,i)=>({...c,restricted:i===1}))});
  });try{
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://fanqienovel.com/page/10000000001';input.dispatch('input');await flush();
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
    assert.equal(f.state.calls.filter(c=>c[0]==='download').length,1);
    assert.equal(f.button('启用本地组件，保存更多免费章节'),undefined);
  }finally{f.close();}
});
test('failed or late component activation cannot start saving the wrong book',async()=>{
  for(const late of [false,true]){
    let finish;const f=await mount(App,{},({api})=>{
      api.localEngineStatus=async()=>({enabled:false,ready:true,message:'原创组件就绪'});
      api.sourceCapabilities=async()=>[{id:'fanqie-web',can_download:true}];
      api.inspectBook=async()=>({...preview.catalog,source:'fanqie-web',chapters:preview.catalog.chapters.map((c,i)=>({...c,restricted:i===1}))});
      api.setLocalEngine=()=>late?new Promise(resolve=>{finish=resolve;}):Promise.reject('原创组件检查失败');
    });try{
      const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://fanqienovel.com/page/10000000001';input.dispatch('input');await flush();
      walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
      await click(f.button('启用本地组件，保存更多免费章节'));
      if(late){input.value='https://fanqienovel.com/page/10000000002';input.dispatch('input');await flush();finish({enabled:true,ready:true,message:'原创组件就绪'});await flush();await flush();}
      else assert.match(f.root.textContent,/原创组件检查失败/);
      assert.equal(f.state.calls.filter(c=>c[0]==='download').length,0);
    }finally{f.close();}
  }
});
test('a restored choice rechecks a stale web catalog and saves without another enable click',async()=>{
  let inspections=0;const f=await mount(App,{},({state,api})=>{
    api.localEngineStatus=async()=>({enabled:true,ready:true,remembered:true,message:'已恢复选择'});
    api.sourceCapabilities=async()=>[{id:'fanqie-web',can_download:true},{id:'fanqie-app-local',can_download:true}];
    api.inspectBook=async()=>{state.calls.push(['inspect',++inspections]);return {...preview.catalog,source:inspections===1?'fanqie-web':'fanqie-app-local',chapters:preview.catalog.chapters.map((c,i)=>({...c,restricted:inspections===1&&i===1}))};};
  });try{
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://fanqienovel.com/page/10000000001';input.dispatch('input');await flush();
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
    assert.equal(inspections,2);const downloads=f.state.calls.filter(c=>c[0]==='download');assert.equal(downloads.length,1);assert.match(downloads[0][4],/2/);
    assert.equal(f.button('启用本地组件，保存更多免费章节'),undefined);
  }finally{f.close();}
});
test('a remembered unavailable component explains recovery before an explicit web-only fallback',async()=>{
  const f=await mount(App,{},({api})=>{
    api.localEngineStatus=async()=>({enabled:false,ready:false,remembered:true,message:'请先启动 Docker Desktop，再重新检查'});
    api.sourceCapabilities=async()=>[{id:'fanqie-web',can_download:true}];
    api.inspectBook=async()=>({...preview.catalog,source:'fanqie-web',chapters:preview.catalog.chapters.map((c,i)=>({...c,restricted:i===1}))});
  });try{
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://fanqienovel.com/page/10000000001';input.dispatch('input');await flush();
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
    assert.equal(f.state.calls.filter(c=>c[0]==='download').length,0);assert.match(f.root.textContent,/请先启动 Docker Desktop/);
    await click(f.button('仅保存目录标记开放章（1）'));assert.equal(f.state.calls.filter(c=>c[0]==='download').length,1);
  }finally{f.close();}
});
test('saving with a remembered installed component prepares it and continues in one action',async()=>{
  let enabled=false;const f=await mount(App,{},({state,api})=>{
    api.localEngineStatus=async()=>({enabled,ready:enabled,installed:true,remembered:true,message:'隔离服务尚未启动'});
    api.setLocalEngine=async()=>{state.calls.push(['prepare']);enabled=true;return api.localEngineStatus();};
    api.sourceCapabilities=async()=>[{id:'fanqie-web',can_download:true},{id:'fanqie-app-local',can_download:enabled}];
    api.inspectBook=async()=>({...preview.catalog,source:enabled?'fanqie-app-local':'fanqie-web',chapters:preview.catalog.chapters.map((c,i)=>({...c,restricted:!enabled&&i===1}))});
  });try{
    const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://fanqienovel.com/page/10000000001';input.dispatch('input');await flush();
    walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
    assert.equal(f.state.calls.filter(c=>c[0]==='prepare').length,1);
    const downloads=f.state.calls.filter(c=>c[0]==='download');assert.equal(downloads.length,1);assert.match(downloads[0][4],/2/);
  }finally{f.close();}
});
test('bookshelf opens the actual automatic TXT record without another export step',async()=>{
  const f=await mount(App,{},({state})=>{state.books=[{...stored,saved_chapters:2}];state.automatic=[{book_id:stored.id,state:'ready',export_id:'original-txt',receipt:{path:'D:/original.txt'}}];});try{
    await click(walk(f.root,true).find(n=>n.tag==='button'&&n.textContent.includes('我的书库')));await click(f.button('打开文件夹'));
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
    assert.deepEqual(f.state.calls.find(c=>c[0]==='official-save'),['official-save','faloo','official-token',true,undefined,'txt']);
    await click(f.button('打开 TXT 所在文件夹'));
    assert.ok(f.state.calls.some(c=>c[0]==='official-reveal'&&c[1]==='official-export'));
    assert.equal(f.state.calls.filter(c=>c[0]==='start'||c[0]==='download').length,0);
  }finally{f.close();}
});
test('migu public excerpt can be exported without login and never starts a complete-book job',async()=>{
 const url='https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123';
 const f=await mount(Official,{initialUrl:url},({state})=>{state.officialPage={status:'excerpt',book_title:'原创山路',author:'原创作者',paragraphs:['原创试读'],declared_words:null};});
 try{
  await click(f.button('打开这本书'));await click(f.button('识别官网试读'));assert.match(f.root.textContent,/官网公开试读/);
  assert.ok(f.button('确认，保存当前页为 TXT').props.disabled);await click(f.button('选择 TXT 保存文件夹'));await acknowledge(f);await click(f.button('确认，保存当前页为 TXT'));
  assert.deepEqual(f.state.calls.find(c=>c[0]==='official-open'),['official-open','migu',url,false]);
   assert.deepEqual(f.state.calls.find(c=>c[0]==='official-save'),['official-save','migu','official-token',true,'D:/chosen-original','txt']);assert.equal(f.state.calls.filter(c=>c[0]==='start'||c[0]==='download').length,0);
 }finally{f.close();}
});
test('protected official pages never expose a save action or start an automatic job',async()=>{
  const f=await mount(Official,{initialUrl:'https://www.ciweimao.com/book/123'},({state})=>{state.officialPage={status:'protected',book_title:'原创图片页',paragraphs:[]};});
  try{await click(f.button('打开刺猬猫，去登录'));await click(f.button('我已登录，识别当前页'));assert.match(f.root.textContent,/图片、画布/);assert.equal(f.button('确认，保存当前页为 TXT'),undefined);assert.equal(f.state.calls.filter(c=>c[0]==='official-save').length,0);}finally{f.close();}
});

test('official excerpt inherits Word selection and exports with its visible scope',async()=>{
  const f=await mount(Official,{initialUrl:'https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123',initialFormat:'docx'},({state,api})=>{
    state.officialPage={status:'excerpt',book_title:'原创山路',paragraphs:['原创试读'],declared_words:null};
    api.saveOfficialReader=async(...args)=>{state.calls.push(['official-save',...args]);return {id:'word-export',path:'D:/original-excerpt.docx'};};
  });
  try {
    await click(f.button('打开这本书'));await click(f.button('识别官网试读'));
    assert.ok(f.button('确认，保存当前页为 Word').props.disabled);
    await acknowledge(f);await click(f.button('确认，保存当前页为 Word'));
    assert.deepEqual(f.state.calls.find(c=>c[0]==='official-save'),['official-save','migu','official-token',true,undefined,'docx']);
    await click(f.button('打开 Word 所在文件夹'));
    assert.ok(f.state.calls.some(c=>c[0]==='official-reveal'&&c[1]==='word-export'));
    assert.equal(f.state.calls.filter(c=>['start','download','import'].includes(c[0])).length,0);
  } finally {f.close();}
});
test('late official opening and failures cannot authorize or label a different selected book',async()=>{
  for(const failed of [false,true]){
    let finish;const f=await mount(Official,{initialUrl:'https://b.faloo.com/123.html'},({api})=>{
      api.openOfficialReader=()=>new Promise((resolve,reject)=>{finish=()=>failed?reject('旧作品原创错误'):resolve();});
    });try{
      await click(f.button('打开飞卢，去登录'));f.props.initialUrl='https://www.ciweimao.com/book/456';await flush();finish();await flush();
      assert.ok(f.button('我已登录，识别当前页').props.disabled);assert.doesNotMatch(f.root.textContent,/旧作品原创错误|请在官网自行登录/);
    }finally{f.close();}
  }
});
test('late official exports stay with the original context and never appear under a new book',async()=>{
  let finish;const f=await mount(Official,{initialUrl:'https://b.faloo.com/123.html'},({api})=>{
    api.saveOfficialReader=()=>new Promise(resolve=>{finish=()=>resolve({id:'old-export',path:'D:/old-original.txt'});});
  });try{
    await click(f.button('打开飞卢，去登录'));await click(f.button('我已登录，识别当前页'));await acknowledge(f);
    await click(f.button('确认，保存当前页为 TXT'));f.props.initialUrl='https://b.faloo.com/456.html';await flush();finish();await flush();
    assert.equal(f.button('打开 TXT 所在文件夹'),undefined);assert.doesNotMatch(f.root.textContent,/old-original|TXT 已保存/);
  }finally{f.close();}
});
test('new official links route to their account reader without public HTTP download',async()=>{
  for(const url of ['https://www.ciweimao.com/book/123','https://b.faloo.com/123.html']){
    const f=await mount(App);
    try{const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value=url;input.dispatch('input');await flush();walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();assert.match(f.root.textContent,url.includes('cmread.com')?/登录官网，识别当前页/:/登录官网，连续保存/);assert.equal(f.state.calls.filter(c=>c[0]==='inspect'||c[0]==='download').length,0);}finally{f.close();}
  }
});
test('migu official book links start only catalog-marked free chapters through the normal queue',async()=>{
  for(const url of ['https://www.cmread.com/yuekan/novel/123.html','https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123']) {
    const f=await mount(App,{},({api,state})=>{
      api.sourceCapabilities=async()=>[{id:'migu-web',can_download:true}];
      api.inspectBook=async()=>{state.calls.push(['inspect']);return {...preview.catalog,source:'migu-web',chapters:[{id:'1',title:'原创免费章',restricted:false},{id:'2',title:'原创受限章',restricted:true}]};};
    });
    try {
      const input=walk(f.root,true).find(n=>n.props.id==='novel-link');
      input.value=url;input.dispatch('input');await flush();
      walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
      const downloads=f.state.calls.filter(c=>c[0]==='download');
      assert.equal(downloads.length,1);assert.equal(downloads[0][4],'1');
      assert.equal(f.state.calls.filter(c=>c[0]==='official-open'||c[0]==='start').length,0);
    } finally {f.close();}
  }
});
test('source list reflects Migu public saving and retains the account verification boundary',async()=>{
  const f=await mount(App,{},({api})=>{
    api.sourceCapabilities=async()=>[{id:'migu-web',name:'咪咕官网免费章节',can_inspect:true,can_download:true,note:'仅明确免费章节',example:'https://www.cmread.com/yuekan/novel/123.html'}];
  });
  try {
    walk(f.root).find(n=>n.tag==='details'&&n.props.class==='nav-more').props.open=true;
    await click(f.button('小说来源'));
    const card=walk(f.root).find(n=>n.tag==='article'&&n.textContent.includes('咪咕官网免费章节'));
    assert.ok(card);
    assert.match(card.textContent,/免费正文保存/);
    assert.doesNotMatch(card.textContent,/账户当前页实验/);
    assert.match(f.root.textContent,/真实已购全书仍待验收/);
    assert.doesNotMatch(f.root.textContent,/咪咕仅作品页识别/);
  } finally {f.close();}
});
test('multi-platform catalog confirmation starts the matching session queue after explicit acknowledgement',async()=>{
 const links={zongheng:'https://www.zongheng.com/detail/123','17k':'https://www.17k.com/book/123.html',jinjiang:'https://www.jjwxc.net/onebook.php?novelid=123',qimao:'https://www.qimao.com/shuku/123/',shuqi:'https://www.shuqi.com/book/123.html',ciweimao:'https://www.ciweimao.com/book/123',faloo:'https://b.faloo.com/123.html'};
 for(const [p,url] of Object.entries(links)){
  const b={...stored,source:p+'-session'};const f=await mount(Platform,{initialUrl:url,initialOptions:{range:'range',first:2,last:2,format:'docx'}},({state,api})=>{
   api.previewPlatformCatalog=async(id,link)=>{state.calls.push(['platform-preview',id,link]);return {...preview,catalog:{...preview.catalog,source:p+'-session'}};};
   api.confirmAccountPage=async(...args)=>{state.calls.push(['confirm',...args]);state.books=[b];return {kind:'catalog',book_id:b.id};};
   api.startAccountDownload=async(...args)=>{state.calls.push(['start',...args]);state.jobs=[{id:'platform-job',book_id:b.id,source:b.source,title:b.title,state:'running',saved:0,total:2,failed:0,blocked:0}];return 'platform-job';};
  });try{
   await click(f.button('打开官网，去登录'));assert.equal(f.state.accounts[0].account.platform,p);
   await click(f.button('我已登录，读取作品目录'));assert.equal(f.button('是这本，开始保存为 Word').props.disabled,true);
   await acknowledge(f);await click(f.button('是这本，开始保存为 Word'));await flush();
   assert.deepEqual(f.state.calls.find(c=>c[0]==='platform-preview'),['platform-preview','original-account',url]);
   assert.deepEqual(f.state.calls.find(c=>c[0]==='start'),['start','original-account','original-book','2-2',true,3000,{directory:null,format:'docx',selection_only:true}]);
   assert.equal(f.state.calls.some(c=>c[0]==='download'),false);
  }finally{f.close();}
 }
});
test('a late multi-platform preview cannot attach to a newly selected work',async()=>{
 let finish;const f=await mount(Platform,{initialUrl:'https://www.ciweimao.com/book/123'},({api})=>{api.previewPlatformCatalog=()=>new Promise(resolve=>{finish=()=>resolve(preview);});});
 try{await click(f.button('打开官网，去登录'));await click(f.button('我已登录，读取作品目录'));f.props.initialUrl='https://b.faloo.com/456.html';await flush();finish();await flush();assert.equal(f.button('是这本，开始保存为 TXT'),undefined);assert.equal(f.state.calls.some(c=>c[0]==='start'),false);}finally{f.close();}
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

async function selectSaveOptions(f,first=2,last=2,format='docx'){
 const fields=walk(f.root,true).filter(n=>n.tag==='fieldset'&&n.props.class==='save-options');assert.ok(fields.length);
 const field=fields[0];const selects=walk(field).filter(n=>n.tag==='select');
 selects[0].props.onChange({target:{value:'range'}});await flush();selects[1].props.onChange({target:{value:format}});await flush();
 const inputs=walk(field).filter(n=>n.tag==='input');inputs[0].props.onInput({target:{value:String(first)}});await flush();inputs[1].props.onInput({target:{value:String(last)}});await flush();
}
test('public range and Word choice are visible before starting and survive catalog discovery',async()=>{
 for(const existing of [false,true]){
  const f=await mount(App,{},({state})=>{if(existing)state.books=[{...stored,source:'qimao-web'}];});try{
   await selectSaveOptions(f);assert.ok(f.button('开始保存为 Word'));assert.equal(f.state.calls.length,0);
   const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://www.qimao.com/shuku/123/';input.dispatch('input');await flush();
   walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
   assert.deepEqual(f.state.calls.find(c=>c[0]==='download'),['download','https://www.qimao.com/shuku/123/',1,2,'2-2',{directory:null,format:'docx',selection_only:true}]);
  }finally{f.close();}
 }
});
test('invalid or out of catalog range cannot create a public download',async()=>{
 for(const [first,last] of [[0,2],[2,1],[1,9]]){
  const f=await mount(App);try{
   await selectSaveOptions(f,first,last);const input=walk(f.root,true).find(n=>n.props.id==='novel-link');input.value='https://www.qimao.com/shuku/123/';input.dispatch('input');await flush();
   walk(f.root,true).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();assert.equal(f.state.calls.filter(c=>c[0]==='download').length,0);assert.match(f.root.textContent,/起止章节|超过目录总数/);
  }finally{f.close();}
 }
});
test('account confirmation sends the chosen chapter range and Word format before the worker starts',async()=>{
 const f=await mount(Account,{initialUrl:link});try{
  await loginAndRead(f);await selectSaveOptions(f);await acknowledge(f);await click(f.button('是这本，开始保存为 Word'));
  assert.deepEqual(f.state.calls.find(c=>c[0]==='start'),['start','original-account','original-book','2-2',true,3000,{directory:null,format:'docx',selection_only:true}]);
 }finally{f.close();}
});
test('Word automatic result is labeled as Word and uses its recorded file',async()=>{
 const f=await mount(AutoTxt,{bookId:'original-book',saved:2,status:{...txtReady,receipt:{...txtReady.receipt,path:'D:/original.docx'}}});try{
  assert.match(f.root.textContent,/Word 已生成/);await click(f.button('打开 Word 所在文件夹'));assert.deepEqual(f.state.calls,[['reveal','original-txt']]);
 }finally{f.close();}
});

test('already saved books export only the requested range as Word without a download',async()=>{
 const f=await mount(ExportActions,{bookId:'original-book',saved:2,total:2,initialOptions:{range:'range',first:2,last:2,format:'docx'}},({state,api})=>{
  api.exportBook=async(...args)=>{state.calls.push(['export',...args]);return {path:'D:/original.docx',saved_chapters:1,total_chapters:1,partial:false};};api.bookDetails=async()=>({exports:[]});
 });try{await click(f.button('导出文件'));assert.deepEqual(f.state.calls,[['export','original-book','docx','2-2']]);assert.match(f.root.textContent,/1\/1/);}finally{f.close();}
});

test('local profile creation does not copy books or switch until requested',async()=>{
 const profiles=[{id:'a',name:'原创甲',database:null}];
 const f=await mount(Profiles,{},({state,api})=>{
  api.localProfiles=async()=>({current_id:'a',profiles:structuredClone(profiles),data_directory:'D:/original/a'});
  api.createLocalProfile=async name=>{const p={id:'b',name,database:null};profiles.push(p);state.calls.push(['create-profile',name]);return p;};
  api.switchLocalProfile=async id=>{state.calls.push(['switch-profile',id]);};
 });try{
  walk(f.root).find(n=>n.tag==='details').props.open=true;
  const input=walk(f.root).find(n=>n.tag==='input');input.value='原创乙';input.dispatch('input');await flush();
  walk(f.root).find(n=>n.tag==='form').props.onSubmit({preventDefault(){}});await flush();await flush();
  assert.deepEqual(f.state.calls,[['create-profile','原创乙']]);assert.match(f.root.textContent,/书库为空/);
  await click(f.button('切换用户'));assert.deepEqual(f.state.calls.at(-1),['switch-profile','b']);
  assert.match(f.root.textContent,/重新打开所选用户/);
 }finally{f.close();}
});
test('a blocked profile switch keeps the current user and shows recovery',async()=>{
 const f=await mount(Profiles,{},({api})=>{
  api.localProfiles=async()=>({current_id:'a',profiles:[{id:'a',name:'原创甲'},{id:'b',name:'原创乙'}],data_directory:'D:/original/a'});
  api.switchLocalProfile=async()=>{throw new Error('请先暂停当前保存任务');};
 });try{
  walk(f.root).find(n=>n.tag==='details').props.open=true;
  const select=walk(f.root).find(n=>n.tag==='select');select.options[0].selected=false;select.options[1].selected=true;select.dispatch('change');await flush();
  await click(f.button('切换用户'));assert.match(f.root.textContent,/当前用户：原创甲/);assert.match(f.root.textContent,/请先暂停/);assert.ok(f.button('切换用户'));
 }finally{f.close();}
});
