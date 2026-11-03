import test from 'node:test';
import assert from 'node:assert/strict';
import {readingFontSize,readingParagraphs} from '../apps/desktop/src/novelTypography.ts';
import {build} from 'vite';
import {mkdtemp,mkdir} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createSSRApp} from 'vue';
import {renderToString} from '@vue/server-renderer';

test('reading paragraphs handle CRLF, old indentation and blank lines without modifying source',()=>{
  const original=Object.freeze(['　　清晨，窗开了。\r\n\r\n  “风从何处来？”','\t桌上有 A  B 两个字。\u2028后记。',' \t']);
  const before=JSON.stringify(original);
  assert.deepEqual(readingParagraphs(original),['清晨，窗开了。','“风从何处来？”','桌上有 A  B 两个字。','后记。']);
  assert.equal(JSON.stringify(original),before);
  for(const value of [null,'',undefined,0,15,29,'NaN',18.5])assert.equal(readingFontSize(value),20);
  assert.equal(readingFontSize('24'),24);
});

test('actual novel reader renders safe text with title, paragraph boundaries and accessible font controls',async()=>{
  await mkdir('.artifacts',{recursive:true});const output=await mkdtemp(resolve('.artifacts/novel-reader-'));
  await build({configFile:'apps/desktop/vite.config.ts',logLevel:'error',build:{ssr:'apps/desktop/src/NovelReader.vue',outDir:output,emptyOutDir:false,rollupOptions:{output:{entryFileNames:'reader.mjs'}}}});
  const {default:Reader}=await import(pathToFileURL(join(output,'reader.mjs')).href);
  const html=await renderToString(createSSRApp(Reader,{bookTitle:'原创山间来信',chapter:{id:'1',title:'第一章 风',paragraphs:['　　第一段。\n第二段。','<script>仅为原文</script>']}}));
  assert.match(html,/原创山间来信/);assert.match(html,/第一章 风/);assert.match(html,/3 段/);
  assert.match(html,/&lt;script&gt;仅为原文&lt;\/script&gt;/);assert.doesNotMatch(html,/<script>/);
  assert.match(html,/aria-label="放大字号"/);assert.match(html,/tabindex="0"/);
});
