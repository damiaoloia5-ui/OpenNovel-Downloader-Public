<script setup lang="ts">
import NovelReader from './NovelReader.vue';
import {ref,computed,onMounted} from 'vue';
import {previewCatalogUpdate,acceptCatalogUpdate,refreshBookChapter,bookHistory,historyDetails,readHistory,exportHistory,type UpdatePreview,type HistoryEntry,type BookDetails,type ChapterContent} from './api';
const props=defineProps<{bookId:string;chapterId:string|null;online:boolean}>();const emit=defineEmits<{changed:[]}>();
const working=ref(false);const error=ref('');const notice=ref('');const proposal=ref<UpdatePreview|null>(null);const entries=ref<HistoryEntry[]>([]);const detail=ref<BookDetails|null>(null);const opened=ref('');const filter=ref('');const chapter=ref<ChapterContent|null>(null);
const visible=computed(()=>detail.value?.chapters.filter(c=>c.saved && `${c.position} ${c.title}`.includes(filter.value)).slice(0,100)||[]);
const changeLabels:Record<string,string>={removed:'移出当前目录',added:'新增',moved_or_renamed:'重排或改名',access_changed:'访问状态变化'};
async function reload(){entries.value=await bookHistory(props.bookId);}
async function action(work:()=>Promise<void>){if(working.value)return;working.value=true;error.value='';notice.value='';try{await work();await reload();}catch(e){error.value=String(e);}finally{working.value=false;}}
function preview(){void action(async()=>{proposal.value=await previewCatalogUpdate(props.bookId);if(!proposal.value.diff.changed)notice.value='当前官网目录与本地一致。';});}
function accept(){void action(async()=>{const token=proposal.value?.token;if(!token)return;proposal.value=null;await acceptCatalogUpdate(token);notice.value='已保留旧快照并应用目录。新增或缺失正文可通过书库补齐任务保存。';emit('changed');});}
function refresh(){void action(async()=>{if(!props.chapterId)return;const result=await refreshBookChapter(props.bookId,props.chapterId);notice.value=result.changed?'本章正文已更新，旧版保留在历史快照。':'本章正文未发现变化。';emit('changed');});}
function open(id:string){void action(async()=>{opened.value=id;detail.value=await historyDetails(id);chapter.value=null;filter.value='';});}
function read(id:string){void action(async()=>{chapter.value=await readHistory(opened.value,id);});}
function save(id:string,format:'txt'|'epub'){void action(async()=>{const r=await exportHistory(id,format);notice.value=`历史快照已导出 ${r.saved_chapters}/${r.total_chapters} 章：${r.path}`;emit('changed');});}
onMounted(()=>{void action(reload);});
</script>
<template>
  <details class="revision-panel">
    <summary>目录变更与历史版本</summary><p class="footnote">目录变化先预览，应用前保存快照。检查正文修订只请求当前预览的已保存章节，失败保留旧文；VIP 和锁定项不请求。</p>
    <div v-if="online" class="actions"><button :disabled="working" @click="preview">预览官网目录变更</button><button :disabled="working||!chapterId" @click="refresh">检查当前预览章节修订</button></div>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <section v-if="proposal?.diff.changed" aria-label="目录变更预览"><p>目录或作品信息有变化，章节变化 {{ proposal.diff.changes.length }} 项。</p><div v-for="field in proposal.diff.metadata" :key="field.field"><h4>{{ ({title:'书名',author:'作者',description:'简介'} as Record<string,string>)[field.field]||field.field }}</h4><p>原值：{{ field.previous||'未提供' }}</p><p>新值：{{ field.proposed||'未提供' }}</p></div><ol><li v-for="(change,index) in proposal.diff.changes.slice(0,100)" :key="index">{{ changeLabels[change.kind]||change.kind }} · {{ change.previous_position||'—' }} → {{ change.position||'—' }} · {{ change.previous_title||'—' }} → {{ change.title||'—' }}</li></ol><p v-if="proposal.diff.changes.length>100" class="footnote">此处显示前 100 项。确认后保留全部旧目录及已保存正文；可从历史快照查看完整旧版。</p><button :disabled="working" @click="accept">保留快照并应用目录</button></section>
    <h3>历史快照</h3><p v-if="!entries.length" class="footnote">尚无修订历史。目录或正文实际变化后生成快照。</p>
    <ul class="history-list"><li v-for="entry in entries.slice(0,20)" :key="entry.id"><strong>{{ entry.title }}</strong><small>{{ entry.created_at }} UTC · {{ entry.reason==='chapter_refresh'?'正文修订前':'目录更新前' }} · {{ entry.saved }}/{{ entry.total }} 章</small><div class="actions"><button :disabled="working" @click="open(entry.id)">查看旧版章节</button><button :disabled="working||!entry.saved" @click="save(entry.id,'txt')">导出旧版 TXT</button><button :disabled="working||!entry.saved" @click="save(entry.id,'epub')">导出旧版 EPUB</button></div></li></ul>
    <section v-if="detail" aria-label="历史快照预览"><h3>{{ detail.book.title }} · {{ detail.book.saved_chapters }}/{{ detail.book.total_chapters }} 章</h3><label>查找旧版章节<input v-model="filter" type="search" placeholder="序号或章节名"></label><p class="footnote">显示匹配的前 100 个已保存章节。</p><div class="history-chapters"><button v-for="c in visible" :key="c.id" :disabled="working" @click="read(c.id)">{{ c.position }} · {{ c.title }}</button></div><NovelReader v-if="chapter" :chapter="chapter" :book-title="detail.book.title+' · 历史版本'" /></section>
  </details>
</template>
<style scoped>
.revision-panel{margin:20px 0;border-top:1px solid var(--border);padding-top:18px}summary{cursor:pointer;font-weight:600}.actions{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0}.history-list{list-style:none;padding:0}.history-list li{padding:12px 0;border-bottom:1px solid var(--border)}.history-list small{display:block;color:#748069;margin-top:5px}.history-chapters{display:grid;gap:6px;max-height:280px;overflow:auto;margin:12px 0}.history-chapters button{text-align:left}.reader-text{max-height:460px;overflow:auto}
</style>
