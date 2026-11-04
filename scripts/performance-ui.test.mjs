import test from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'vite';
import {mkdir,mkdtemp} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createSSRApp} from 'vue';
import {renderToString} from '@vue/server-renderer';
test('account speed selector exposes bounded modes without removing acknowledgement',async()=>{
  await mkdir('.artifacts',{recursive:true});const out=await mkdtemp(resolve('.artifacts/performance-render-'));
  await build({configFile:'apps/desktop/vite.config.ts',logLevel:'error',build:{ssr:'apps/desktop/src/AutoSavePanel.vue',outDir:out,emptyOutDir:false,rollupOptions:{output:{entryFileNames:'panel.mjs'}}}});
  const {default:Panel}=await import(pathToFileURL(join(out,'panel.mjs')).href);
  const html=await renderToString(createSSRApp(Panel,{accountId:'original',opened:false,preferredBook:'original-book',revision:0}));
  assert.match(html,/极速.*0.5 秒/);assert.match(html,/快速.*1 秒/);assert.match(html,/标准.*3 秒/);assert.match(html,/缓慢.*6 秒/);
  assert.match(html,/<option value="3000"[^>]*\bselected>/);assert.match(html,/确认只保存已有阅读权限/);assert.match(html,/遇到限流或验证仍会停止/);
});
