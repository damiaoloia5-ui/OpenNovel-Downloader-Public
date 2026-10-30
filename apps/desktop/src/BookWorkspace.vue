<script setup lang="ts">
import {computed,nextTick,onMounted,onUnmounted,ref,watch} from 'vue';
import AccountPanel from './AccountPanel.vue';
import ExportActions from './ExportActions.vue';
import RevisionPanel from './RevisionPanel.vue';
import AvailabilityPanel from './AvailabilityPanel.vue';
import AutomaticTxtPanel from './AutomaticTxtPanel.vue';
import type {AutoTxtStatus} from './api';
import {bookDetails,readChapter,jobDetails,pauseJob,resumeJob,cancelJob,updateBook,retryChapters,revealExport,organizeBook,type Book,type BookDetails,type BookPreferences,type ChapterContent,type Job,type JobDetails,type SourceInfo} from './api';
import {saveMode,workbenchStatus,type WorkbenchTab} from './workbenchModel';
import {saveRecovery} from './workflow';

const props=defineProps<{bookId:string;initialTab?:WorkbenchTab;accountId?:string;sources:SourceInfo[];jobs:Job[];summary?:Book;preference?:BookPreferences;automatic?:AutoTxtStatus}>();
const emit=defineEmits<{changed:[];back:[];tasks:[];help:[]}>();
const tab=ref<WorkbenchTab>(props.initialTab||'save');
const selectedAccount=ref(props.accountId||'');
const detail=ref<BookDetails|null>(null);const busy=ref(false);const error=ref('');const notice=ref('');
const heading=ref<HTMLElement|null>(null);const panel=ref<HTMLElement|null>(null);
const reader=ref<ChapterContent|null>(null);const search=ref('');const onlySaved=ref(true);const readerPage=ref(1);
const openedJob=ref('');const jobView=ref<JobDetails|null>(null);const itemPage=ref(1);const itemFilter=ref('missing');
const tags=ref('');const tagsDirty=ref(false);const preference=ref<BookPreferences|undefined>(props.preference);
let disposed=false;let loadSequence=0;let readSequence=0;let jobSequence=0;
const book=computed(()=>detail.value?.book);
const mode=computed(()=>saveMode(book.value?.source||'',props.sources));
const source=computed(()=>props.sources.find(s=>s.id===book.value?.source));
const ownJobs=computed(()=>props.jobs.filter(j=>j.book_id===props.bookId));
const status=computed(()=>book.value?workbenchStatus(book.value,ownJobs.value,mode.value):null);
const currentJob=computed(()=>status.value?.job||ownJobs.value[0]);
const active=computed(()=>ownJobs.value.some(j=>['running','queued'].includes(j.state)));
const pending=computed(()=>ownJobs.value.some(j=>['running','queued','paused'].includes(j.state)));
const recoverable=computed(()=>!!currentJob.value&&['paused','cancelled','failed','partial'].includes(currentJob.value.state));
const chapters=computed(()=>detail.value?.chapters.filter(c=>(!onlySaved.value||c.saved)&&`${c.position} ${c.title}`.includes(search.value))||[]);
const readerPages=computed(()=>Math.max(1,Math.ceil(chapters.value.length/100)));
const visibleChapters=computed(()=>chapters.value.slice((readerPage.value-1)*100,readerPage.value*100));
const savedChapters=computed(()=>detail.value?.chapters.filter(c=>c.saved)||[]);
const readIndex=computed(()=>savedChapters.value.findIndex(c=>c.id===reader.value?.id));
const items=computed(()=>jobView.value?.items.filter(i=>itemFilter.value==='all'||(itemFilter.value==='missing'?i.state!=='completed':i.state===itemFilter.value))||[]);
const itemPages=computed(()=>Math.max(1,Math.ceil(items.value.length/100)));
const visibleItems=computed(()=>items.value.slice((itemPage.value-1)*100,itemPage.value*100));
const labels:Record<string,string>={queued:'等待执行',running:'正在保存',paused:'已暂停',cancelled:'已取消',failed:'失败',partial:'部分保存',blocked:'访问受限',completed:'本次范围完成'};
const itemLabels:Record<string,string>={queued:'待保存',completed:'已保存',failed:'失败',blocked:'受限'};
const tabs:{id:WorkbenchTab;label:string}[]=[{id:'save',label:'保存进度'},{id:'read',label:'阅读正文'},{id:'export',label:'其他格式与文件记录'},{id:'overview',label:'书籍详情'}];

