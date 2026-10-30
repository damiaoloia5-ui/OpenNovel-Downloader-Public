<script setup lang="ts">
import {ref} from 'vue';
import {revealExport,retryAutomaticTxt,type AutoTxtStatus} from './api';
const props=defineProps<{bookId:string;saved:number;active?:boolean;status?:AutoTxtStatus}>();
const busy=ref(false);const error=ref('');
async function act(retry=false){if(busy.value)return;busy.value=true;error.value='';try{if(retry)await retryAutomaticTxt(props.bookId);else if(props.status?.export_id)await revealExport(props.status.export_id);}catch(e){error.value=String(e);}finally{busy.value=false;}}
</script>
<template>
  <section class="automatic-txt" aria-label="自动 TXT 文件" aria-live="polite">
    <h2>自动 TXT 文件</h2>
    <template v-if="status?.state==='ready'&&status.receipt"><p><strong>TXT 已生成</strong> · {{ status.receipt.saved_chapters }}/{{ status.receipt.total_chapters }} 章{{ status.receipt.partial?'（部分内容，标明保存章数）':'' }}</p><details><summary>查看 TXT 文件完整路径</summary><code>{{ status.receipt.path }}</code></details><p v-if="active">这是上次生成的文件，当前任务结束后会自动更新版本。</p><button :disabled="busy||!status.export_id" @click="act()">打开 TXT 所在文件夹</button></template>
    <template v-else-if="status?.state==='failed'"><p role="alert">TXT 生成失败，正文仍在书库：{{ status.error }}</p><button :disabled="busy||active||!saved" @click="act(true)">重试生成 TXT</button></template>
    <p v-else-if="!saved">保存到正文后会自动生成 TXT，无需选格式或手动导出。</p>
    <p v-else-if="active">正在保存正文，任务结束后自动生成 TXT。</p>
    <p v-else role="status">{{ status?.state==='generating'?'正在生成 TXT…':'正文已保存，后台正在准备 TXT…' }}</p>
    <p v-if="error" role="alert">{{ error }}</p>
    <details class="automatic-note"><summary>想要 EPUB 或查看旧文件？</summary><p>在“其他格式与文件记录”中选择。TXT 会自动生成，补齐或修订后保留新旧版本。</p></details>
  </section>
</template>
<style scoped>
.automatic-txt{margin:20px 0;padding:18px;border:1px solid #cbd4c0;background:#f1f5eb;border-radius:8px;min-width:0}.automatic-txt h2{font-size:18px;margin-top:0}.automatic-txt p{font-size:13px;line-height:1.9;overflow-wrap:anywhere}.automatic-txt code{display:block;font-size:12px;overflow-wrap:anywhere;white-space:pre-wrap;margin:12px 0}.automatic-txt .automatic-note{color:#526048;margin-bottom:0}
</style>
