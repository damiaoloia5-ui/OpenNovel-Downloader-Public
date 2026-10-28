// Compile and server-render the actual component; no browser or window control.
import test from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'vite';
import {mkdtemp,mkdir} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createSSRApp} from 'vue';
import {renderToString} from '@vue/server-renderer';

test('actual Vue missing-chapter panel renders counts, safe reasons and bounded lists',async()=>{
    await mkdir('.artifacts',{recursive:true});
    const output=await mkdtemp(resolve('.artifacts/availability-render-'));
    await build({configFile:'apps/desktop/vite.config.ts',logLevel:'error',build:{ssr:'apps/desktop/src/AvailabilityPanel.vue',outDir:output,emptyOutDir:false,rollupOptions:{output:{entryFileNames:'panel.mjs'}}}});
    const {default:Panel}=await import(pathToFileURL(join(output,'panel.mjs')).href);
    const render=chapters=>renderToString(createSSRApp(Panel,{chapters}));
    const saved=await render([{id:'saved',title:'原创首章',position:1,saved:true}]);
    assert.match(saved,/已收齐/);assert.doesNotMatch(saved,/查看全部缺失章节/);
    const chapters=[{id:'saved',title:'原创首章',position:1,saved:true},...Array.from({length:101},(_,i)=>({id:`missing-${i}`,title:`原创缺章${i+1}`,position:i+2,saved:false,availability:{state:'failed',reason:'<script>原创错误</script>',job_id:'original-job'}}))];
    const missing=await render(chapters);
    assert.match(missing,/尚缺 101 章/);assert.match(missing,/获取失败 101 章/);assert.match(missing,/1\/3/);
    assert.match(missing,/&lt;script&gt;原创错误&lt;\/script&gt;/);assert.doesNotMatch(missing,/<script>/);
    assert.equal((missing.match(/查看相关任务/g)||[]).length,50);assert.doesNotMatch(missing,/原创缺章51</);
    const unknown=await render([{id:'old',title:'旧格式原创章',position:1,saved:false}]);
    assert.match(unknown,/状态待核对/);assert.doesNotMatch(unknown,/查看相关任务/);
    const unsupported=await render([{id:'unsupported',title:'原创目录',position:1,saved:false,availability:{state:'unsupported',reason:'未开放正文',job_id:null}}]);
    assert.match(unsupported,/来源未开放 1 章/);assert.doesNotMatch(unsupported,/查看相关任务/);
});