async function reload(){
  const id=props.bookId;const sequence=++loadSequence;
  const result=await bookDetails(id);
  if(disposed||sequence!==loadSequence||id!==props.bookId)return;
  detail.value=result;
  if(reader.value&&!result.chapters.some(c=>c.id===reader.value?.id&&c.saved))reader.value=null;
  if(!tagsDirty.value)tags.value=(preference.value?.tags||[]).join('，');
}
async function refresh(){try{await reload();}catch(e){if(!disposed)error.value=String(e);}}
async function action(work:()=>Promise<void>,reloadAfter=false){
  if(busy.value)return;busy.value=true;error.value='';notice.value='';
  try{await work();if(reloadAfter)await reload();}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}
}
async function selectTab(value:WorkbenchTab){tab.value=value;await nextTick();panel.value?.focus();}
function tabKeys(event:KeyboardEvent){
  const index=tabs.findIndex(t=>t.id===tab.value);let target:number|undefined;
  if(event.key==='ArrowRight')target=(index+1)%tabs.length;
  if(event.key==='ArrowLeft')target=(index+tabs.length-1)%tabs.length;
  if(event.key==='Home')target=0;if(event.key==='End')target=tabs.length-1;
  if(target!==undefined){event.preventDefault();tab.value=tabs[target]!.id;void nextTick(()=>document.getElementById(`book-tab-${tab.value}`)?.focus());}
}
async function openChapter(id:string){
  const owner=props.bookId;const sequence=++readSequence;
  await action(async()=>{const result=await readChapter(owner,id);if(!disposed&&owner===props.bookId&&sequence===readSequence)reader.value=result;});
}
async function loadJob(id:string){const owner=props.bookId;const sequence=++jobSequence;const result=await jobDetails(id);if(!disposed&&owner===props.bookId&&openedJob.value===id&&sequence===jobSequence)jobView.value=result;}
async function showJob(id:string){openedJob.value=id;jobView.value=null;itemPage.value=1;await action(()=>loadJob(id));}
async function relatedJob(id:string){await selectTab('save');await showJob(id);}
function control(job:Job,operation:'pause'|'resume'|'cancel'){
  void action(async()=>{
    if(operation==='resume')await resumeJob(job.id);else if(operation==='pause')await pauseJob(job.id);else await cancelJob(job.id);
    notice.value='请求已提交，任务状态以刷新后的实际记录为准。';emit('changed');if(openedJob.value===job.id)jobView.value=await jobDetails(job.id);
  },true);
}
function fillMissing(){void action(async()=>{await updateBook(props.bookId);notice.value='已重新读取官网目录并创建补齐任务，已有正文会复用。';emit('changed');},true);}
function retry(id:string){const job=jobView.value?.job;if(!job)return;void action(async()=>{await retryChapters(job.id,[id]);notice.value='已请求重试所选失败章节，受限章节不会强行重试。';emit('changed');jobView.value=await jobDetails(job.id);},true);}
function exported(){void refresh();emit('changed');}
function revised(){const owner=props.bookId;const id=reader.value?.id;void action(async()=>{await reload();if(id&&detail.value?.chapters.some(c=>c.id===id&&c.saved)){const sequence=++readSequence;const content=await readChapter(owner,id);if(!disposed&&owner===props.bookId&&sequence===readSequence)reader.value=content;}emit('changed');});}
function accountSaved(id:string){if(id===props.bookId){void refresh();emit('changed');}}
function readSaved(){void selectTab('read');if(!reader.value&&savedChapters.value[0])void openChapter(savedChapters.value[0].id);}
function organize(archived:boolean){void action(async()=>{preference.value=await organizeBook(props.bookId,tags.value.split(/[,，]/).map(t=>t.trim()).filter(Boolean),archived);tagsDirty.value=false;notice.value=archived?'作品已归档，正文和导出保留。':'书库整理设置已保存。';emit('changed');},true);}
watch(()=>props.initialTab,value=>{if(value)void selectTab(value);});
watch(()=>props.automatic?.receipt?.path,()=>{void refresh();});
watch(()=>props.preference,value=>{preference.value=value;if(!tagsDirty.value)tags.value=(value?.tags||[]).join('，');});
watch(()=>JSON.stringify([props.summary?.saved_chapters,props.summary?.total_chapters,ownJobs.value.map(j=>[j.id,j.state,j.saved,j.failed,j.blocked])]),()=>{void refresh();if(tab.value==='save'&&openedJob.value&&!busy.value)void loadJob(openedJob.value).catch(e=>{if(!disposed)error.value=String(e);});});
watch([search,onlySaved],()=>{readerPage.value=1;});
watch(readerPages,n=>{readerPage.value=Math.min(readerPage.value,n);});
watch(itemFilter,()=>{itemPage.value=1;});watch(itemPages,n=>{itemPage.value=Math.min(itemPage.value,n);});
onMounted(async()=>{await refresh();await nextTick();heading.value?.focus();});
onUnmounted(()=>{disposed=true;loadSequence++;readSequence++;jobSequence++;});
</script>

