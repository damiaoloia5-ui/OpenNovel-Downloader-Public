import test from 'node:test';
import assert from 'node:assert/strict';
import {saveMode,workbenchStatus,publicAccessLimited,preferredExportFormat,rememberExportFormat,exportFormatKey} from '../apps/desktop/src/workbenchModel.ts';
const book={id:'original',title:'原创工作台样书',author:'测试作者',source:'qimao-web',remote_id:'123',saved_chapters:1,total_chapters:3};
const job=(state,rest={})=>({id:'job',source:book.source,book_id:book.id,title:book.title,state,saved:1,total:3,blocked:0,failed:0,error:null,...rest});

test('public web restrictions lead to local supplementation, not a no-op resume or account login',()=>{
  const restricted=job('partial',{blocked:2});
  assert.equal(publicAccessLimited(restricted,'public'),true);
  const status=workbenchStatus({...book,source:'fanqie-web'},[restricted],'public');
  assert.equal(status.kind,'limited');assert.match(status.message,/TXT/);assert.doesNotMatch(status.message,/官网窗口|登录/);
  assert.equal(publicAccessLimited(restricted,'account'),false);
  assert.equal(publicAccessLimited(job('partial',{blocked:1,failed:1}),'public'),false);
  assert.equal(publicAccessLimited(job('paused',{blocked:2}),'public'),false);
});

test('selected task completion cannot imply whole-book completion',()=>{
  const result=workbenchStatus(book,[job('completed',{total:1})],'public');
  assert.equal(result.kind,'missing');assert.equal(result.tab,'save');
});
test('active and paused work retain a task-specific next step',()=>{
  assert.equal(workbenchStatus({...book,saved_chapters:3},[job('running')],'public').kind,'running');
  const result=workbenchStatus(book,[job('paused')],'account');
  assert.equal(result.kind,'paused');assert.equal(result.job.id,'job');assert.match(result.message,/官网/);
});
test('whole-book completion leads to export without offering another download',()=>{
  const result=workbenchStatus({...book,saved_chapters:3},[job('failed')],'public');
  assert.equal(result.kind,'complete');assert.equal(result.tab,'export');
});
test('blocked content retains its reason and saved text',()=>{
  const result=workbenchStatus(book,[job('blocked',{blocked:1,error:'原创订阅提示'})],'account');
  assert.equal(result.kind,'attention');assert.equal(result.message,'原创订阅提示');assert.equal(book.saved_chapters,1);
});
test('foreign-book tasks never control the selected book',()=>{
  assert.equal(workbenchStatus(book,[job('running',{book_id:'another'})],'public').kind,'missing');
});
test('unsupported sources never gain download capability from existing text',()=>{
  assert.equal(saveMode('shuqi-web',[{id:'shuqi-web',can_download:false}]),'unavailable');
  assert.equal(workbenchStatus(book,[],'unavailable').tab,'read');
  assert.equal(workbenchStatus({...book,saved_chapters:0},[],'unavailable').tab,'save');
});
test('account, public and local flows remain distinct',()=>{
  assert.equal(saveMode('qidian-session',[{id:'qidian-session',can_download:false}]),'account');
  assert.equal(saveMode('local-json',[]),'local');assert.equal(saveMode('qimao-web',[{id:'qimao-web',can_download:true}]),'public');assert.equal(saveMode('unknown',[]),'unavailable');
});
test('cancelled and partially imported books still expose recovery information',()=>{
  assert.equal(workbenchStatus(book,[job('cancelled')],'public').kind,'attention');
  assert.equal(workbenchStatus({...book,saved_chapters:0},[],'local').kind,'local');
});
test('format preference survives reload and safely defaults on invalid or denied storage',()=>{
  const data=new Map();const storage={getItem:k=>data.get(k)??null,setItem:(k,v)=>data.set(k,v)};
  assert.equal(preferredExportFormat(storage),'txt');assert.equal(rememberExportFormat(storage,'epub'),true);assert.equal(preferredExportFormat(storage),'epub');
  data.set(exportFormatKey,'unexpected');assert.equal(preferredExportFormat(storage),'txt');
  const denied={getItem:()=>{throw Error('denied');},setItem:()=>{throw Error('denied');}};
  assert.equal(preferredExportFormat(denied),'txt');assert.equal(rememberExportFormat(denied,'epub'),false);
});
