<script setup lang="ts">
import {onMounted,onUnmounted,ref,watch,computed} from 'vue';
import ExportActions from './ExportActions.vue';
import {snapshot,accountDownloadJobs,startAccountDownload,resumeAccountDownload,pauseJob,cancelJob,type Book,type Job} from './api';
import {saveRecovery} from './workflow';
const props=defineProps<{accountId:string;opened:boolean;preferredBook:string;revision:number;guided?:boolean;hideExport?:boolean}>();
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string]}>();
const books=ref<Book[]>([]);const bookId=ref('');const selection=ref('');const acknowledged=ref(false);const jobs=ref<Job[]>([]);const busy=ref(false);const error=ref('');const notice=ref('');const polling=ref(false);
const active=computed(()=>jobs.value.some(j=>['queued','running'].includes(j.state)));
const pending=computed(()=>jobs.value.some(j=>['queued','running','paused'].includes(j.state)));
const book=computed(()=>books.value.find(b=>b.id===bookId.value));
const resumable=computed(()=>{const job=jobs.value[0];return job&&['paused','cancelled','failed','partial','blocked'].includes(job.state)&&job.saved<job.total?job:null;});
const complete=computed(()=>!!book.value&&book.value.total_chapters>0&&book.value.saved_chapters===book.value.total_chapters);
const newTask=ref(false);const intervalMs=ref(500);
const labels:Record<string,string>={queued:'等待开始',running:'自动保存中',paused:'已暂停',cancelled:'已取消',failed:'已停止，需要处理',partial:'部分保存，已停止',blocked:'阅读受限，已停止',completed:'本次范围完成'};
let timer:ReturnType<typeof setTimeout>|undefined;let disposed=false;
async function loadBooks(){const result=await snapshot();if(disposed)return;books.value=result.books.filter(b=>b.source==='qidian-session'&&(!props.guided||b.id===props.preferredBook));if(books.value.some(b=>b.id===props.preferredBook))bookId.value=props.preferredBook;else if(!books.value.some(b=>b.id===bookId.value))bookId.value=books.value[0]?.id||'';}
async function reload(){const chosen=bookId.value;if(!chosen){jobs.value=[];return;}const current=await accountDownloadJobs(chosen);if(disposed||bookId.value!==chosen)return;const before=jobs.value[0];jobs.value=current;if(current[0]&&(before?.saved!==current[0].saved||before.state!==current[0].state)){await loadBooks();if(!disposed)emit('saved',chosen);}}
async function poll(){if(disposed)return;if(!document.hidden&&!polling.value){polling.value=true;try{await reload();}catch(e){error.value=String(e);}finally{polling.value=false;}}if(!disposed)timer=setTimeout(()=>void poll(),active.value?2000:5000);}
async function action(work:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{await work();await reload();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function start(){void action(async()=>{await startAccountDownload(props.accountId,bookId.value,selection.value,acknowledged.value,intervalMs.value);notice.value='自动任务已启动。官网窗口会逐章切换，请勿操作该窗口；可在下方暂停。';});}
function resume(job:Job){void action(async()=>{await resumeAccountDownload(props.accountId,job.id,acknowledged.value,intervalMs.value);notice.value='已继续原任务，只保存缺失章节；遇到限制会再次停止。';});}
watch(bookId,()=>{jobs.value=[];error.value='';notice.value='';void action(reload);});
watch(()=>jobs.value[0]?.id,()=>{newTask.value=false;});
watch(()=>[props.preferredBook,props.revision],()=>{void action(async()=>{await loadBooks();});});
onMounted(()=>{void action(async()=>{await loadBooks();});void poll();});
onUnmounted(()=>{disposed=true;if(timer)clearTimeout(timer);});
</script>
<template>
  <section class="automatic-panel" aria-label="起点自动保存正文">
    <h3>{{ complete?'本书正文已保存完成':active?'正在自动保存，请保持官网窗口打开':resumable?'进度已保留，可以接着保存':'开始保存这本书' }}</h3>
    <p v-if="complete" role="status">不需要再下载，TXT 会自动生成。生成后点上方的“打开 TXT 所在文件夹”。</p>
    <template v-else>
      <p class="footnote">{{ active?'保存时不要手动切换官网页面。想中途退出，先点“暂停保存”。':'默认保存全书缺章，已经保存的章节会自动跳过；保存结束后自动生成 TXT。' }}</p>
      <p v-if="!books.length" class="footnote">还没有这本书的目录，请先按上方提示核对书名。</p>
      <label v-if="!guided">要保存的小说<select v-model="bookId" :disabled="busy||active||!books.length"><option v-if="!books.length" value="">请先核对这本书</option><option v-for="b in books" :key="b.id" :value="b.id">{{ b.title }}</option></select></label>
      <template v-if="!active"><label class="ack"><input v-model="acknowledged" type="checkbox" :disabled="busy">我已在官网关闭自动订阅，确认只保存已有阅读权限的内容</label><p v-if="!acknowledged" class="footnote">先勾选上面的确认项，再点下面的保存按钮。</p></template>
      <div class="actions"><button v-if="!pending&&(!resumable||newTask)" class="primary" :disabled="busy||!opened||!bookId||!acknowledged" @click="start">{{ selection.trim()?'保存所选章节为 TXT':'开始保存为 TXT' }}</button><button v-if="resumable&&!newTask" class="primary" :disabled="busy||active||!opened||!acknowledged" @click="resume(resumable)">继续保存</button><button v-if="book?.saved_chapters&&!hideExport" :disabled="busy" @click="emit('read',bookId)">阅读已保存正文</button></div>
      <p v-if="!opened" class="footnote">先打开起点官网并登录，再回来保存。</p>
      <ol class="auto-jobs"><li v-for="job in jobs.slice(0,1)" :key="job.id"><strong>{{ labels[job.state]||job.state }} · 本次已保存 {{ job.saved }}/{{ job.total }} 章</strong><progress :value="job.saved" :max="job.total" :aria-label="`${job.title} 自动保存进度`"></progress><p v-if="job.error" class="recovery" role="status">下一步：{{ saveRecovery(job.error) }}</p><details v-if="job.error||job.blocked||job.failed"><summary>查看停止原因</summary><p>{{ job.error }}</p><p>受限 {{ job.blocked }} 章，失败 {{ job.failed }} 章；未保存正文不会填造。</p></details><div class="actions"><button v-if="['running','queued'].includes(job.state)" :disabled="busy" @click="action(()=>pauseJob(job.id))">暂停保存</button></div></li></ol>
    </template>
    <p v-if="error" class="message failure" role="alert">{{ error }}<br>下一步：{{ saveRecovery(error) }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <details class="save-settings">
<summary>更多设置：速度、选章、取消任务</summary>
      <label>保存节奏<select v-model.number="intervalMs" :disabled="busy||active"><option :value="500">极速 · 最小翻页间隔 0.5 秒（实验）</option><option :value="1000">快速 · 最小翻页间隔 1 秒（实验）</option><option :value="3000">标准 · 最小翻页间隔 3 秒</option><option :value="6000">缓慢 · 最小翻页间隔 6 秒</option></select></label><p class="footnote">默认极速。实际速度还取决于网页加载与完整性确认。遇到限流或验证仍会停止；可暂停后改用标准或缓慢节奏继续。</p>
      <label v-if="resumable" class="ack"><input v-model="newTask" type="checkbox" :disabled="busy||pending">另建任务并保留旧记录（高级，不会改变原任务范围）</label><label>只保存部分章节（留空就是全书缺章）<input v-model="selection" :disabled="busy||pending||!!resumable&&!newTask" placeholder="例如 1-3 或 1,3,5-8" /></label>
      <div v-for="job in jobs.filter(j=>['running','queued','paused'].includes(j.state))" :key="job.id" class="actions"><button :disabled="busy" @click="action(()=>cancelJob(job.id))">取消此任务（已保存正文保留）</button></div>
    </details>
    <ExportActions v-if="book&&!hideExport" :key="book.id" :book-id="book.id" :saved="book.saved_chapters" :total="book.total_chapters" :title="guided?'4. 导出文件':undefined" />
  </section>
</template>
<style scoped>
.automatic-panel{border-top:1px solid #d4ddca;margin-top:18px;padding-top:18px}h3{font-size:18px;margin:0 0 12px}.footnote{line-height:1.8;color:#526048;font-size:13px}.automatic-panel label{display:grid;gap:8px;font-size:13px;margin:12px 0}.automatic-panel select,.automatic-panel input:not([type=checkbox]){width:100%;padding:10px;border:1px solid #bdc9b1;background:#fafbf7;color:#334431;border-radius:5px;font:inherit}.automatic-panel .ack{display:flex;align-items:start;gap:8px;line-height:1.7}.ack input{margin-top:4px}.actions{display:flex;flex-wrap:wrap;gap:10px;margin:14px 0}.auto-jobs{padding:0;list-style:none}.auto-jobs li{padding:12px 0;border-top:1px solid #e1e5da}progress{display:block;width:100%;height:10px;margin-top:12px}summary{cursor:pointer;font-size:13px}.export-step{margin-top:28px}
</style>