<template>
  <section class="book-workspace" aria-label="作品工作台">
    <div class="workspace-top"><button @click="emit('back')">返回书库</button><div class="actions"><button :disabled="busy" @click="refresh();emit('changed')">刷新本书状态</button><button @click="emit('help')">操作指引</button></div></div>
    <p v-if="error" class="message failure" role="alert">{{ error }}<button aria-label="关闭工作台错误" @click="error=''">×</button></p>
    <p v-if="notice" class="message success" role="status">{{ notice }}<button aria-label="关闭工作台提示" @click="notice=''">×</button></p>
    <p v-if="!detail&&!error" role="status">正在读取作品和保存进度…</p>
    <template v-if="book&&detail&&status">
      <header class="workspace-heading"><h1 ref="heading" tabindex="-1">{{ book.title }}</h1><p>{{ book.author||'未标注作者' }} · {{ mode==='local'?'本地 JSON':source?.name||book.source }}</p></header>
      <section class="book-status" :data-state="status.kind" aria-label="本书保存状态"><div><strong>{{ status.title }}</strong><p>{{ status.kind==='attention'?saveRecovery(currentJob?.error||''):status.message }}</p><p>全书正文已保存 <b>{{ book.saved_chapters }}/{{ book.total_chapters }}</b> 章 · 尚缺 {{ Math.max(0,book.total_chapters-book.saved_chapters) }} 章</p></div><button v-if="tab!==status.tab&&status.kind!=='complete'" class="primary" :disabled="busy" @click="selectTab(status.tab)">{{ status.action }}</button></section>
      <AutomaticTxtPanel :book-id="book.id" :saved="book.saved_chapters" :active="active" :status="automatic" />
      <div class="workspace-tabs" role="tablist" aria-label="作品功能" @keydown="tabKeys"><button v-for="entry in tabs" :id="`book-tab-${entry.id}`" :key="entry.id" role="tab" :aria-selected="tab===entry.id" :aria-controls="`book-panel-${entry.id}`" :tabindex="tab===entry.id?0:-1" @click="selectTab(entry.id)">{{ entry.label }}</button></div>
      <section :id="`book-panel-${tab}`" ref="panel" role="tabpanel" :aria-labelledby="`book-tab-${tab}`" tabindex="-1" class="workspace-panel">
        <template v-if="tab==='overview'">
          <h2>接着处理这本书</h2><p>{{ book.saved_chapters?'已保存正文可立即离线阅读，后续仍可补齐缺章。':'目前只有作品信息或目录，保存正文后才能阅读和导出。' }}</p>
          <div class="actions"><button :disabled="!book.saved_chapters||busy" @click="readSaved">阅读已保存正文</button><button :disabled="!book.saved_chapters||busy" @click="selectTab('export')">导出已保存内容</button></div>
          <p v-if="currentJob" class="recent-task">最近相关任务：{{ labels[currentJob.state]||currentJob.state }} · 本次范围 {{ currentJob.saved }}/{{ currentJob.total }} 章。任务范围完成不表示全书完整。</p>
          <AvailabilityPanel :chapters="detail.chapters" :busy="busy" @save="selectTab('save')" @read="readSaved" @job="relatedJob" />
          <details v-if="detail.description"><summary>作品简介</summary><p class="workspace-description">{{ detail.description }}</p></details>
          <details class="workspace-advanced"><summary>整理作品、目录变更与历史版本</summary><div class="tag-editor"><label>标签（逗号分隔）<input v-model="tags" :disabled="busy" placeholder="例如 收藏，连载" @input="tagsDirty=true" /></label><button :disabled="busy" @click="organize(!!preference?.archived)">保存标签</button><button :disabled="busy" @click="organize(!preference?.archived)">{{ preference?.archived?'移回书架':'归档作品' }}</button></div><RevisionPanel :book-id="book.id" :chapter-id="reader?.id||null" :online="mode==='public'" @changed="revised" /></details>
        </template>
        <template v-else-if="tab==='save'">
          <AccountPanel v-if="mode==='account'" :book-id="book.id" :account-id="selectedAccount" embedded @selected="selectedAccount=$event" @saved="accountSaved" @ready="accountSaved" @read="readSaved" />
          <template v-else>
            <h2>{{ mode==='local'?'本地导入进度':'保存本书正文' }}</h2>
            <p v-if="mode==='public'">{{ active?'保存正在进行，完成后会自动生成 TXT。':book.saved_chapters===book.total_chapters?'正文已保存，不需要再次下载。上方可以打开 TXT 文件夹。':'点击下方按钮继续保存，已有正文会自动跳过。' }}</p>
            <p v-else-if="mode==='unavailable'">{{ source?.note||'这个来源尚未开放正文保存。已有文字仍可离线阅读和导出。' }}</p>
            <p v-else>原始 JSON 中的章节通过持久任务导入。本地作品不能从官网补齐。</p>
            <button v-if="mode==='public'&&!pending&&!recoverable&&book.saved_chapters<book.total_chapters" class="primary" :disabled="busy||pending" @click="fillMissing">检查目录并保存缺章</button>
            <article v-if="currentJob&&(active||book.saved_chapters<book.total_chapters)" class="workspace-task"><h3>{{ labels[currentJob.state]||currentJob.state }}</h3><p>本次任务范围已保存 {{ currentJob.saved }}/{{ currentJob.total }} 章，受限 {{ currentJob.blocked }}，失败 {{ currentJob.failed }}。</p><progress :value="currentJob.saved" :max="currentJob.total" aria-label="当前任务范围进度"></progress><p v-if="currentJob.error" class="recovery">下一步：{{ saveRecovery(currentJob.error) }}</p><div class="actions"><button v-if="['queued','running'].includes(currentJob.state)" :disabled="busy" @click="control(currentJob,'pause')">暂停本书任务</button><button v-if="recoverable&&mode!=='unavailable'" class="primary" :disabled="busy||active" @click="control(currentJob,'resume')">继续保存</button></div></article>
          </template>
          <details class="workspace-advanced"><summary>为什么有些章节没保存？</summary><AvailabilityPanel :chapters="detail.chapters" :busy="busy" @save="selectTab('save')" @read="readSaved" @job="relatedJob" /></details>
          <details class="workspace-advanced" :open="!!openedJob">
