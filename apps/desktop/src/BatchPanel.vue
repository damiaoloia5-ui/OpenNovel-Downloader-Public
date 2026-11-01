<script setup lang="ts">
import {ref,onMounted,onUnmounted} from 'vue';
import {batches,createBatch,resumeBatch,stopBatch,desktopAvailable,type Batch} from './api';
const text=ref('');const selection=ref('1-3');const items=ref<Batch[]>([]);const working=ref(false);const error=ref('');const notice=ref('');
const labels:Record<string,string>={queued:'等待执行',running:'正在执行',paused:'已暂停',cancelled:'已取消',completed:'所选章节完成',partial:'部分完成',failed:'失败',blocked:'受限'};
let timer:ReturnType<typeof setInterval>|undefined;let refreshing=false;
async function refresh(){if(!desktopAvailable||refreshing)return;refreshing=true;try{items.value=await batches();}catch(e){error.value=String(e);}finally{refreshing=false;}}
async function action(work:()=>Promise<void>){if(working.value)return;working.value=true;error.value='';notice.value='';try{await work();}catch(e){error.value=String(e);}finally{working.value=false;await refresh();}}
function start(){void action(async()=>{const input=text.value.split(/\r?\n/).map(s=>s.trim()).filter(Boolean);await createBatch(input,selection.value);notice.value='批次已持久保存；同来源相同作品去重，每次顺序执行一本。';});}
function resume(id:string){void action(async()=>{await resumeBatch(id);});}
function stop(id:string,paused:boolean){void action(async()=>{await stopBatch(id,paused);notice.value=paused?'批次已暂停，已保存内容保留。':'批次已取消，已保存内容保留。';});}
onMounted(()=>{void refresh();timer=setInterval(()=>void refresh(),1000);});onUnmounted(()=>{if(timer)clearInterval(timer);});
</script>
<template>
  <section class="download-panel" aria-label="批量添加">
    <h2>小批量下载</h2><p class="footnote">每批最多 10 部，最多 5 个待处理批次，顺序执行。每行一个已支持来源的作品链接或 ID；先选小范围核验，受限项仍明确计入缺章。</p>
    <label for="batch-input">作品列表</label><textarea id="batch-input" v-model="text" rows="4" placeholder="qimao:作品ID&#10;jinjiang:作品ID" :disabled="working"></textarea>
    <div class="range-row"><label>每本选章<input v-model="selection" placeholder="例如 1-3 或 1,3,5" :disabled="working"></label><button :disabled="!desktopAvailable||working||!text.trim()||!selection.trim()" @click="start">创建并执行批次</button></div>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
  </section>
  <section v-if="items.length" class="download-panel" aria-label="持久批次">
    <h2>批次记录</h2><article v-for="batch in items" :key="batch.id" class="batch">
      <p><strong>{{ labels[batch.state]||batch.state }}</strong> · {{ batch.items.length }} 本 · 选章 {{ batch.selection }} · 合并重复 {{ batch.duplicates }} 项</p><small>{{ batch.id }}</small>
      <p v-if="batch.error" class="message failure">{{ batch.error }}</p>
      <div class="batch-actions"><button v-if="['queued','paused'].includes(batch.state)" :disabled="working" @click="resume(batch.id)">继续批次</button><button v-if="['queued','running'].includes(batch.state)" :disabled="working" @click="stop(batch.id,true)">暂停批次</button><button v-if="['queued','running','paused'].includes(batch.state)" :disabled="working" @click="stop(batch.id,false)">取消批次</button></div>
      <ol><li v-for="item in batch.items" :key="item.position"><span>{{ item.input }} · {{ labels[item.state]||item.state }}</span><small v-if="item.job_id">任务 {{ item.job_id }}</small><p v-if="item.error" class="footnote">{{ item.error }}</p></li></ol>
    </article>
    <p class="footnote">网络/限流会暂停整批，按等待提示后手动继续。由批次管理的未完成任务，请在此操作；终结批次后可在下方逐任务检查和导出。</p>
  </section>
</template>
<style scoped>
textarea{display:block;width:100%;resize:vertical;border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:12px;font:inherit;color:var(--ink);margin:10px 0}h2{margin-bottom:12px}.batch{padding:12px 0;border-bottom:1px solid #e4e8dd}.batch small{display:block;color:#748069;overflow-wrap:anywhere}.batch li{padding:7px 0;overflow-wrap:anywhere}.batch-actions{display:flex;gap:8px;margin-top:12px}
</style>
