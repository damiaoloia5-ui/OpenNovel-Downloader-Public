<script setup lang="ts">
import {ref,watch} from 'vue';
import {previewAccountPage,confirmAccountPage,exportBook,type CapturePreview} from './api';
const props=defineProps<{accountId:string;opened:boolean}>();
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string]}>();
const busy=ref(false);const error=ref('');const notice=ref('');const preview=ref<CapturePreview|null>(null);const savedBook=ref('');
watch(()=>[props.accountId,props.opened],()=>{preview.value=null;});
async function action(work:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{await work();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function capture(kind:'catalog'|'chapter'){void action(async()=>{preview.value=null;preview.value=await previewAccountPage(props.accountId,kind);notice.value='已读取预览，尚未写入书库。请核对后确认；预览 10 分钟内有效。';});}
function confirm(){void action(async()=>{const current=preview.value;if(!current)return;preview.value=null;const result=await confirmAccountPage(props.accountId,current.token);savedBook.value=result.book_id;notice.value=result.save?`${result.save.already_saved?'该章已保存，未重复写入':'已保存第 '+result.save.position+' 章'}：${result.save.chapter_title}。本书已保存 ${result.save.saved}/${result.save.total} 章。`:'完整目录已保存。请到同一官网窗口打开一章完整正文，再点“预览当前章”。';emit('saved',result.book_id);});}
function save(format:'txt'|'epub'){void action(async()=>{const receipt=await exportBook(savedBook.value,format);notice.value=`已导出 ${receipt.saved_chapters}/${receipt.total_chapters} 章${receipt.partial?'（含缺章提示）':''}：${receipt.path}`;});}
</script>
<template>
  <details class="capture-panel">
    <summary>保存当前可读章节 · 实验性</summary>
    <ol class="footnote"><li>在本账户官网窗口打开作品，展开全部目录；读取并确认保存目录。</li><li>本人正常登录，打开有阅读权限的完整章节，再回来预览当前章。</li><li>核对标题与正文，确认保存。继续在官网手动打开下一章，逐章保存。</li></ol>
    <p class="footnote">只读取当前页面正常显示的文字；不自动翻页或购买。未购、试读、保护字形、页面结构未知时拒绝保存。已存正文保留，内容变化时不覆盖。</p>
    <div class="actions"><button :disabled="busy||!opened" @click="capture('catalog')">读取完整目录</button><button :disabled="busy||!opened" @click="capture('chapter')">预览当前章</button></div>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <section v-if="preview" class="capture-preview" aria-label="官网页面保存预览">
      <template v-if="preview.catalog"><h4>{{ preview.catalog.title }} · 目录预览</h4><p>{{ preview.catalog.author||'未标注作者' }} · {{ preview.catalog.chapters.length }} 章；目录不代表已获阅读权限。</p><ol><li v-for="c in preview.catalog.chapters.slice(0,100)" :key="c.id">{{ c.title }}</li></ol><p v-if="preview.catalog.chapters.length>100" class="footnote">预览显示前 100 项；确认后保存完整目录。</p></template>
      <article v-if="preview.chapter" aria-label="当前章正文预览"><h4>{{ preview.chapter.chapter.title }}</h4><p class="footnote">作品 ID {{ preview.chapter.remote_book_id }} · 官网标注 {{ preview.chapter.declared_words }} 字。请核对首尾与官网是否一致。</p><div class="preview-text"><p v-for="(p,index) in preview.chapter.chapter.paragraphs" :key="index">{{ p }}</p></div></article>
      <div class="actions"><button class="primary" :disabled="busy||!opened" @click="confirm">{{ preview.catalog?'确认保存完整目录':'确认保存当前章' }}</button><button :disabled="busy" @click="preview=null">放弃预览</button></div>
    </section>
    <div v-if="savedBook" class="actions"><button :disabled="busy" @click="emit('read',savedBook)">在书库查看</button><button :disabled="busy" @click="save('txt')">导出已保存 TXT</button><button :disabled="busy" @click="save('epub')">导出已保存 EPUB</button></div>
  </details>
</template>
<style scoped>
.capture-panel{margin-top:18px;border-top:1px solid var(--border);padding-top:16px}summary{cursor:pointer;font-weight:600}.actions{display:flex;flex-wrap:wrap;gap:10px;margin:14px 0}.footnote{line-height:1.8}.capture-preview{padding:16px;border:1px solid var(--border);border-radius:8px}.capture-preview ol,.preview-text{max-height:360px;overflow:auto}.preview-text{line-height:1.9;white-space:pre-wrap;overflow-wrap:anywhere}h4{font-size:17px}
</style>
