<script setup lang="ts">
import {computed,onUnmounted,ref,watch} from 'vue';
import AppIcon from './AppIcon.vue';
import ExportLocation from './ExportLocation.vue';
import {revealExport,retryAutomaticTxt,type AutoTxtStatus} from './api';
const props=defineProps<{bookId:string;saved:number;active?:boolean;status?:AutoTxtStatus}>();
const label=computed(()=>props.status?.receipt?.path.toLowerCase().endsWith('.docx')?'Word':'TXT');
const busy=ref(false);const error=ref('');const notice=ref('');let disposed=false;
async function act(retry=false){if(busy.value||retry&&(props.active||!props.saved))return;const owner=props.bookId;busy.value=true;error.value='';notice.value='';try{if(retry){await retryAutomaticTxt(owner);if(!disposed&&owner===props.bookId)notice.value='已请求按原范围与格式重新生成文件，无需重新下载。请稍后查看状态。';}else if(props.status?.export_id)await revealExport(props.status.export_id);}catch(e){if(!disposed&&owner===props.bookId)error.value=String(e);}finally{busy.value=false;}}
watch(()=>props.bookId,()=>{error.value='';notice.value='';});
onUnmounted(()=>{disposed=true;});
</script>
<template>
  <section class="automatic-txt" aria-label="自动导出文件" aria-live="polite">
    <h2><AppIcon name="file" :size="21" />自动导出文件</h2>
    <ExportLocation :book-id="bookId" />
    <template v-if="status?.state==='ready'&&status.receipt"><p><strong>{{ label }} 已生成</strong> · {{ status.receipt.saved_chapters }}/{{ status.receipt.total_chapters }} 章{{ status.receipt.partial?'（部分内容，标明保存章数）':'' }}</p><details><summary>查看 {{ label }} 文件完整路径</summary><code>{{ status.receipt.path }}</code></details><p v-if="active">这是上次生成的文件，当前任务结束后会自动更新版本。</p><div class="file-actions"><button class="primary" :disabled="busy||!status.export_id" @click="act()">打开 {{ label }} 所在文件夹</button><button :disabled="busy||active||!saved" @click="act(true)">重新生成 {{ label }}</button></div><p v-if="!active" class="recovery-note">文件已移动或删除？可从本地书库重新生成；你改过的文件会保留，不会被覆盖。</p></template>
    <template v-else-if="status?.state==='failed'"><p role="alert">文件生成失败，正文仍在书库：{{ status.error }}</p><button :disabled="busy||active||!saved" @click="act(true)">重试生成文件</button></template>
    <p v-else-if="!saved">保存到正文后会按所选格式和范围自动生成文件。</p>
    <p v-else-if="active">正在保存正文，任务结束后按所选格式自动生成文件。</p>
    <p v-else role="status">{{ status?.state==='generating'?'正在生成文件…':'正文已保存，后台正在准备文件…' }}</p>
    <p v-if="error" role="alert">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
    <details class="automatic-note"><summary>想要其他格式或查看旧文件？</summary><p>在“其他格式与文件记录”中选择。所选格式会自动生成，补齐或修订后保留新旧版本。</p></details>
  </section>
</template>
<style scoped>
.automatic-txt{margin:20px 0;padding:22px 24px;border:1px solid var(--border);background:var(--surface);border-radius:12px;min-width:0}.automatic-txt h2{display:flex;align-items:center;gap:10px;font-size:18px;margin-top:0}.automatic-txt h2 .app-icon{color:var(--accent)}.automatic-txt p{font-size:13px;line-height:1.9;overflow-wrap:anywhere}.automatic-txt code{display:block;font-size:12px;overflow-wrap:anywhere;white-space:pre-wrap;margin:12px 0}.automatic-txt .automatic-note{color:var(--muted);margin-bottom:0}.file-actions{display:flex;flex-wrap:wrap;gap:10px;margin-top:16px}.automatic-txt summary{font-size:12px;color:var(--muted)}.automatic-txt .recovery-note{font-size:12px;color:var(--muted)}
</style>
