import test from 'node:test';
import assert from 'node:assert/strict';
import {qidianBookLink,qidianBookId} from '../apps/desktop/src/workflow.ts';

test('routes clean official book and catalog links to the same guided work',()=>{
  for(const input of ['https://www.qidian.com/book/123',' https://www.qidian.com/book/123/ ','https://www.qidian.com/book/123/catalog/']){
    assert.equal(qidianBookLink(input),'https://www.qidian.com/book/123/');
    assert.equal(qidianBookId(input),'123');
  }
});
test('does not route foreign origins, credential-bearing or parameterized links',()=>{
  for(const input of ['https://www.qidian.com.evil/book/123/','http://www.qidian.com/book/123/','https://user@www.qidian.com/book/123/','https://www.qidian.com:9999/book/123/','https://www.qidian.com/book/123/?token=test','https://www.qidian.com/book/123/#catalog'])assert.equal(qidianBookLink(input),null);
});
test('chapter, invalid and other-source inputs remain outside the account-book flow',()=>{
  for(const input of ['https://www.qidian.com/chapter/123/456/','https://www.qidian.com/book/123catalog/','https://www.qidian.com/book/abc/','qimao:123','123','https://fanqienovel.com/page/123',''])assert.equal(qidianBookId(input),null,input);
});