<summary>更多管理：检查更新、取消任务、详细记录</summary><button v-if="mode==='public'" :disabled="busy||pending" @click="fillMissing">检查更新并保存缺章</button><button v-if="currentJob&&mode!=='account'&&['queued','running','paused'].includes(currentJob.state)" :disabled="busy" @click="control(currentJob,'cancel')">取消本书任务（正文保留）</button><p>批次任务需到全局任务面板统一控制。受限内容需要重新核对官网，不能强行重试。</p><ol class="workspace-jobs"><li v-for="job in ownJobs.slice(0,20)" :key="job.id"><span>{{ labels[job.state]||job.state }} · {{ job.saved }}/{{ job.total }} 章</span><button :disabled="busy" @click="showJob(job.id)">查看这次任务明细</button></li></ol><p v-if="!ownJobs.length">本书还没有保存任务。</p>
            <template v-if="jobView"><label>章节状态<select v-model="itemFilter"><option value="missing">未保存</option><option value="all">全部</option><option value="failed">失败</option><option value="blocked">受限</option></select></label><ol class="job-reasons"><li v-for="item in visibleItems" :key="item.chapter_id"><strong>{{ item.position }} · {{ item.title }}</strong><span>{{ itemLabels[item.state]||item.state }}{{ item.error?'：'+item.error:'' }}</span><button v-if="item.retryable&&mode==='public'" :disabled="busy||active" @click="retry(item.chapter_id)">重试此失败章</button></li></ol><p v-if="!items.length">当前筛选下没有章节。</p><div v-if="itemPages>1" class="detail-pagination"><button :disabled="itemPage<=1" @click="itemPage--">上一页</button><span>{{ itemPage }}/{{ itemPages }}</span><button :disabled="itemPage>=itemPages" @click="itemPage++">下一页</button></div></template>
          </details><button class="global-task-link" @click="emit('tasks')">查看全局任务与批次</button>
        </template>
        <template v-else-if="tab==='read'">
          <h2>离线阅读</h2><p v-if="!book.saved_chapters">还没有正文。请先在“保存进度”中保存章节。</p><div v-if="!book.saved_chapters" class="actions"><button @click="selectTab('save')">去保存正文</button></div>
          <div v-else class="reader-layout"><div class="reader-directory"><label>查找章节<input v-model="search" type="search" placeholder="序号或章节标题" /></label><label class="saved-filter"><input v-model="onlySaved" type="checkbox" />只看已保存章节</label><ol><li v-for="chapter in visibleChapters" :key="chapter.id"><button :disabled="busy||!chapter.saved" :aria-current="reader?.id===chapter.id?'true':undefined" @click="openChapter(chapter.id)">{{ chapter.position }} · {{ chapter.title }}{{ chapter.saved?'':'（未保存）' }}</button></li></ol><p v-if="!chapters.length">没有符合条件的章节。</p><div class="detail-pagination"><button :disabled="readerPage<=1" @click="readerPage--">上一页</button><span>{{ readerPage }}/{{ readerPages }}</span><button :disabled="readerPage>=readerPages" @click="readerPage++">下一页</button></div></div><article class="reader-text" aria-label="已保存正文"><template v-if="reader"><h3>{{ reader.title }}</h3><p v-for="(paragraph,index) in reader.paragraphs" :key="index">{{ paragraph }}</p><div class="detail-pagination"><button :disabled="busy||readIndex<=0" @click="openChapter(savedChapters[readIndex-1]!.id)">上一章</button><button :disabled="busy||readIndex<0||readIndex>=savedChapters.length-1" @click="openChapter(savedChapters[readIndex+1]!.id)">下一章</button></div></template><p v-else>从左侧选择已保存章节。切换工作台页签会保留当前阅读章节。</p></article></div>
        </template>
        <template v-else>
          <ExportActions :book-id="book.id" :saved="book.saved_chapters" :total="book.total_chapters" @exported="exported" />
          <h3>本书导出记录</h3><p>记录标明导出时的范围与时间，同章数不保证内容版本相同。旧文件不会覆盖。</p><p v-if="!detail.exports.length">还没有导出记录，先在上方导出已保存内容。</p><ul class="export-history"><li v-for="record in detail.exports" :key="record.id"><span>{{ record.job_id?'任务选章':'书库内容' }} · {{ record.format.toUpperCase() }} · {{ record.saved }}/{{ record.total }} 章 · {{ record.created_at }} UTC</span><code>{{ record.path }}</code><button :disabled="busy" @click="action(()=>revealExport(record.id))">打开此导出文件夹</button></li></ul>
        </template>
      </section>
    </template>
  </section>
