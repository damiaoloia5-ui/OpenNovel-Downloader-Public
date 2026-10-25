<script setup lang="ts">
import {onMounted,onUnmounted,ref,watch,computed} from 'vue';
import {snapshot,accountDownloadJobs,startAccountDownload,resumeAccountDownload,pauseJob,cancelJob,exportBook,type Book,type Job} from './api';
const props=defineProps<{accountId:string;opened:boolean;preferredBook:string;revision:number}>();
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string]}>();
const books=ref<Book[]>([]);const bookId=ref('');const selection=ref('');const acknowledged=ref(false);const jobs=ref<Job[]>([]);const busy=ref(false);const error=ref('');const notice=ref('');const polling=ref(false);
const active=computed(()=>jobs.value.some(j=>['queued','running'].includes(j.state)));
const pending=computed(()=>jobs.value.some(j=>['queued','running','paused'].includes(j.state)));
const labels:Record<string,string>={queued:'等待开始',running:'自动保存中',paused:'已暂停',cancelled:'已取消',failed:'已停止，需要处理',partial:'部分保存，已停止',blocked:'阅读受限，已停止',completed:'本次范围完成'};
let timer:ReturnType<typeof setTimeout>|undefined;let disposed=false;
async function loadBooks(){const result=await snapshot();books.value=result.books.filter(b=>b.source==='qidian-session');if(books.value.some(b=>b.id===props.preferredBook))bookId.value=props.preferredBook;else if(!books.value.some(b=>b.id===bookId.value))bookId.value=books.value[0]?.id||'';}
async function reload(){const chosen=bookId.value;if(!chosen){jobs.value=[];return;}const current=await accountDownloadJobs(chosen);if(bookId.value!==chosen)return;const before=jobs.value[0];jobs.value=current;if(current[0]&&(before?.saved!==current[0].saved||before.state!==current[0].state))emit('saved',chosen);}
async function poll(){if(disposed)return;if(!document.hidden&&!polling.value){polling.value=true;try{await reload();}catch(e){error.value=String(e);}finally{polling.value=false;}}if(!disposed)timer=setTimeout(()=>void poll(),active.value?2000:5000);}
async function action(work:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{await work();await reload();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function start(){void action(async()=>{await startAccountDownload(props.accountId,bookId.value,selection.value,acknowledged.value);notice.value='自动任务已启动。官网窗口会逐章切换，请勿操作该窗口；可在下方暂停。';});}
function resume(job:Job){void action(async()=>{await resumeAccountDownload(props.accountId,job.id,acknowledged.value);notice.value='已继续原任务，只保存缺失章节；遇到限制会再次停止。';});}
function exportSaved(format:'txt'|'epub'){void action(async()=>{const r=await exportBook(bookId.value,format);notice.value=`已导出 ${r.saved_chapters}/${r.total_chapters} 章${r.partial?'（含缺章提示）':''}：${r.path}`;});}
watch(bookId,()=>{jobs.value=[];error.value='';notice.value='';void action(reload);});
watch(()=>[props.preferredBook,props.revision],()=>{void action(async()=>{await loadBooks();});});
onMounted(()=>{void action(async()=>{await loadBooks();});void poll();});
onUnmounted(()=>{disposed=true;if(timer)clearTimeout(timer);});
</script>
<template>
  <section class="automatic-panel" aria-label="起点自动保存正文">
    <h4>自动保存全书正文 · 实验性</h4>
    <p class="footnote">选择已确认完整目录的作品，自动逐章保存缺失正文，已有内容跳过。遇到登录、订阅、验证或页面异常即停止，保留进度；关闭软件后也可重新登录并继续。</p>
    <p v-if="!books.length" class="footnote">请先在下方“保存当前可读章节”中读取并确认完整目录，随后作品会出现在这里。</p>
    <label>要自动保存的作品<select v-model="bookId" :disabled="busy||active||!books.length"><option v-if="!books.length" value="">尚无已确认目录</option><option v-for="b in books" :key="b.id" :value="b.id">{{ b.title }}</option></select></label>
    <details><summary>限定章节范围（可选）</summary><label>留空保存全书缺章<input v-model="selection" :disabled="busy||pending" placeholder="例如 1-3 或 1,3,5-8；序号按目录"></label></details>
    <label class="ack"><input v-model="acknowledged" type="checkbox" :disabled="busy||active">我已在官网关闭自动订阅，确认只保存已有阅读权限的内容</label>
    <p class="footnote">本软件不会点击订阅或购买按钮。官网自身的自动订阅设置需由你本人确认；自动任务期间保持官网窗口打开。</p>
    <div class="actions"><button class="primary" :disabled="busy||pending||!opened||!bookId||!acknowledged" @click="start">{{ selection.trim()?'自动保存所选章节':'自动保存全书' }}</button><button :disabled="busy" @click="action(loadBooks)">刷新作品列表</button><button v-if="bookId" :disabled="busy" @click="emit('read',bookId)">查看本书已保存正文</button></div>
    <p v-if="!opened" class="footnote">请先点击本账户的“登录起点”或“打开官网窗口”。</p>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <ol class="auto-jobs"><li v-for="job in jobs.slice(0,5)" :key="job.id"><strong>{{ labels[job.state]||job.state }} · 本次已保存 {{ job.saved }}/{{ job.total }} 章</strong><progress :value="job.saved" :max="job.total" :aria-label="`${job.title} 自动保存进度`"></progress><p v-if="job.error" class="message failure">{{ job.error }}</p><p v-if="job.blocked||job.failed" class="footnote">受限 {{ job.blocked }} 章，失败 {{ job.failed }} 章；后续尚未保存。请先在官网处理原因，再继续原任务。</p><div class="actions"><button v-if="['running','queued'].includes(job.state)" :disabled="busy" @click="action(()=>pauseJob(job.id))">暂停自动保存</button><button v-if="!['running','queued','completed'].includes(job.state)" :disabled="busy||active||!opened||!acknowledged" @click="resume(job)">继续此任务</button><button v-if="['running','queued','paused'].includes(job.state)" :disabled="busy" @click="action(()=>cancelJob(job.id))">取消此任务</button></div></li></ol>
    <div v-if="bookId" class="actions"><button :disabled="busy" @click="exportSaved('txt')">导出本书 TXT</button><button :disabled="busy" @click="exportSaved('epub')">导出本书 EPUB</button></div>
  </section>
</template>
<style scoped>
.automatic-panel{border-top:1px solid #d4ddca;margin-top:18px;padding-top:18px}h4{font-size:18px;margin:0 0 12px}.footnote{line-height:1.8}.automatic-panel label{display:grid;gap:8px;font-size:13px;margin:12px 0}.automatic-panel select,.automatic-panel input:not([type=checkbox]){width:100%;padding:10px;border:1px solid #d4ddca;background:#fafbf7;color:#334431;border-radius:5px;font:inherit}.automatic-panel .ack{display:flex;align-items:start;gap:8px;line-height:1.7}.ack input{margin-top:4px}.actions{display:flex;flex-wrap:wrap;gap:10px;margin:14px 0}.auto-jobs{padding:0;list-style:none}.auto-jobs li{padding:12px 0;border-top:1px solid #e1e5da}progress{display:block;width:100%;height:10px;margin-top:12px}summary{cursor:pointer}
</style>
