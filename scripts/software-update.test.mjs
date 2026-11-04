import test from 'node:test';
import assert from 'node:assert/strict';
import {build,transformWithOxc} from 'vite';
import {parse,compileScript} from '@vue/compiler-sfc';
import {mkdtemp,mkdir} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createRenderer,nextTick} from 'vue';

test('actual update panel checks on startup but installs only after user action; portable requires opt-in',async()=>{
  await mkdir('.artifacts',{recursive:true});
  const output=await mkdtemp(resolve('.artifacts/updater-ui-'));
  await build({configFile:false,logLevel:'error',plugins:[{
    name:'controlled-updater-ipc',enforce:'pre',
    resolveId(id){if(id==='./api')return '\0updater-fixture';},
    async transform(source,id){if(!id.endsWith('.vue'))return;const {descriptor}=parse(source,{filename:id});const compiled=compileScript(descriptor,{id:'updater-test',inlineTemplate:true,templateOptions:{compilerOptions:{hoistStatic:false}}});return transformWithOxc(compiled.content,`${id}.ts`,{lang:'ts'});},
    load(id){if(id==='\0updater-fixture')return `export const desktopAvailable=true;
      export const softwareUpdateStatus=async()=>globalThis.updateFixture.status;
      export const checkSoftwareUpdate=async()=>{globalThis.updateFixture.calls.push('check');return globalThis.updateFixture.status;};
      export const downloadSoftwareUpdate=async()=>{globalThis.updateFixture.calls.push('download');globalThis.updateFixture.status.phase='ready';return globalThis.updateFixture.status;};
      export const installSoftwareUpdate=async(convert)=>{globalThis.updateFixture.calls.push(['install',convert]);throw Error('原创忙碌任务：请先暂停');};
      export const cancelSoftwareDownload=async()=>{};export const openSoftwareReleases=async()=>{};`;}
  }],build:{ssr:'apps/desktop/src/SoftwareUpdatePanel.vue',outDir:output,emptyOutDir:false,rollupOptions:{output:{entryFileNames:'panel.mjs'}}}});
  const {default:Panel}=await import(pathToFileURL(join(output,'panel.mjs')).href);
  const node=(kind,text='')=>({kind,text,props:{},children:[],parent:null,addEventListener(){},removeEventListener(){}});
  const renderer=createRenderer({
    createElement:kind=>node(kind),createText:text=>node('#text',text),createComment:text=>node('#comment',text),
    setText:(n,t)=>{n.text=t;},setElementText:(n,t)=>{n.text=t;n.children=[];},
    patchProp:(n,k,_old,v)=>{n.props[k]=v;},parentNode:n=>n.parent,
    nextSibling:n=>n.parent?.children[n.parent.children.indexOf(n)+1]??null,
    insert(n,p,anchor){if(n.parent){const i=n.parent.children.indexOf(n);if(i>=0)n.parent.children.splice(i,1);}n.parent=p;const at=p.children.indexOf(anchor);p.children.splice(at<0?p.children.length:at,0,n);},
    remove(n){const i=n.parent?.children.indexOf(n)??-1;if(i>=0)n.parent.children.splice(i,1);},
  });
  const prior=globalThis.localStorage;
  globalThis.localStorage={getItem:()=>null,setItem:()=>{}};
  globalThis.updateFixture={calls:[],status:{phase:'available',current_version:'1.0.0',version:'1.1.0',notes:'<script>original note</script>',downloaded:0,total:null,message:'原创更新提示',backup_directory:null,portable:true}};
  const root=node('root');const app=renderer.createApp(Panel,{busy:false});
  const flatten=n=>[n,...n.children.flatMap(flatten)];
  const text=n=>n.text+n.children.map(text).join('');
  try{
    app.mount(root);
    await new Promise(r=>setTimeout(r,2700));await nextTick();
    assert.deepEqual(globalThis.updateFixture.calls,['check']);
    let button=flatten(root).find(n=>n.kind==='button'&&text(n).includes('下载并安装'));
    assert.equal(button.props.disabled,true);
    const label=flatten(root).find(n=>n.kind==='label'&&text(n).includes('同意转为安装版'));
    const input=label.children.find(n=>n.kind==='input');
    input.props['onUpdate:modelValue'](true);await nextTick();
    assert.equal(button.props.disabled,false);
    await button.props.onClick();await nextTick();
    assert.deepEqual(globalThis.updateFixture.calls,['check','download',['install',true]]);
    assert.match(text(root),/请先暂停/);
    button=flatten(root).find(n=>n.kind==='button'&&text(n).includes('备份书库并安装'));
    await button.props.onClick();await nextTick();
    assert.equal(globalThis.updateFixture.calls.filter(c=>c==='download').length,1,'retry reuses verified bytes');
    assert.equal(flatten(root).filter(n=>n.kind==='script').length,0,'release notes never execute HTML');
  }finally{app.unmount();globalThis.localStorage=prior;delete globalThis.updateFixture;}
});
