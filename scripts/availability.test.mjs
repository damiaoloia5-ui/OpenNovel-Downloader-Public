import test from 'node:test';
import assert from 'node:assert/strict';
import {catalogAvailability,chapterEvidence,missingGroups} from '../apps/desktop/src/availabilityModel.ts';
const catalog={source:'qimao-web',chapters:Array.from({length:8},(_,i)=>({id:`chapter-${i}`,title:`原创第${i+1}章`,restricted:[2,5,6].includes(i)}))};
const sources=[{id:'qimao-web',can_download:true}];
test('catalog selection uses positions and includes only marked open chapters',()=>{
  assert.deepEqual(catalogAvailability(catalog,sources),{supported:true,total:8,open:5,restricted:3,unverified:0,selection:'1-2,4-5,8'});
  assert.equal(catalog.chapters[0].id,'chapter-0');
});
test('unsupported, account-only and unknown sources do not gain public access',()=>{
  for(const source of ['shuqi-web','qidian-session','unknown']){
    const r=catalogAvailability({...catalog,source},[{id:source,can_download:false}]);
    assert.equal(r.open,0);assert.equal(r.unverified,8);assert.equal(r.restricted,0);assert.equal(r.selection,'');
  }
});
test('all restricted or empty catalog never offers a public selection',()=>{
  assert.equal(catalogAvailability({...catalog,chapters:catalog.chapters.map(c=>({...c,restricted:true}))},sources).selection,'');
  assert.equal(catalogAvailability({...catalog,chapters:[]},sources).open,0);
});
test('large alternating catalog preserves gaps and final position',()=>{
  const r=catalogAvailability({...catalog,chapters:Array.from({length:10000},(_,i)=>({id:`c${i}`,restricted:i%2===0}))},sources);
  assert.equal(r.open,5000);assert.equal(r.selection.split(',').length,5000);assert.match(r.selection,/,10000$/);
});
test('missing summaries count all chapters, not only a selected job',()=>{
  const chapters=[{saved:true,availability:{state:'restricted'}},{saved:false,availability:{state:'failed',reason:'原创网络错误',job_id:'a'}},{saved:false,availability:{state:'unattempted'}},{saved:false,availability:{state:'failed',job_id:'b'}}];
  assert.deepEqual(missingGroups(chapters).map(g=>[g.state,g.count]),[['failed',2],['unattempted',1]]);
  assert.equal(chapterEvidence(chapters[0]).state,'saved');assert.equal(chapterEvidence(chapters[1]).job_id,'a');
});
test('old payloads fall back to unknown, never infer login or subscription from absence',()=>{
  const r=chapterEvidence({saved:false});assert.equal(r.state,'unverified');assert.equal(r.job_id,null);
  assert.equal(missingGroups([]).length,0);
});
import {sourceHint} from '../apps/desktop/src/availabilityModel.ts';

test('source hint is exact-host display metadata, not a download permission',()=>{
  const sources=[{id:'qidian-session',name:'起点',example:'https://www.qidian.com/book/作品ID/',can_download:false,note:'全书待验收'}];
  assert.equal(sourceHint('https://www.qidian.com/book/123/',sources)?.can_download,false);
  for(const input of ['https://www.qidian.com.evil/book/1/','https://user@www.qidian.com/book/1/','http://www.qidian.com/book/1/','not a url'])assert.equal(sourceHint(input,sources),undefined);
});
