<script setup lang="ts">
import {ref,watch} from 'vue';
import {bookDetails,exportBook,revealExport,type ExportRecord,type Receipt} from './api';
const props=defineProps<{bookId:string;saved:number;total:number;title?:string}>();
const emit=defineEmits<{exported:[]}>();
const format=ref<'txt'|'epub'>('txt');const busy=ref(false);const error=ref('');const receipt=ref<Receipt|null>(null);const record=ref<ExportRecord|null>(null);
watch(()=>props.bookId,()=>{receipt.value=null;record.value=null;error.value='';});
async function save(){
  if(busy.value||!props.saved)return;
  const id=props.bookId;busy.value=true;error.value='';receipt.value=null;record.value=null;
  try {
    const result=await exportBook(id,format.value);
    if(id!==props.bookId)return;
    receipt.value=result;emit('exported');
    try {const details=await bookDetails(id);if(id===props.bookId)record.value=details.exports.find(r=>r.path===result.path)??null;} catch { /* A saved file remains valid if the record refresh fails. */ }
  } catch(e) {
    if(id!==props.bookId)return;
    error.value=String(e);
    try {const details=await bookDetails(id);if(id===props.bookId)record.value=details.exports.find(r=>r.job_id===null&&r.format===format.value&&r.saved===props.saved&&r.total===props.total)??null;} catch { /* Preserve the original export error. */ }
  } finally {busy.value=false;}
}
async function openFolder(){if(!record.value||busy.value)return;busy.value=true;error.value='';try{await revealExport(record.value.id);}catch(e){error.value=String(e);}finally{busy.value=false;}}
</script>
<template>
  <section class="export-actions" aria-label="导出本地文件">
    <h3>{{ title||'导出到电脑' }}</h3>
    <p>{{ saved?`已保存 ${saved}/${total} 章，可以导出。`:'先保存至少一章正文，才能生成文件。' }}<span v-if="saved&&saved<total"> 尚有缺章，文件会明确标注。</span></p>
    <div class="export-controls"><label>文件格式<select v-model="format" :disabled="busy"><option value="txt">TXT · 普通文本，推荐</option><option value="epub">EPUB · 电子书阅读器</option></select></label><button class="primary" :disabled="busy||!saved" @click="save">{{ busy?'正在处理…':'导出文件' }}</button></div>
    <div v-if="receipt" class="export-result" role="status"><strong>文件已生成</strong><p>{{ receipt.saved_chapters }}/{{ receipt.total_chapters }} 章{{ receipt.partial?'（部分内容）':'' }}</p><code>{{ receipt.path }}</code></div>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p>
    <div v-if="record" class="export-location"><p v-if="!receipt">可查看同章数的已有导出，旧文件不会覆盖：<code>{{ record.path }}</code></p><button :disabled="busy" @click="openFolder">打开文件所在文件夹</button></div>
  </section>
</template>
<style scoped>
.export-actions{margin-top:24px;padding-top:20px;border-top:1px solid #d4ddca}.export-actions h3{margin:0;font-size:18px}.export-actions p{font-size:13px;line-height:1.8;color:#526048}.export-controls{display:flex;flex-wrap:wrap;align-items:end;gap:12px}.export-controls label{display:grid;gap:8px;font-size:13px}.export-controls select{font:inherit;padding:11px;border:1px solid #bdc9b1;border-radius:5px;background:#fafbf7;color:#334431}.export-result{margin-top:16px;background:#eaf1e0;padding:16px;border-radius:6px}.export-result p{margin:6px 0}.export-actions code{display:block;overflow-wrap:anywhere;white-space:pre-wrap;font-size:12px;line-height:1.8}.export-location{margin-top:12px}
</style>