</template>

<style scoped>
.book-workspace{color:#273329}.workspace-top,.workspace-top .actions,.actions{display:flex;flex-wrap:wrap;gap:12px;align-items:center}.workspace-top{justify-content:space-between;margin-bottom:24px}.workspace-heading h1{font-size:28px;letter-spacing:0;overflow-wrap:anywhere}.workspace-heading p{font-size:13px;color:#526048}.book-status{display:flex;justify-content:space-between;gap:20px;align-items:center;background:#edf1e5;padding:20px;border-radius:8px;margin:24px 0}.book-status strong{font-size:17px}.book-status p{margin:8px 0 0;line-height:1.8;font-size:13px;max-width:65ch;overflow-wrap:anywhere}.book-status[data-state=attention]{background:#fff0e9}.workspace-tabs{display:flex;gap:4px;border-bottom:1px solid #cbd4c0;overflow:auto}.workspace-tabs button{flex-shrink:0;background:transparent;border:0;border-radius:5px 5px 0 0;padding:13px 22px;font-size:14px}.workspace-tabs [aria-selected=true]{background:#354d32;color:#fff}.workspace-panel{padding:26px;background:#fffefa;border:1px solid #dfe4d7;border-top:0;border-radius:0 0 8px 8px;min-width:0}.workspace-panel:focus{outline:none}.workspace-panel h2{font-size:19px}.workspace-panel p{font-size:13px;line-height:1.9;color:#526048}.workspace-panel label{display:grid;gap:8px;font-size:13px}.workspace-panel input:not([type=checkbox]),.workspace-panel select{font:inherit;padding:10px;min-width:0;border:1px solid #bdc9b1;background:#fafbf7;color:#334431;border-radius:5px}.workspace-panel input::placeholder{color:#617056}.workspace-advanced{margin-top:26px;padding-top:18px;border-top:1px solid #dfe4d7}.workspace-panel summary{cursor:pointer;font-size:13px;font-weight:600;line-height:1.8}.workspace-panel .actions{margin:16px 0}.workspace-description{white-space:pre-wrap;max-width:72ch;overflow-wrap:anywhere}.recent-task{margin:24px 0}.tag-editor{display:flex;align-items:end;gap:12px;flex-wrap:wrap;margin:18px 0}.workspace-task{margin-top:22px;padding-top:18px;border-top:1px solid #dfe4d7}.workspace-task h3{font-size:16px}.workspace-jobs,.job-reasons{list-style:none;padding:0}.workspace-jobs li{display:flex;gap:14px;justify-content:space-between;align-items:center;flex-wrap:wrap;border-bottom:1px solid #e1e5da;padding:12px 0;font-size:13px}.job-reasons li{display:grid;gap:8px;padding:14px 0;border-bottom:1px solid #e1e5da;font-size:13px;line-height:1.8;overflow-wrap:anywhere}.job-reasons button{justify-self:start}.global-task-link{margin-top:18px}.workspace-panel .saved-filter{display:flex;align-items:center;gap:7px;margin:14px 0}.workspace-panel .reader-directory input[type=checkbox]{width:auto}.workspace-panel .reader-directory input[type=search]{max-width:100%}.workspace-panel .reader-text{overflow-wrap:anywhere}.workspace-panel .export-history code{overflow-wrap:anywhere}.workspace-panel :deep(.account-panel){padding:0;border:0;background:transparent}.workspace-panel :deep(.export-actions){margin-top:0;border-top:0;padding-top:0}
@media(max-width:800px){.book-status{align-items:start;flex-direction:column}.workspace-tabs button{padding:12px 17px}.workspace-panel{padding:18px}.reader-layout{grid-template-columns:minmax(0,1fr)}.workspace-heading h1{font-size:24px}}
</style>
