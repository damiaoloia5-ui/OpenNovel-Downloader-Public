import test from 'node:test';
import assert from 'node:assert/strict';
import {qidianBookLink,qidianBookId,saveRecovery} from '../apps/desktop/src/workflow.ts';

test('local engine failures explain validation instead of sending users to an account window',()=>{
  const advice=saveRecovery('第2章：本地引擎正文长度未通过校验（实际 3031，声明 3027）','engine');
  assert.match(advice,/未通过校验/);assert.match(advice,/已保存正文保留/);
  assert.doesNotMatch(advice,/官网窗口|重新登录/);
  assert.match(saveRecovery('限流 429','engine'),/等待/);
  assert.match(saveRecovery('HTTP 403 验证','engine'),/不使用官网登录窗口/);
  assert.match(saveRecovery('引擎退出','engine'),/组件状态/);
});

test('public access challenges never suggest an unrelated Qidian account window',()=>{
  const advice=saveRecovery('官网访问需处理：HTTP 403 验证','public');
  assert.match(advice,/保留进度/);assert.doesNotMatch(advice,/起点|官网窗口/);
  assert.match(saveRecovery('需要登录','account'),/起点官网窗口/);
});

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
