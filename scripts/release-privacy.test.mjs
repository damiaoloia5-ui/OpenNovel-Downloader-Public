import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import {verifyExact,verifyConfig} from './release-privacy.mjs';
test('release config permits only reviewed files and a per-user installer',()=>{assert.ok(verifyConfig());});
test('personal books, profiles, cookies and hidden files cannot ride along with a bundle',()=>{
 for(const name of ['library.db','exports/novel.txt','profiles.json','ui-webview/Cookies','.env','unexpected.json']){
  const root=mkdtempSync(join(tmpdir(),'opennovel-privacy-'));writeFileSync(join(root,'LICENSE'),'original license');
  const expected={LICENSE:createHash('sha256').update('original license').digest('hex')};assert.equal(verifyExact(root,expected),1);
  const file=join(root,name);mkdirSync(join(file,'..'),{recursive:true});writeFileSync(file,'original private canary');
  assert.throws(()=>verifyExact(root,expected),/Unexpected or missing/);
 }
});
test('renaming private data to an allowed filename still fails the trusted hash check',()=>{
 const root=mkdtempSync(join(tmpdir(),'opennovel-privacy-'));writeFileSync(join(root,'LICENSE'),'private original canary');
 assert.throws(()=>verifyExact(root,{LICENSE:'0'.repeat(64)}),/Resource content differs/);
});
